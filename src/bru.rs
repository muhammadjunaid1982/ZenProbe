//! `.bru` file format, mirroring Bruno's `@usebruno/lang` v2 grammar (bruToJson / jsonToBru).
//! Blocks ZenProbe does not model are kept verbatim so saving never loses data.

use crate::model::{Auth, Environment, FolderSettings, Message, Pair, Request};

#[derive(Clone, Debug, PartialEq)]
pub enum Body {
    Text(String),
    Dict(Vec<Pair>),
    List(Vec<String>),
    /// Single-line top-level entry such as `color: #fff` in environment files.
    Line,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub name: String,
    pub body: Body,
    /// Exact source text of the block, re-emitted for blocks ZenProbe does not model.
    pub raw: String,
}

const TEXT_BLOCKS: &[&str] = &[
    "body", "body:json", "body:text", "body:xml", "body:sparql", "body:graphql", "body:graphql:vars",
    "script:pre-request", "script:post-response", "script:grpc:before-call-start",
    "script:grpc:before-message-send", "script:grpc:after-message-receive", "script:grpc:after-call-end",
    "tests", "docs", "example",
];

pub const METHODS: &[&str] = &["get", "post", "put", "patch", "delete", "head", "options", "trace", "connect"];

pub fn parse_blocks(src: &str) -> Result<Vec<Block>, String> {
    let lines: Vec<&str> = src.lines().collect();
    let mut blocks = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let header = lines[i].trim_end();
        if header.trim().is_empty() {
            i += 1;
            continue;
        }
        let (name, close) = if let Some(h) = header.strip_suffix('{') {
            (h.trim(), '}')
        } else if let Some(h) = header.strip_suffix('[') {
            (h.trim(), ']')
        } else {
            let name = header.split(':').next().unwrap_or("").trim();
            blocks.push(Block { name: name.into(), body: Body::Line, raw: header.into() });
            i += 1;
            continue;
        };
        let end = (i + 1..lines.len())
            .find(|&j| lines[j].starts_with(close))
            .ok_or_else(|| format!("line {}: block `{name}` is never closed", i + 1))?;
        let inner = &lines[i + 1..end];
        let body = if close == ']' {
            Body::List(list_items(inner))
        } else if TEXT_BLOCKS.contains(&name) {
            Body::Text(text_block(inner))
        } else {
            Body::Dict(parse_pairs(inner).map_err(|e| format!("block `{name}`: {e}"))?)
        };
        blocks.push(Block { name: name.into(), body, raw: lines[i..=end].join("\n") });
        i = end + 1;
    }
    Ok(blocks)
}

fn list_items(lines: &[&str]) -> Vec<String> {
    lines.iter().flat_map(|l| l.split(',')).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
}

fn text_block(lines: &[&str]) -> String {
    let start = lines.iter().position(|l| !l.is_empty()).unwrap_or(lines.len());
    lines[start..].iter().map(|l| l.strip_prefix("  ").unwrap_or(l)).collect::<Vec<_>>().join("\n")
}

/// `@name(...)` or `@name` on its own line; `@name: value` is a local-var pair instead.
fn is_annotation(t: &str) -> bool {
    let Some(rest) = t.strip_prefix('@') else { return false };
    let end = rest.find(['(', ' ', '\t', ':']).unwrap_or(rest.len());
    end > 0 && !rest[end..].trim_start().starts_with(':')
}

fn split_key(t: &str) -> Result<(String, &str), String> {
    let (disabled, s) = match t.strip_prefix('~') {
        Some(s) => ("~", s),
        None => ("", t),
    };
    if let Some(q) = s.strip_prefix('"') {
        let mut key = String::from(disabled);
        let mut chars = q.char_indices();
        while let Some((idx, c)) = chars.next() {
            match c {
                '\\' if q[idx + 1..].starts_with('"') => {
                    key.push('"');
                    chars.next();
                }
                '"' => {
                    let rest = q[idx + 1..].trim_start();
                    let rest = rest.strip_prefix(':').ok_or(format!("expected `:` after key in `{t}`"))?;
                    return Ok((key, rest));
                }
                c => key.push(c),
            }
        }
        return Err(format!("unterminated quoted key in `{t}`"));
    }
    let colon = t.find(':').ok_or(format!("expected `key: value`, got `{t}`"))?;
    Ok((t[..colon].trim().to_string(), &t[colon + 1..]))
}

/// Port of utils.parseAnnotationMultilineTextBlock: drop edge blank lines, strip common indent.
fn dedent_block(content: &str) -> String {
    if !content.contains('\n') {
        return content.to_string();
    }
    let mut lines: Vec<&str> = content.split('\n').map(|l| l.trim_end_matches('\r')).collect();
    if lines.first() == Some(&"") {
        lines.remove(0);
    }
    if lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
    }
    let min = lines.iter().filter(|l| !l.trim().is_empty()).map(|l| l.len() - l.trim_start().len()).min().unwrap_or(0);
    lines.iter().map(|l| if l.trim().is_empty() { "" } else { &l[min..] }).collect::<Vec<_>>().join("\n")
}

/// Value of `@description(...)`, or None for other annotations.
fn description_of(ann: &str) -> Option<String> {
    let arg = ann.strip_prefix("@description")?;
    let inner = arg.trim().strip_prefix('(')?.strip_suffix(')')?;
    Some(if let Some(b) = inner.strip_prefix("'''").and_then(|s| s.strip_suffix("'''")) {
        dedent_block(b).replace("\\'\\'\\'", "'''").replace("\\\\'", "\\'")
    } else if let Some(s) = inner.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')) {
        s.to_string()
    } else if let Some(s) = inner.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
        let mut out = String::new();
        let mut it = s.chars();
        while let Some(c) = it.next() {
            if c == '\\' {
                match it.next() {
                    Some('n') => out.push('\n'),
                    Some('r') => out.push('\r'),
                    Some('t') => out.push('\t'),
                    Some(o) => out.push(o),
                    None => out.push('\\'),
                }
            } else {
                out.push(c);
            }
        }
        out
    } else {
        inner.to_string()
    })
}

fn description_annotation(d: &str) -> String {
    if d.contains('\n') {
        let escaped = d.replace("\\'", "\\\\'").replace("'''", "\\'\\'\\'");
        format!("@description('''\n{}\n''')", indent(&escaped, 1))
    } else if d.contains('\'') {
        format!("@description(\"{}\")", d.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        format!("@description('{d}')")
    }
}

fn parse_pairs(lines: &[&str]) -> Result<Vec<Pair>, String> {
    let mut pairs = Vec::new();
    let mut annotations = Vec::new();
    let mut k = 0;
    while k < lines.len() {
        let t = lines[k].trim();
        k += 1;
        if t.is_empty() {
            continue;
        }
        if is_annotation(t) {
            let mut ann = vec![lines[k - 1].strip_prefix("  ").unwrap_or(lines[k - 1]).to_string()];
            // multiline annotation argument: @name(''' ... ''')
            if t.matches("'''").count() == 1 {
                while k < lines.len() {
                    ann.push(lines[k].strip_prefix("  ").unwrap_or(lines[k]).to_string());
                    k += 1;
                    if lines[k - 1].contains("'''") {
                        break;
                    }
                }
            }
            annotations.push(ann.join("\n"));
            continue;
        }
        let (key, rest) = split_key(t)?;
        let rest = rest.trim();
        let mut pair = Pair::default();
        for a in std::mem::take(&mut annotations) {
            match description_of(a.trim_start()) {
                Some(d) => pair.description = d,
                None => pair.annotations.push(a),
            }
        }
        if rest.starts_with("'''") {
            let mut body = Vec::new();
            let mut suffix = "";
            while k < lines.len() {
                let l = lines[k];
                k += 1;
                if let Some(after) = l.trim_start().strip_prefix("'''") {
                    suffix = after.trim();
                    break;
                }
                // Bruno slices exactly 4 characters off every multiline line
                body.push(l.chars().skip(4).collect::<String>());
            }
            pair.value = body.join("\n").trim().to_string();
            if !suffix.is_empty() {
                pair.value = format!("{} {suffix}", pair.value);
            }
        } else if rest == "[" {
            let end = (k..lines.len()).find(|&j| lines[j].trim() == "]").ok_or("unterminated list")?;
            pair.list = Some(list_items(&lines[k..end]));
            k = end + 1;
        } else {
            pair.value = rest.to_string();
        }
        pair.enabled = !key.starts_with('~');
        pair.key = key.strip_prefix('~').unwrap_or(&key).to_string();
        pairs.push(pair);
    }
    Ok(pairs)
}

// ---------- writing ----------

pub fn indent(s: &str, levels: usize) -> String {
    let pad = "  ".repeat(levels);
    s.split('\n').map(|l| format!("{pad}{l}")).collect::<Vec<_>>().join("\n")
}

fn key_string(k: &str) -> String {
    if k.contains([':', '"', '{', '}', ' ']) { format!("\"{}\"", k.replace('"', "\\\"")) } else { k.to_string() }
}

fn value_string(v: &str) -> String {
    if v.contains('\n') { format!("'''\n{}\n'''", indent(v, 1)) } else { v.to_string() }
}

/// One pair, unindented (block writer indents it). `quote` mirrors getKeyString usage.
fn pair_string(p: &Pair, quote: bool, value: &str) -> String {
    let mut out = String::new();
    if !p.description.is_empty() {
        out += &description_annotation(&p.description);
        out.push('\n');
    }
    for a in &p.annotations {
        out += a;
        out.push('\n');
    }
    if !p.enabled {
        out.push('~');
    }
    out += &if quote { key_string(&p.key) } else { p.key.clone() };
    out += ": ";
    if let Some(items) = &p.list {
        out += "[\n";
        for it in items {
            out += &format!("  {it}\n");
        }
        out.push(']');
    } else {
        out += value;
    }
    out
}

fn dict(name: &str, lines: Vec<String>) -> String {
    if lines.is_empty() { format!("{name} {{\n}}\n\n") } else { format!("{name} {{\n{}\n}}\n\n", indent(&lines.join("\n"), 1)) }
}

pub fn dict_block(name: &str, pairs: &[Pair], quote: bool) -> String {
    dict(name, pairs.iter().map(|p| pair_string(p, quote, &value_string(&p.value))).collect())
}

/// jsonToBru writes enabled entries first, then disabled ones.
fn dict_block_sorted(name: &str, pairs: &[Pair], quote: bool) -> String {
    let sorted: Vec<Pair> = pairs.iter().filter(|p| p.enabled).chain(pairs.iter().filter(|p| !p.enabled)).cloned().collect();
    dict_block(name, &sorted, quote)
}

fn text(name: &str, text: &str) -> String {
    format!("{name} {{\n{}\n}}\n\n", indent(text, 1))
}

fn get(pairs: &[Pair], key: &str) -> String {
    pairs.iter().find(|p| p.key == key).map(|p| p.value.clone()).unwrap_or_default()
}

fn split_content_type(v: &str) -> (String, String) {
    if let Some(i) = v.rfind("@contentType(")
        && v.ends_with(')')
    {
        return (v[..i].trim().to_string(), v[i + 13..v.len() - 1].trim().to_string());
    }
    (v.trim().to_string(), String::new())
}

fn auth_blocks(auth: &mut Auth, name: &str, p: &[Pair]) -> bool {
    match name {
        "auth:basic" => {
            auth.username = get(p, "username");
            auth.password = get(p, "password");
        }
        "auth:bearer" => auth.token = get(p, "token"),
        _ => return false,
    }
    true
}

fn write_auth(auth: &Auth) -> String {
    let mut s = String::new();
    if auth.mode == "basic" || !auth.username.is_empty() || !auth.password.is_empty() {
        s += &format!("auth:basic {{\n  username: {}\n  password: {}\n}}\n\n", auth.username, auth.password);
    }
    if auth.mode == "bearer" || !auth.token.is_empty() {
        s += &format!("auth:bearer {{\n  token: {}\n}}\n\n", auth.token);
    }
    s
}

fn message_from(p: &[Pair]) -> Message {
    Message {
        name: get(p, "name"),
        kind: get(p, "type"),
        content: get(p, "content"),
        selected: get(p, "selected") == "true",
    }
}

fn finish(mut s: String) -> String {
    s.truncate(s.trim_end_matches('\n').len());
    s.push('\n');
    s
}

// ---------- requests ----------

pub fn parse_request(src: &str) -> Result<Request, String> {
    let mut r = Request { kind: "http".into(), seq: "1".into(), ..Default::default() };
    for b in parse_blocks(src)? {
        let name = b.name.as_str();
        match (name, &b.body) {
            ("meta", Body::Dict(p)) => {
                for pair in p {
                    match pair.key.as_str() {
                        "name" => r.name = pair.value.clone(),
                        "type" => r.kind = pair.value.clone(),
                        "seq" => r.seq = pair.value.clone(),
                        "tags" if pair.list.is_some() => r.tags = pair.list.clone().unwrap_or_default(),
                        _ => r.keep.bru_meta.push(pair.clone()),
                    }
                }
            }
            (m, Body::Dict(p)) if METHODS.contains(&m) || m == "http" => {
                r.method = if m == "http" { get(p, "method") } else { m.to_string() };
                r.url = get(p, "url");
                r.body_mode = get(p, "body");
                r.auth.mode = get(p, "auth");
            }
            ("grpc" | "ws", Body::Dict(p)) => {
                r.url = get(p, "url");
                r.method = get(p, "method");
                r.body_mode = get(p, "body");
                r.auth.mode = get(p, "auth");
                r.proto_path = get(p, "protoPath");
                r.grpc_method_type = get(p, "methodType");
            }
            ("params:query", Body::Dict(p)) => r.query = p.clone(),
            ("params:path", Body::Dict(p)) => r.path_params = p.clone(),
            ("headers" | "metadata", Body::Dict(p)) => r.headers = p.clone(),
            (n, Body::Dict(p)) if auth_blocks(&mut r.auth, n, p) => {}
            ("body:json", Body::Text(t)) => r.body_json = t.clone(),
            ("body:text", Body::Text(t)) => r.body_text = t.clone(),
            ("body:xml", Body::Text(t)) => r.body_xml = t.clone(),
            ("body:sparql", Body::Text(t)) => r.body_sparql = t.clone(),
            ("body:graphql", Body::Text(t)) => r.gql_query = t.clone(),
            ("body:graphql:vars", Body::Text(t)) => r.gql_vars = t.clone(),
            ("body:form-urlencoded", Body::Dict(p)) => r.form = p.clone(),
            ("body:multipart-form", Body::Dict(p)) => {
                r.multipart = p
                    .iter()
                    .map(|x| {
                        let (v, ct) = split_content_type(&x.value);
                        let mut part = Pair { content_type: ct, ..x.clone() };
                        match v.strip_prefix("@file(").and_then(|f| f.strip_suffix(')')) {
                            Some(files) => {
                                part.is_file = true;
                                part.value = files.to_string();
                            }
                            None => part.value = v,
                        }
                        part
                    })
                    .collect()
            }
            ("body:file", Body::Dict(p)) => {
                r.files = p
                    .iter()
                    .map(|x| {
                        let (v, ct) = split_content_type(&x.value);
                        let path = v.strip_prefix("@file(").and_then(|f| f.strip_suffix(')')).unwrap_or(&v).to_string();
                        Pair { key: "file".into(), value: path, content_type: ct, ..x.clone() }
                    })
                    .collect()
            }
            ("body:grpc" | "body:ws", Body::Dict(p)) => r.messages.push(message_from(p)),
            ("vars:pre-request", Body::Dict(p)) => r.vars_req = p.clone(),
            ("vars:post-response", Body::Dict(p)) => r.vars_res = p.clone(),
            ("assert", Body::Dict(p)) => r.asserts = p.clone(),
            ("script:pre-request", Body::Text(t)) => r.script_req = t.clone(),
            ("script:post-response", Body::Text(t)) => r.script_res = t.clone(),
            ("tests", Body::Text(t)) => r.tests = t.clone(),
            ("docs", Body::Text(t)) => r.docs = t.clone(),
            ("settings", Body::Dict(p)) => r.settings = p.clone(),
            _ => r.keep.bru_blocks.push(b),
        }
    }
    let method_default = if r.kind == "grpc" || r.kind == "ws" { "" } else { "get" };
    for (field, default) in [(&mut r.method, method_default), (&mut r.body_mode, "none"), (&mut r.auth.mode, "none")] {
        if field.is_empty() {
            *field = default.into();
        }
    }
    Ok(r)
}

/// Serialize in jsonToBru block order; unmodelled blocks are appended verbatim.
pub fn write_request(r: &Request) -> String {
    let mut s = format!("meta {{\n  name: {}\n  type: {}\n  seq: {}\n", r.name, r.kind, r.seq);
    for p in &r.keep.bru_meta {
        s += &indent(&pair_string(p, false, &value_string(&p.value)), 1);
        s.push('\n');
    }
    if !r.tags.is_empty() {
        s += &format!("  tags: [\n{}\n  ]\n", r.tags.iter().map(|t| format!("    {t}")).collect::<Vec<_>>().join("\n"));
    }
    s += "}\n\n";

    match r.kind.as_str() {
        "grpc" => {
            s += &format!("grpc {{\n  url: {}", r.url);
            for (k, v) in [("method", &r.method), ("body", &r.body_mode), ("protoPath", &r.proto_path), ("auth", &r.auth.mode), ("methodType", &r.grpc_method_type)] {
                if !v.is_empty() {
                    s += &format!("\n  {k}: {v}");
                }
            }
            s += "\n}\n\n";
        }
        "ws" => {
            s += &format!("ws {{\n  url: {}", r.url);
            for (k, v) in [("body", &r.body_mode), ("auth", &r.auth.mode)] {
                if !v.is_empty() {
                    s += &format!("\n  {k}: {v}");
                }
            }
            s += "\n}\n\n";
        }
        _ => {
            let url = if r.url.contains('\n') { format!("'''\n{}\n'''", indent(&r.url, 2)) } else { r.url.clone() };
            s += &if METHODS.contains(&r.method.as_str()) { format!("{} {{", r.method) } else { format!("http {{\n  method: {}", r.method) };
            s += &format!("\n  url: {url}\n  body: {}\n  auth: {}\n}}\n\n", r.body_mode, r.auth.mode);
        }
    }

    if !r.query.is_empty() {
        s += &dict_block_sorted("params:query", &r.query, true);
    }
    if !r.path_params.is_empty() {
        s += &dict_block("params:path", &r.path_params, false);
    }
    if !r.headers.is_empty() {
        s += &dict_block_sorted(if r.kind == "grpc" { "metadata" } else { "headers" }, &r.headers, true);
    }
    s += &write_auth(&r.auth);
    for (name, t) in [("body:json", &r.body_json), ("body:text", &r.body_text), ("body:xml", &r.body_xml), ("body:sparql", &r.body_sparql)] {
        if !t.is_empty() {
            s += &text(name, t);
        }
    }
    if !r.form.is_empty() {
        s += &dict_block_sorted("body:form-urlencoded", &r.form, true);
    }
    if !r.multipart.is_empty() {
        let parts: Vec<Pair> = r.multipart.iter().filter(|p| p.enabled).chain(r.multipart.iter().filter(|p| !p.enabled)).cloned().collect();
        s += &dict(
            "body:multipart-form",
            parts
                .iter()
                .map(|p| {
                    let ct = if p.content_type.is_empty() { String::new() } else { format!(" @contentType({})", p.content_type) };
                    let v = if p.is_file { format!("@file({})", p.value) } else { value_string(&p.value) };
                    pair_string(p, true, &format!("{v}{ct}"))
                })
                .collect(),
        );
    }
    if !r.files.is_empty() {
        let files: Vec<Pair> = r.files.iter().filter(|p| p.enabled).chain(r.files.iter().filter(|p| !p.enabled)).cloned().collect();
        s += &dict(
            "body:file",
            files
                .iter()
                .map(|p| {
                    let ct = if p.content_type.is_empty() { String::new() } else { format!(" @contentType({})", p.content_type) };
                    pair_string(&Pair { key: "file".into(), ..p.clone() }, false, &format!("@file({}){ct}", p.value))
                })
                .collect(),
        );
    }
    if !r.gql_query.is_empty() {
        s += &text("body:graphql", &r.gql_query);
    }
    if !r.gql_vars.is_empty() {
        s += &text("body:graphql:vars", &r.gql_vars);
    }
    for m in &r.messages {
        let content = format!("content: '''\n{}\n'''", indent(&m.content, 1));
        if r.kind == "grpc" {
            s += &format!("body:grpc {{\n  name: {}\n{}\n}}\n\n", m.name, indent(&content, 1));
        } else {
            let mut b = format!("body:ws {{\n  name: {}\n", m.name);
            if !m.kind.is_empty() {
                b += &format!("  type: {}\n", m.kind);
            }
            if m.selected {
                b += "  selected: true\n";
            }
            s += &format!("{b}{}\n}}\n\n", indent(&content, 1));
        }
    }
    s += &write_vars(&r.vars_req, &r.vars_res);
    if !r.asserts.is_empty() {
        s += &dict_block_sorted("assert", &r.asserts, false);
    }
    for (name, t) in [("script:pre-request", &r.script_req), ("script:post-response", &r.script_res), ("tests", &r.tests)] {
        if !t.is_empty() {
            s += &text(name, t);
        }
    }
    if !r.settings.is_empty() {
        s += &dict_block("settings", &r.settings, false);
    }
    for b in &r.keep.bru_blocks {
        s += &b.raw;
        s += "\n\n";
    }
    if !r.docs.is_empty() {
        s += &text("docs", &r.docs);
    }
    finish(s)
}

fn write_vars(req: &[Pair], res: &[Pair]) -> String {
    let mut s = String::new();
    if !req.is_empty() {
        s += &dict_block_sorted("vars:pre-request", req, false);
    }
    if !res.is_empty() {
        s += &dict_block_sorted("vars:post-response", res, false);
    }
    s
}

// ---------- collection.bru / folder.bru ----------

pub fn parse_folder(src: &str) -> Result<FolderSettings, String> {
    let mut f = FolderSettings { auth: Auth { mode: "none".into(), ..Default::default() }, ..Default::default() };
    for b in parse_blocks(src)? {
        match (b.name.as_str(), &b.body) {
            ("meta", Body::Dict(p)) => {
                f.name = get(p, "name");
                f.seq = get(p, "seq");
                f.keep.bru_meta = p.iter().filter(|x| x.key != "name" && x.key != "seq").cloned().collect();
            }
            ("headers", Body::Dict(p)) => f.headers = p.clone(),
            ("auth", Body::Dict(p)) => f.auth.mode = get(p, "mode"),
            (n, Body::Dict(p)) if auth_blocks(&mut f.auth, n, p) => {}
            ("vars:pre-request", Body::Dict(p)) => f.vars_req = p.clone(),
            ("vars:post-response", Body::Dict(p)) => f.vars_res = p.clone(),
            ("script:pre-request", Body::Text(t)) => f.script_req = t.clone(),
            ("script:post-response", Body::Text(t)) => f.script_res = t.clone(),
            ("tests", Body::Text(t)) => f.tests = t.clone(),
            ("docs", Body::Text(t)) => f.docs = t.clone(),
            _ => f.keep.bru_blocks.push(b),
        }
    }
    Ok(f)
}

pub fn write_folder(f: &FolderSettings, is_collection: bool) -> String {
    let mut s = String::new();
    if !is_collection && (!f.name.is_empty() || !f.keep.bru_meta.is_empty()) {
        let mut meta = vec![Pair::new("name", &f.name)];
        if !f.seq.is_empty() {
            meta.push(Pair::new("seq", &f.seq));
        }
        meta.extend(f.keep.bru_meta.iter().cloned());
        s += &dict_block("meta", &meta, false);
    }
    if !f.headers.is_empty() {
        s += &dict_block_sorted("headers", &f.headers, true);
    }
    if !f.auth.mode.is_empty() && !(f.auth.mode == "none" && f.auth.username.is_empty() && f.auth.token.is_empty()) {
        s += &format!("auth {{\n  mode: {}\n}}\n\n", f.auth.mode);
    }
    s += &write_auth(&f.auth);
    s += &write_vars(&f.vars_req, &f.vars_res);
    for (name, t) in [("script:pre-request", &f.script_req), ("script:post-response", &f.script_res), ("tests", &f.tests)] {
        if !t.is_empty() {
            s += &text(name, t);
        }
    }
    for b in &f.keep.bru_blocks {
        s += &b.raw;
        s += "\n\n";
    }
    if !f.docs.is_empty() {
        s += &text("docs", &f.docs);
    }
    finish(s)
}

// ---------- environments ----------

pub fn parse_env(src: &str) -> Result<Environment, String> {
    let mut env = Environment::default();
    for b in parse_blocks(src)? {
        match (b.name.as_str(), &b.body) {
            ("vars", Body::Dict(p)) => env.vars = p.clone(),
            ("vars:secret", Body::List(items)) => env.secrets = items.clone(),
            _ => env.keep.bru_blocks.push(b),
        }
    }
    Ok(env)
}

pub fn write_env(env: &Environment) -> String {
    let mut s = dict_block("vars", &env.vars, false);
    if !env.secrets.is_empty() {
        s += &format!("vars:secret [\n{}\n]\n\n", env.secrets.iter().map(|n| format!("  {n}")).collect::<Vec<_>>().join(",\n"));
    }
    for b in &env.keep.bru_blocks {
        s += &b.raw;
        s += "\n\n";
    }
    finish(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    pub const SAMPLE: &str = r#"meta {
  name: Create user
  type: http
  seq: 2
  tags: [
    smoke
  ]
}

post {
  url: {{baseUrl}}/users/:id?verbose=true
  body: multipartForm
  auth: bearer
}

params:query {
  verbose: true
  ~debug: 1
}

params:path {
  id: 42
}

headers {
  @description('trace header')
  x-trace: abc
  "weird key": v
}

auth:bearer {
  token: {{token}}
}

body:json {
  {
    "name": "John"
  }
}

body:multipart-form {
  name: John @contentType(text/plain)
  avatar: @file(a.png|b.png)
  ~old: x
}

body:file {
  file: @file(data.bin) @contentType(application/octet-stream)
  ~file: @file(other.bin)
}

vars:pre-request {
  @local: 1
}

vars:post-response {
  userId: res.body.id
}

assert {
  res.status: eq 201
}

settings {
  encodeUrl: true
  timeout: 0
}

auth:oauth2 {
  grant_type: client_credentials
}

tests {
  test("ok", () => {});
}

docs {
  multi
  '''
}
"#;

    #[test]
    fn parses_sample() {
        let r = parse_request(SAMPLE).unwrap();
        assert_eq!(r.name, "Create user");
        assert_eq!(r.tags, vec!["smoke"]);
        assert_eq!(r.method, "post");
        assert_eq!(r.query.len(), 2);
        assert!(!r.query[1].enabled);
        assert_eq!(r.headers[0].description, "trace header");
        assert!(r.headers[0].annotations.is_empty());
        assert_eq!(r.headers[1].key, "weird key");
        assert_eq!(r.multipart[0].content_type, "text/plain");
        assert_eq!(r.multipart[0].value, "John");
        assert!(r.multipart[1].is_file);
        assert_eq!(r.multipart[1].value, "a.png|b.png");
        assert_eq!(r.files[0].value, "data.bin");
        assert!(!r.files[1].enabled);
        assert_eq!(r.settings.len(), 2);
        assert_eq!(r.keep.bru_blocks[0].name, "auth:oauth2");
    }

    #[test]
    fn round_trips() {
        let r = parse_request(SAMPLE).unwrap();
        assert_eq!(r, parse_request(&write_request(&r)).unwrap());
    }

    #[test]
    fn grpc_and_ws() {
        let src = "meta {\n  name: g\n  type: grpc\n  seq: 1\n}\n\ngrpc {\n  url: localhost:50051\n  method: /pkg.Svc/Hello\n  body: grpc\n  auth: none\n  methodType: unary\n}\n\nmetadata {\n  x: 1\n}\n\nbody:grpc {\n  name: message 1\n  content: '''\n    {\n      \"a\": 1\n    }\n  '''\n}\n";
        let r = parse_request(src).unwrap();
        assert_eq!(r.method, "/pkg.Svc/Hello");
        assert_eq!(r.messages[0].content, "{\n  \"a\": 1\n}");
        assert_eq!(r.headers[0].key, "x");
        assert_eq!(r, parse_request(&write_request(&r)).unwrap());

        let mut ws = crate::model::Request::new("w", 1, "ws");
        ws.url = "ws://localhost/x".into();
        ws.messages[0].kind = "text".into();
        assert_eq!(ws.messages, parse_request(&write_request(&ws)).unwrap().messages);
    }

    #[test]
    fn descriptions() {
        for d in ["plain", "it's", "multi\nline 'q' '''"] {
            let p = Pair { description: d.into(), ..Pair::new("k", "v") };
            let s = dict_block("headers", &[p.clone()], true);
            let Body::Dict(back) = &parse_blocks(&s).unwrap()[0].body else { panic!() };
            assert_eq!(back[0].description, d, "{s}");
        }
    }

    #[test]
    fn folder_and_env() {
        let src = "meta {\n  name: Users\n  seq: 3\n}\n\nheaders {\n  x: 1\n}\n\nauth {\n  mode: bearer\n}\n\nauth:bearer {\n  token: t\n}\n\nscript:pre-request {\n  bru.setVar('a', 1);\n}\n";
        let f = parse_folder(src).unwrap();
        assert_eq!((f.name.as_str(), f.auth.mode.as_str(), f.auth.token.as_str()), ("Users", "bearer", "t"));
        assert_eq!(f, parse_folder(&write_folder(&f, false)).unwrap());

        let e = parse_env("vars {\n  host: localhost\n}\nvars:secret [\n  token,\n  pass\n]\ncolor: #ff0000\n").unwrap();
        assert_eq!(e.secrets, vec!["token", "pass"]);
        assert_eq!(e, parse_env(&write_env(&e)).unwrap());
    }
}

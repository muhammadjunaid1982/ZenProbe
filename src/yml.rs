//! OpenCollection YAML format (opencollection.yml, *.yml requests, folder.yml, environments/*.yml),
//! mirroring @usebruno/filestore `formats/yml`. Writing edits the original document in place so
//! examples, config and other keys ZenProbe does not model are preserved.

use crate::model::{Auth, Environment, FolderSettings, Keep, Message, Pair, Request};
use serde_yaml::{Mapping, Value};

fn s(v: &Value) -> String {
    match v {
        Value::String(x) => x.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => String::new(),
        other => serde_yaml::to_string(other).unwrap_or_default().trim_end().to_string(),
    }
}

fn get<'a>(v: &'a Value, key: &str) -> &'a Value {
    v.get(key).unwrap_or(&Value::Null)
}

fn text_or_content(v: &Value) -> String {
    match v {
        Value::Mapping(_) => s(get(v, "content")),
        other => s(other),
    }
}

fn str_v(x: &str) -> Value {
    Value::String(x.to_string())
}

/// `{type, data}` typed variable values become a `@type` annotation, like .bru files.
fn kv_list(v: &Value) -> Vec<Pair> {
    let Some(items) = v.as_sequence() else { return vec![] };
    items
        .iter()
        .map(|it| {
            let mut p = Pair {
                key: s(get(it, "name")),
                enabled: get(it, "disabled").as_bool() != Some(true),
                description: text_or_content(get(it, "description")),
                content_type: s(get(it, "contentType")),
                ..Default::default()
            };
            let value = get(it, "value");
            match value {
                Value::Mapping(_) if value.get("type").is_some() && value.get("data").is_some() => {
                    p.value = s(get(value, "data"));
                    let t = s(get(value, "type"));
                    if t != "string" {
                        p.annotations.push(format!("@{t}"));
                    }
                }
                Value::Sequence(files) => {
                    p.is_file = true;
                    p.value = files.iter().map(s).collect::<Vec<_>>().join("|");
                }
                other => p.value = s(other),
            }
            if s(get(it, "type")) == "file" {
                p.is_file = true;
            }
            p
        })
        .collect()
}

fn kv_entry(p: &Pair) -> Mapping {
    let mut m = Mapping::new();
    m.insert(str_v("name"), str_v(&p.key));
    let typed = p.annotations.iter().find_map(|a| a.strip_prefix('@').filter(|t| ["number", "boolean", "object"].contains(t)));
    m.insert(
        str_v("value"),
        match typed {
            Some(t) => {
                let mut tv = Mapping::new();
                tv.insert(str_v("type"), str_v(t));
                tv.insert(str_v("data"), str_v(&p.value));
                Value::Mapping(tv)
            }
            None => str_v(&p.value),
        },
    );
    if !p.description.trim().is_empty() {
        m.insert(str_v("description"), str_v(&p.description));
    }
    if !p.enabled {
        m.insert(str_v("disabled"), Value::Bool(true));
    }
    m
}

fn kv_value(pairs: &[Pair]) -> Option<Value> {
    (!pairs.is_empty()).then(|| Value::Sequence(pairs.iter().map(|p| Value::Mapping(kv_entry(p))).collect()))
}

fn set(m: &mut Mapping, key: &str, v: Option<Value>) {
    match v {
        Some(v) => {
            m.insert(str_v(key), v);
        }
        None => {
            m.remove(key);
        }
    }
}

fn some_str(x: &str) -> Option<Value> {
    (!x.is_empty()).then(|| str_v(x))
}

fn child<'a>(m: &'a mut Mapping, key: &str) -> &'a mut Mapping {
    if !m.get(key).is_some_and(Value::is_mapping) {
        m.insert(str_v(key), Value::Mapping(Mapping::new()));
    }
    m.get_mut(key).and_then(Value::as_mapping_mut).unwrap()
}

fn parse_auth(v: &Value) -> Auth {
    match v {
        Value::Null => Auth { mode: "none".into(), ..Default::default() },
        Value::String(x) => Auth { mode: x.clone(), ..Default::default() },
        m => Auth {
            mode: s(get(m, "type")),
            username: s(get(m, "username")),
            password: s(get(m, "password")),
            token: s(get(m, "token")),
        },
    }
}

/// Modes ZenProbe doesn't edit (oauth2, digest, ...) keep the original value untouched.
fn write_auth(m: &mut Mapping, auth: &Auth) {
    let v = match auth.mode.as_str() {
        "none" | "" => None,
        "inherit" => Some(str_v("inherit")),
        "basic" | "bearer" => {
            let mut a = Mapping::new();
            a.insert(str_v("type"), str_v(&auth.mode));
            if auth.mode == "basic" {
                a.insert(str_v("username"), str_v(&auth.username));
                a.insert(str_v("password"), str_v(&auth.password));
            } else {
                a.insert(str_v("token"), str_v(&auth.token));
            }
            Some(Value::Mapping(a))
        }
        _ => return,
    };
    set(m, "auth", v);
}

const SCRIPT_TYPES: [&str; 3] = ["before-request", "after-response", "tests"];

/// Shared `runtime` (requests) / `request` (collection, folder) section.
struct Runtime {
    vars_req: Vec<Pair>,
    vars_res: Vec<Pair>,
    asserts: Vec<Pair>,
    script_req: String,
    script_res: String,
    tests: String,
}

fn parse_runtime(rt: &Value) -> Runtime {
    let mut r = Runtime { vars_req: kv_list(get(rt, "variables")), vars_res: vec![], asserts: vec![], script_req: String::new(), script_res: String::new(), tests: String::new() };
    for a in get(rt, "actions").as_sequence().into_iter().flatten() {
        if s(get(a, "type")) == "set-variable" && s(get(a, "phase")) == "after-response" {
            r.vars_res.push(Pair {
                key: s(get(get(a, "variable"), "name")),
                value: s(get(get(a, "selector"), "expression")),
                enabled: get(a, "disabled").as_bool() != Some(true),
                description: text_or_content(get(a, "description")),
                ..Default::default()
            });
        }
    }
    for sc in get(rt, "scripts").as_sequence().into_iter().flatten() {
        let code = s(get(sc, "code"));
        match s(get(sc, "type")).as_str() {
            "before-request" => r.script_req = code,
            "after-response" => r.script_res = code,
            "tests" => r.tests = code,
            _ => {}
        }
    }
    for a in get(rt, "assertions").as_sequence().into_iter().flatten() {
        let op = s(get(a, "operator"));
        let value = match a.get("value") {
            Some(v) if !v.is_null() => format!("{op} {}", s(v)),
            _ => op,
        };
        r.asserts.push(Pair {
            key: s(get(a, "expression")),
            value,
            enabled: get(a, "disabled").as_bool() != Some(true),
            description: text_or_content(get(a, "description")),
            ..Default::default()
        });
    }
    r
}

const UNARY: &[&str] = &["isEmpty", "isNotEmpty", "isNull", "isUndefined", "isDefined", "isTruthy", "isFalsy", "isJson", "isNumber", "isString", "isBoolean", "isArray"];
const BINARY: &[&str] = &["eq", "neq", "gt", "gte", "lt", "lte", "in", "notIn", "contains", "notContains", "length", "matches", "notMatches", "startsWith", "endsWith", "between"];

fn write_runtime(m: &mut Mapping, r: &Runtime) {
    set(m, "variables", kv_value(&r.vars_req));

    // keep actions/scripts of kinds ZenProbe does not model
    let mut actions: Vec<Value> = m
        .get("actions")
        .and_then(Value::as_sequence)
        .into_iter()
        .flatten()
        .filter(|a| !(s(get(a, "type")) == "set-variable" && s(get(a, "phase")) == "after-response"))
        .cloned()
        .collect();
    for p in &r.vars_res {
        let mut a = Mapping::new();
        a.insert(str_v("type"), str_v("set-variable"));
        a.insert(str_v("phase"), str_v("after-response"));
        let mut sel = Mapping::new();
        sel.insert(str_v("expression"), str_v(&p.value));
        sel.insert(str_v("method"), str_v("jsonq"));
        a.insert(str_v("selector"), Value::Mapping(sel));
        let mut var = Mapping::new();
        var.insert(str_v("name"), str_v(&p.key));
        var.insert(str_v("scope"), str_v("runtime"));
        a.insert(str_v("variable"), Value::Mapping(var));
        if !p.description.trim().is_empty() {
            a.insert(str_v("description"), str_v(&p.description));
        }
        if !p.enabled {
            a.insert(str_v("disabled"), Value::Bool(true));
        }
        actions.push(Value::Mapping(a));
    }
    set(m, "actions", (!actions.is_empty()).then_some(Value::Sequence(actions)));

    let mut scripts: Vec<Value> = Vec::new();
    for (t, code) in SCRIPT_TYPES.iter().zip([&r.script_req, &r.script_res, &r.tests]) {
        if !code.trim().is_empty() {
            let mut sc = Mapping::new();
            sc.insert(str_v("type"), str_v(t));
            sc.insert(str_v("code"), str_v(code.trim()));
            scripts.push(Value::Mapping(sc));
        }
    }
    scripts.extend(m.get("scripts").and_then(Value::as_sequence).into_iter().flatten().filter(|sc| !SCRIPT_TYPES.contains(&s(get(sc, "type")).as_str())).cloned());
    set(m, "scripts", (!scripts.is_empty()).then_some(Value::Sequence(scripts)));

    let asserts: Vec<Value> = r
        .asserts
        .iter()
        .map(|p| {
            let (first, rest) = p.value.trim().split_once(' ').unwrap_or((p.value.trim(), ""));
            let (op, value) = if UNARY.contains(&first) {
                (first, None)
            } else if BINARY.contains(&first) {
                (first, Some(rest))
            } else {
                ("eq", Some(p.value.as_str()))
            };
            let mut a = Mapping::new();
            a.insert(str_v("expression"), str_v(&p.key));
            a.insert(str_v("operator"), str_v(op));
            if let Some(v) = value {
                a.insert(str_v("value"), str_v(v));
            }
            if !p.description.trim().is_empty() {
                a.insert(str_v("description"), str_v(&p.description));
            }
            if !p.enabled {
                a.insert(str_v("disabled"), Value::Bool(true));
            }
            Value::Mapping(a)
        })
        .collect();
    set(m, "assertions", (!asserts.is_empty()).then_some(Value::Sequence(asserts)));
}

/// Bruno's stringifyYml adds a blank line before top-level blocks.
const BLOCK_KEYS: &[&str] = &["info", "http", "graphql", "grpc", "websocket", "mock", "routes", "runtime", "settings", "examples", "docs", "items", "request"];

/// Does a string need quotes to stay a string (yaml npm core schema rules, as used by Bruno)?
fn needs_quotes(x: &str) -> bool {
    x.is_empty()
        || x.starts_with([' ', '-', '?', ':', ',', '[', ']', '{', '}', '#', '&', '*', '!', '|', '>', '\'', '"', '%', '@', '`'])
        || x.ends_with([' ', ':'])
        || x.contains(": ")
        || x.contains(" #")
        || x.contains('\t')
        || x.chars().any(|c| c.is_control())
        || matches!(x.to_lowercase().as_str(), "true" | "false" | "null" | "~" | ".inf" | "-.inf" | ".nan")
        || x.parse::<f64>().is_ok()
        || x.starts_with("0x")
        || x.starts_with("0o")
}

fn quoted(x: &str) -> String {
    let mut out = String::from("\"");
    for c in x.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\x{:02X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn scalar(v: &Value, indent: usize) -> String {
    match v {
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(x) if x.contains('\n') && !x.contains('\r') && !x.chars().any(|c| c.is_control() && c != '\n') => {
            let chomp = if x.ends_with("\n\n") { "+" } else if x.ends_with('\n') { "" } else { "-" };
            let ind = if x.starts_with(' ') { "2" } else { "" };
            let pad = " ".repeat(indent);
            let body = x.strip_suffix('\n').unwrap_or(x);
            let lines: Vec<String> = body.split('\n').map(|l| if l.is_empty() { String::new() } else { format!("{pad}{l}") }).collect();
            format!("|{ind}{chomp}\n{}", lines.join("\n"))
        }
        Value::String(x) if needs_quotes(x) || x.contains('\n') => quoted(x),
        Value::String(x) => x.clone(),
        Value::Mapping(m) if m.is_empty() => "{}".into(),
        Value::Sequence(s) if s.is_empty() => "[]".into(),
        Value::Tagged(t) => scalar(&t.value, indent),
        _ => unreachable!("collections are emitted by emit()"),
    }
}

fn is_block(v: &Value) -> bool {
    matches!(v, Value::Mapping(m) if !m.is_empty()) || matches!(v, Value::Sequence(s) if !s.is_empty())
}

/// Block-style emitter matching the `yaml` npm package defaults (indent 2, indented sequences).
fn emit(v: &Value, indent: usize, out: &mut String) {
    let pad = " ".repeat(indent);
    match v {
        Value::Mapping(m) if !m.is_empty() => {
            for (k, val) in m {
                let key = match k {
                    Value::String(x) if !needs_quotes(x) => x.clone(),
                    other => scalar(other, indent),
                };
                if is_block(val) {
                    out.push_str(&format!("{pad}{key}:\n"));
                    emit(val, indent + 2, out);
                } else {
                    out.push_str(&format!("{pad}{key}: {}\n", scalar(val, indent + 2)));
                }
            }
        }
        Value::Sequence(s) if !s.is_empty() => {
            for item in s {
                if is_block(item) {
                    let mut inner = String::new();
                    emit(item, indent + 2, &mut inner);
                    // first line of the nested block goes right after "- "
                    out.push_str(&format!("{pad}- {}", &inner[indent + 2..]));
                } else {
                    out.push_str(&format!("{pad}- {}\n", scalar(item, indent + 2)));
                }
            }
        }
        other => out.push_str(&format!("{pad}{}\n", scalar(other, indent))),
    }
}

pub fn dump(doc: &Value) -> String {
    let mut raw = String::new();
    emit(doc, 0, &mut raw);
    let mut out: Vec<&str> = Vec::new();
    for (i, line) in raw.lines().enumerate() {
        if i > 0 && !line.starts_with([' ', '\t', '-']) && BLOCK_KEYS.contains(&line.split(':').next().unwrap_or("")) && out.last().is_some_and(|l| !l.trim().is_empty()) {
            out.push("");
        }
        out.push(line);
    }
    out.join("\n") + "\n"
}

fn section_key(kind: &str) -> &'static str {
    match kind {
        "graphql" => "graphql",
        "grpc" => "grpc",
        "ws" => "websocket",
        _ => "http",
    }
}

fn typed_setting(v: &str) -> Value {
    match v {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        _ => v.parse::<i64>().map(|n| Value::Number(n.into())).unwrap_or_else(|_| str_v(v)),
    }
}

// ---------- requests ----------

pub fn parse_request(src: &str) -> Result<Request, String> {
    let doc: Value = serde_yaml::from_str(src).map_err(|e| e.to_string())?;
    let info = get(&doc, "info");
    let kind = match s(get(info, "type")).as_str() {
        "graphql" => "graphql",
        "grpc" => "grpc",
        "websocket" => "ws",
        _ => "http",
    };
    let h = get(&doc, section_key(kind));
    let mut r = Request {
        name: s(get(info, "name")),
        kind: kind.into(),
        seq: match get(info, "seq") {
            Value::Null => "1".into(),
            v => s(v),
        },
        tags: get(info, "tags").as_sequence().map(|t| t.iter().map(s).collect()).unwrap_or_default(),
        url: s(get(h, "url")),
        auth: parse_auth(get(h, "auth")),
        docs: text_or_content(get(&doc, "docs")),
        ..Default::default()
    };
    match kind {
        "grpc" => {
            r.method = s(get(h, "method"));
            r.grpc_method_type = s(get(h, "methodType"));
            r.proto_path = s(get(h, "protoFilePath"));
            r.headers = kv_list(get(h, "metadata"));
            r.body_mode = "grpc".into();
            r.messages = get(h, "message")
                .as_sequence()
                .into_iter()
                .flatten()
                .enumerate()
                .map(|(i, m)| Message { name: s(get(m, "title")).if_empty(format!("message {}", i + 1)), content: s(get(m, "message")), ..Default::default() })
                .collect();
        }
        "ws" => {
            r.headers = kv_list(get(h, "headers"));
            r.body_mode = "ws".into();
            let msg = get(h, "message");
            r.messages = match msg {
                Value::Sequence(items) => items
                    .iter()
                    .enumerate()
                    .map(|(i, m)| Message {
                        name: s(get(m, "title")).if_empty(format!("message {}", i + 1)),
                        kind: s(get(get(m, "message"), "type")).if_empty("text".into()),
                        content: s(get(get(m, "message"), "data")),
                        selected: get(m, "selected").as_bool() == Some(true),
                    })
                    .collect(),
                Value::Mapping(_) if !s(get(msg, "data")).trim().is_empty() => {
                    vec![Message { name: "message 1".into(), kind: s(get(msg, "type")).if_empty("text".into()), content: s(get(msg, "data")), selected: true }]
                }
                _ => vec![],
            };
        }
        _ => {
            r.method = s(get(h, "method")).if_empty(if kind == "graphql" { "POST" } else { "GET" }.into()).to_lowercase();
            r.headers = kv_list(get(h, "headers"));
            for (p, raw) in kv_list(get(h, "params")).into_iter().zip(get(h, "params").as_sequence().into_iter().flatten()) {
                if s(get(raw, "type")) == "path" { r.path_params.push(p) } else { r.query.push(p) }
            }
            let body = get(h, "body");
            if kind == "graphql" {
                r.body_mode = "graphql".into();
                r.gql_query = s(get(body, "query"));
                r.gql_vars = s(get(body, "variables"));
            } else {
                let data = get(body, "data");
                r.body_mode = match s(get(body, "type")).as_str() {
                    "json" => { r.body_json = s(data); "json" }
                    "text" => { r.body_text = s(data); "text" }
                    "xml" => { r.body_xml = s(data); "xml" }
                    "sparql" => { r.body_sparql = s(data); "sparql" }
                    "form-urlencoded" => { r.form = kv_list(data); "formUrlEncoded" }
                    "multipart-form" => { r.multipart = kv_list(data); "multipartForm" }
                    "file" => {
                        r.files = data.as_sequence().into_iter().flatten().map(|f| Pair {
                            key: "file".into(),
                            value: s(get(f, "filePath")),
                            content_type: s(get(f, "contentType")),
                            enabled: get(f, "selected").as_bool() == Some(true),
                            ..Default::default()
                        }).collect();
                        "file"
                    }
                    _ => "none",
                }.into();
            }
        }
    }
    let rt = parse_runtime(get(&doc, "runtime"));
    (r.vars_req, r.vars_res, r.asserts, r.script_req, r.script_res, r.tests) = (rt.vars_req, rt.vars_res, rt.asserts, rt.script_req, rt.script_res, rt.tests);
    if let Some(m) = get(&doc, "settings").as_mapping() {
        r.settings = m
            .iter()
            .map(|(k, v)| match v.as_sequence() {
                Some(items) => Pair { key: s(k), list: Some(items.iter().map(s).collect()), enabled: true, ..Default::default() },
                None => Pair::new(&s(k), &s(v)),
            })
            .collect();
    }
    r.keep.yaml = Some(doc);
    Ok(r)
}

trait IfEmpty {
    fn if_empty(self, other: String) -> String;
}
impl IfEmpty for String {
    fn if_empty(self, other: String) -> String {
        if self.is_empty() { other } else { self }
    }
}

pub fn write_request(r: &Request) -> String {
    let mut doc = r.keep.yaml.clone().filter(Value::is_mapping).unwrap_or(Value::Mapping(Mapping::new()));
    let m = doc.as_mapping_mut().unwrap();
    let section = section_key(&r.kind);
    for other in ["http", "graphql", "grpc", "websocket"] {
        if other != section {
            m.remove(other);
        }
    }
    {
        let info = child(m, "info");
        info.insert(str_v("name"), str_v(&r.name));
        info.insert(str_v("type"), str_v(if r.kind == "ws" { "websocket" } else { &r.kind }));
        info.insert(str_v("seq"), typed_setting(&r.seq));
        set(info, "tags", (!r.tags.is_empty()).then(|| Value::Sequence(r.tags.iter().map(|t| str_v(t)).collect())));
    }
    {
        let h = child(m, section);
        // Bruno writes `method` before `url` for http/graphql; existing keys keep their position
        if matches!(r.kind.as_str(), "http" | "graphql") {
            h.insert(str_v("method"), str_v(&r.method.to_uppercase()));
        }
        h.insert(str_v("url"), str_v(&r.url));
        match r.kind.as_str() {
            "grpc" => {
                h.insert(str_v("method"), str_v(&r.method));
                set(h, "methodType", some_str(&r.grpc_method_type));
                set(h, "protoFilePath", some_str(&r.proto_path));
                set(h, "metadata", kv_value(&r.headers));
                h.remove("headers");
                write_auth(h, &r.auth);
                let msgs: Vec<Value> = r
                    .messages
                    .iter()
                    .map(|msg| {
                        let mut x = Mapping::new();
                        x.insert(str_v("title"), str_v(&msg.name));
                        x.insert(str_v("message"), str_v(&msg.content));
                        Value::Mapping(x)
                    })
                    .collect();
                set(h, "message", (!msgs.is_empty()).then_some(Value::Sequence(msgs)));
            }
            "ws" => {
                set(h, "headers", kv_value(&r.headers));
                write_auth(h, &r.auth);
                let msgs: Vec<Value> = r
                    .messages
                    .iter()
                    .map(|msg| {
                        let mut inner = Mapping::new();
                        inner.insert(str_v("type"), str_v(if msg.kind.is_empty() { "text" } else { &msg.kind }));
                        inner.insert(str_v("data"), str_v(&msg.content));
                        let mut x = Mapping::new();
                        x.insert(str_v("title"), str_v(&msg.name));
                        x.insert(str_v("selected"), Value::Bool(msg.selected));
                        x.insert(str_v("message"), Value::Mapping(inner));
                        Value::Mapping(x)
                    })
                    .collect();
                set(h, "message", (!msgs.is_empty()).then_some(Value::Sequence(msgs)));
            }
            _ => {
                h.insert(str_v("method"), str_v(&r.method.to_uppercase()));
                set(h, "headers", kv_value(&r.headers));
                let params: Vec<Value> = r
                    .query
                    .iter()
                    .map(|p| (p, "query"))
                    .chain(r.path_params.iter().map(|p| (p, "path")))
                    .map(|(p, t)| {
                        let mut e = kv_entry(p);
                        e.insert(str_v("type"), str_v(t));
                        Value::Mapping(e)
                    })
                    .collect();
                set(h, "params", (!params.is_empty()).then_some(Value::Sequence(params)));
                let body = if r.kind == "graphql" {
                    let mut b = Mapping::new();
                    b.insert(str_v("query"), str_v(&r.gql_query));
                    b.insert(str_v("variables"), str_v(&r.gql_vars));
                    Some(b)
                } else {
                    http_body(r)
                };
                // auth goes after body in Bruno's files; re-insert to keep that order
                h.remove("auth");
                set(h, "body", body.map(Value::Mapping));
                write_auth(h, &r.auth);
                if !h.contains_key("auth") && !matches!(r.auth.mode.as_str(), "none" | "" | "inherit" | "basic" | "bearer") {
                    if let Some(orig) = r.keep.yaml.as_ref().and_then(|d| d.get(section)).and_then(|x| x.get("auth")) {
                        h.insert(str_v("auth"), orig.clone());
                    }
                }
            }
        }
        if r.kind == "grpc" || r.kind == "ws" {
            if !h.contains_key("auth") && !matches!(r.auth.mode.as_str(), "none" | "" | "inherit" | "basic" | "bearer") {
                if let Some(orig) = r.keep.yaml.as_ref().and_then(|d| d.get(section)).and_then(|x| x.get("auth")) {
                    h.insert(str_v("auth"), orig.clone());
                }
            }
        }
    }
    {
        let rt = child(m, "runtime");
        write_runtime(rt, &Runtime { vars_req: r.vars_req.clone(), vars_res: r.vars_res.clone(), asserts: r.asserts.clone(), script_req: r.script_req.clone(), script_res: r.script_res.clone(), tests: r.tests.clone() });
        if rt.is_empty() {
            m.remove("runtime");
        }
    }
    if r.settings.is_empty() {
        m.remove("settings");
    } else {
        let st = child(m, "settings");
        for p in &r.settings {
            let v = match &p.list {
                Some(items) => Value::Sequence(items.iter().map(|i| str_v(i)).collect()),
                None => typed_setting(&p.value),
            };
            st.insert(str_v(&p.key), v);
        }
    }
    set(m, "docs", some_str(&r.docs));
    dump(&doc)
}

fn http_body(r: &Request) -> Option<Mapping> {
    let mut b = Mapping::new();
    let (t, data) = match r.body_mode.as_str() {
        "json" => ("json", str_v(&r.body_json)),
        "text" => ("text", str_v(&r.body_text)),
        "xml" => ("xml", str_v(&r.body_xml)),
        "sparql" => ("sparql", str_v(&r.body_sparql)),
        "formUrlEncoded" => ("form-urlencoded", kv_value(&r.form).unwrap_or(Value::Sequence(vec![]))),
        "multipartForm" => (
            "multipart-form",
            Value::Sequence(
                r.multipart
                    .iter()
                    .map(|p| {
                        let mut e = Mapping::new();
                        e.insert(str_v("name"), str_v(&p.key));
                        e.insert(str_v("type"), str_v(if p.is_file { "file" } else { "text" }));
                        e.insert(
                            str_v("value"),
                            if p.is_file { Value::Sequence(p.value.split('|').filter(|x| !x.is_empty()).map(str_v).collect()) } else { str_v(&p.value) },
                        );
                        if !p.content_type.is_empty() {
                            e.insert(str_v("contentType"), str_v(&p.content_type));
                        }
                        if !p.description.trim().is_empty() {
                            e.insert(str_v("description"), str_v(&p.description));
                        }
                        if !p.enabled {
                            e.insert(str_v("disabled"), Value::Bool(true));
                        }
                        Value::Mapping(e)
                    })
                    .collect(),
            ),
        ),
        "file" => (
            "file",
            Value::Sequence(
                r.files
                    .iter()
                    .map(|p| {
                        let mut e = Mapping::new();
                        e.insert(str_v("filePath"), str_v(&p.value));
                        e.insert(str_v("contentType"), str_v(&p.content_type));
                        e.insert(str_v("selected"), Value::Bool(p.enabled));
                        Value::Mapping(e)
                    })
                    .collect(),
            ),
        ),
        _ => return None,
    };
    b.insert(str_v("type"), str_v(t));
    b.insert(str_v("data"), data);
    Some(b)
}

// ---------- opencollection.yml / folder.yml ----------

pub fn parse_folder(src: &str) -> Result<FolderSettings, String> {
    let doc: Value = serde_yaml::from_str(src).map_err(|e| e.to_string())?;
    let req = get(&doc, "request");
    let rt = parse_runtime(req);
    let info = get(&doc, "info");
    Ok(FolderSettings {
        name: s(get(info, "name")),
        seq: s(get(info, "seq")),
        headers: kv_list(get(req, "headers")),
        auth: parse_auth(get(req, "auth")),
        vars_req: rt.vars_req,
        vars_res: rt.vars_res,
        script_req: rt.script_req,
        script_res: rt.script_res,
        tests: rt.tests,
        docs: text_or_content(get(&doc, "docs")),
        keep: Keep { yaml: Some(doc), ..Default::default() },
    })
}

pub fn write_folder(f: &FolderSettings, is_collection: bool) -> String {
    let mut doc = f.keep.yaml.clone().filter(Value::is_mapping).unwrap_or(Value::Mapping(Mapping::new()));
    let m = doc.as_mapping_mut().unwrap();
    if is_collection && !m.contains_key("opencollection") {
        m.insert(str_v("opencollection"), str_v("1.0.0"));
    }
    {
        let info = child(m, "info");
        info.insert(str_v("name"), str_v(&f.name));
        if !is_collection {
            info.insert(str_v("type"), str_v("folder"));
            set(info, "seq", (!f.seq.is_empty()).then(|| typed_setting(&f.seq)));
        }
    }
    {
        let had_request = m.contains_key("request");
        let req = child(m, "request");
        set(req, "headers", kv_value(&f.headers));
        write_auth(req, &f.auth);
        write_runtime(req, &Runtime { vars_req: f.vars_req.clone(), vars_res: f.vars_res.clone(), asserts: vec![], script_req: f.script_req.clone(), script_res: f.script_res.clone(), tests: f.tests.clone() });
        // Bruno omits an empty `request` in opencollection.yml but writes `request: {}` in folder.yml
        if req.is_empty() && is_collection && !had_request {
            m.remove("request");
        }
    }
    let docs = (!f.docs.trim().is_empty()).then(|| {
        let mut d = Mapping::new();
        d.insert(str_v("content"), str_v(&f.docs));
        d.insert(str_v("type"), str_v("text/markdown"));
        Value::Mapping(d)
    });
    set(m, "docs", docs);
    dump(&doc)
}

// ---------- environments ----------

pub fn parse_env(src: &str) -> Result<Environment, String> {
    let doc: Value = serde_yaml::from_str(src).map_err(|e| e.to_string())?;
    let items = get(&doc, "variables");
    let mut env = Environment { name: s(get(&doc, "name")), ..Default::default() };
    for (p, raw) in kv_list(items).into_iter().zip(items.as_sequence().into_iter().flatten()) {
        if get(raw, "secret").as_bool() == Some(true) {
            env.secrets.push(p.key);
        } else {
            env.vars.push(p);
        }
    }
    env.keep.yaml = Some(doc);
    Ok(env)
}

pub fn write_env(env: &Environment) -> String {
    let mut doc = env.keep.yaml.clone().filter(Value::is_mapping).unwrap_or(Value::Mapping(Mapping::new()));
    let m = doc.as_mapping_mut().unwrap();
    m.insert(str_v("name"), str_v(&env.name));
    let mut vars: Vec<Value> = env.vars.iter().map(|p| Value::Mapping(kv_entry(p))).collect();
    for name in &env.secrets {
        let mut e = Mapping::new();
        e.insert(str_v("secret"), Value::Bool(true));
        e.insert(str_v("name"), str_v(name));
        vars.push(Value::Mapping(e));
    }
    m.insert(str_v("variables"), Value::Sequence(vars));
    dump(&doc)
}

#[cfg(test)]
mod tests {
    use super::*;

    const REQ: &str = r#"info:
  name: post /api/Books/AddOne
  type: http
  seq: 4
  tags:
    - Books

http:
  method: POST
  url: "{{baseUrl}}/api/Books/AddOne"
  params:
    - name: q
      value: "1"
      type: query
    - name: id
      value: "7"
      type: path
  body:
    type: json
    data: |-
      {
        "id": ""
      }
  auth: inherit

runtime:
  variables:
    - name: n
      value:
        type: number
        data: "5"
  actions:
    - type: set-variable
      phase: after-response
      selector:
        expression: res.body.id
        method: jsonq
      variable:
        name: id
        scope: runtime
  scripts:
    - type: before-request
      code: bru.setVar("a", 1)
  assertions:
    - expression: res.status
      operator: eq
      value: "200"
    - expression: res.body
      operator: isJson

settings:
  encodeUrl: true
  timeout: 0

examples:
  - name: 200 Response
    response:
      status: 200
"#;

    #[test]
    fn parses_and_round_trips_request() {
        let r = parse_request(REQ).unwrap();
        assert_eq!((r.method.as_str(), r.body_mode.as_str(), r.auth.mode.as_str()), ("post", "json", "inherit"));
        assert_eq!(r.body_json, "{\n  \"id\": \"\"\n}");
        assert_eq!(r.query[0].key, "q");
        assert_eq!(r.path_params[0].value, "7");
        assert_eq!(r.vars_req[0].annotations, vec!["@number"]);
        assert_eq!(r.vars_res[0].value, "res.body.id");
        assert_eq!(r.asserts[0].value, "eq 200");
        assert_eq!(r.asserts[1].value, "isJson");
        assert_eq!(r.script_req, "bru.setVar(\"a\", 1)");
        assert_eq!(r.setting("timeout"), Some("0"));

        let out = write_request(&r);
        assert!(out.contains("examples:"), "{out}");
        assert!(out.contains("\n\nhttp:"), "{out}");
        let back = parse_request(&out).unwrap();
        assert_eq!(Request { keep: Keep::default(), ..r.clone() }, Request { keep: Keep::default(), ..back });
    }

    #[test]
    fn new_request_kinds_round_trip() {
        for kind in ["http", "graphql", "grpc", "ws"] {
            let mut r = Request::new("x", 1, kind);
            r.url = "localhost:1/x".into();
            r.auth.mode = "bearer".into();
            r.auth.token = "t".into();
            r.headers.push(Pair::new("h", "v"));
            if kind == "graphql" {
                r.gql_query = "{ a }".into();
            }
            let back = parse_request(&write_request(&r)).unwrap();
            assert_eq!(Request { keep: Keep::default(), ..back }, r, "{kind}");
        }
    }

    #[test]
    fn folder_and_env() {
        let src = "opencollection: 1.0.0\n\ninfo:\n  name: LMS\nconfig:\n  proxy:\n    inherit: true\n\nrequest:\n  variables:\n    - name: baseUrl\n      value: http://localhost:5159\n";
        let f = parse_folder(src).unwrap();
        assert_eq!(f.vars_req[0].value, "http://localhost:5159");
        let out = write_folder(&f, true);
        assert!(out.contains("proxy:"));
        assert_eq!(parse_folder(&out).unwrap().vars_req, f.vars_req);

        let e = parse_env("name: dev\nvariables:\n  - name: host\n    value: x\n  - secret: true\n    name: token\n").unwrap();
        assert_eq!(e.secrets, vec!["token"]);
        let back = parse_env(&write_env(&e)).unwrap();
        assert_eq!((back.vars, back.secrets), (e.vars, e.secrets));
    }
}

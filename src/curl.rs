//! cURL import (parse a command into a request) and export (request → bash cURL command).

use crate::collection::query_from_url;
use crate::http::{BodyData, Prepared};
use crate::model::{Pair, Request};

/// Shell-like tokenizer: '...', "...", $'...', backslash escapes, and bash `\` / cmd `^` line continuations.
fn tokenize(cmd: &str) -> Result<Vec<String>, String> {
    // Windows "Copy as cURL (cmd)": undo cmd.exe `^` escaping first, leaving MSVC-style \" quoting
    let cmd = if cmd.contains("^\"") || cmd.contains("^\n") || cmd.contains("^\r\n") {
        let joined = cmd.replace("^\r\n", " ").replace("^\n", " ");
        let mut out = String::new();
        let mut it = joined.chars();
        while let Some(c) = it.next() {
            match c {
                '^' => out.extend(it.next()),
                c => out.push(c),
            }
        }
        out
    } else {
        cmd.to_string()
    };
    let cmd = cmd.replace("\\\r\n", " ").replace("\\\n", " ");
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut has = false;
    let mut it = cmd.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            c if c.is_whitespace() => {
                if has {
                    out.push(std::mem::take(&mut cur));
                    has = false;
                }
            }
            '\'' => {
                has = true;
                loop {
                    match it.next() {
                        Some('\'') => break,
                        Some(x) => cur.push(x),
                        None => return Err("unterminated ' quote".into()),
                    }
                }
            }
            '$' if it.peek() == Some(&'\'') => {
                it.next();
                has = true;
                loop {
                    match it.next() {
                        Some('\'') => break,
                        Some('\\') => match it.next() {
                            Some('n') => cur.push('\n'),
                            Some('t') => cur.push('\t'),
                            Some('r') => cur.push('\r'),
                            Some(x) => cur.push(x),
                            None => return Err("unterminated $' quote".into()),
                        },
                        Some(x) => cur.push(x),
                        None => return Err("unterminated $' quote".into()),
                    }
                }
            }
            '"' => {
                has = true;
                loop {
                    match it.next() {
                        Some('"') => break,
                        Some('\\') if matches!(it.peek(), Some('"' | '\\' | '$' | '`')) => cur.push(it.next().unwrap()),
                        Some(x) => cur.push(x),
                        None => return Err("unterminated \" quote".into()),
                    }
                }
            }
            '\\' => {
                has = true;
                if let Some(x) = it.next() {
                    cur.push(x);
                }
            }
            c => {
                has = true;
                cur.push(c);
            }
        }
    }
    if has {
        out.push(cur);
    }
    Ok(out)
}

fn percent_decode(s: &str) -> String {
    let bytes = s.replace('+', " ").into_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(b) = u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""), 16)
        {
            out.push(b);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn is_form_pairs(data: &str) -> bool {
    !data.is_empty() && data.split('&').all(|kv| kv.split_once('=').is_some_and(|(k, _)| !k.is_empty() && !k.contains([' ', '{', '"', '<'])))
}

/// Parse a cURL command into a new HTTP request (name derived from the URL).
pub fn parse_curl(cmd: &str) -> Result<Request, String> {
    let tokens = tokenize(cmd.trim())?;
    let mut it = tokens.into_iter();
    match it.next() {
        Some(c) if c == "curl" || c.ends_with("curl") || c.ends_with("curl.exe") => {}
        _ => return Err("not a curl command (it must start with `curl`)".into()),
    }
    let mut url = String::new();
    let mut method: Option<String> = None;
    let mut headers: Vec<Pair> = Vec::new();
    let mut data: Vec<String> = Vec::new();
    let mut urlencoded: Vec<(String, String)> = Vec::new();
    let mut form: Vec<Pair> = Vec::new();
    let mut user: Option<String> = None;
    let mut get = false;
    let mut binary_file: Option<String> = None;
    let mut args = it.peekable();
    while let Some(a) = args.next() {
        // --flag=value form
        let (flag, inline) = match a.split_once('=') {
            Some((f, v)) if f.starts_with("--") => (f.to_string(), Some(v.to_string())),
            _ => (a.clone(), None),
        };
        let value = |args: &mut std::iter::Peekable<std::vec::IntoIter<String>>| -> Result<String, String> { inline.clone().or_else(|| args.next()).ok_or(format!("{flag} needs a value")) };
        match flag.as_str() {
            "-X" | "--request" => method = Some(value(&mut args)?.to_uppercase()),
            "-H" | "--header" => {
                let h = value(&mut args)?;
                if let Some((k, v)) = h.split_once(':') {
                    headers.push(Pair::new(k.trim(), v.trim()));
                }
            }
            "-d" | "--data" | "--data-raw" | "--data-ascii" => data.push(value(&mut args)?),
            "--data-binary" => {
                let v = value(&mut args)?;
                match v.strip_prefix('@') {
                    Some(f) => binary_file = Some(f.to_string()),
                    None => data.push(v),
                }
            }
            "--data-urlencode" => {
                let v = value(&mut args)?;
                match v.split_once('=') {
                    Some((k, val)) if !k.is_empty() => urlencoded.push((k.to_string(), val.to_string())),
                    _ => urlencoded.push((String::new(), v.trim_start_matches('=').to_string())),
                }
            }
            "--json" => {
                data.push(value(&mut args)?);
                headers.push(Pair::new("Content-Type", "application/json"));
                headers.push(Pair::new("Accept", "application/json"));
            }
            "-F" | "--form" | "--form-string" => {
                let v = value(&mut args)?;
                let (k, val) = v.split_once('=').ok_or(format!("invalid form field `{v}`"))?;
                let mut p = Pair::new(k, val);
                if let Some(path) = val.strip_prefix('@').filter(|_| flag != "--form-string") {
                    let (path, ct) = match path.split_once(";type=") {
                        Some((p, t)) => (p, t),
                        None => (path, ""),
                    };
                    p.is_file = true;
                    p.value = path.trim_matches('"').to_string();
                    p.content_type = ct.to_string();
                }
                form.push(p);
            }
            "-u" | "--user" => user = Some(value(&mut args)?),
            "-b" | "--cookie" => headers.push(Pair::new("Cookie", &value(&mut args)?)),
            "-A" | "--user-agent" => headers.push(Pair::new("User-Agent", &value(&mut args)?)),
            "-e" | "--referer" => headers.push(Pair::new("Referer", &value(&mut args)?)),
            "--url" => url = value(&mut args)?,
            "-G" | "--get" => get = true,
            "-I" | "--head" => method = Some("HEAD".into()),
            // flags with an argument we accept but don't model
            "-o" | "--output" | "-m" | "--max-time" | "--connect-timeout" | "-x" | "--proxy" | "--cacert" | "--cert" | "--key" | "-w" | "--write-out" | "--retry" | "-c" | "--cookie-jar" | "--resolve" | "--limit-rate" => {
                value(&mut args)?;
            }
            f if f.starts_with('-') => {} // boolean flags: -L, -k, -s, -v, --compressed, -i, ...
            _ if url.is_empty() => url = a,
            _ => {}
        }
    }
    if url.is_empty() {
        return Err("no URL found in the curl command".into());
    }
    if get && (!data.is_empty() || !urlencoded.is_empty()) {
        let extra: Vec<String> = data.drain(..).chain(urlencoded.drain(..).map(|(k, v)| if k.is_empty() { v } else { format!("{k}={v}") })).collect();
        url = format!("{url}{}{}", if url.contains('?') { "&" } else { "?" }, extra.join("&"));
    }
    let has_body = !data.is_empty() || !urlencoded.is_empty() || !form.is_empty() || binary_file.is_some();
    let method = method.unwrap_or_else(|| if has_body { "POST".into() } else { "GET".into() });

    let mut r = Request::new("", 1, "http");
    r.method = method.to_lowercase();
    r.url = url.clone();
    r.query = query_from_url(&url).into_iter().map(|(k, v)| Pair::new(&k, &v)).collect();
    r.path_params = crate::collection::path_params_in_url(&url).into_iter().map(|n| Pair::new(&n, "")).collect();
    r.auth.mode = "none".into();

    // credentials become Bruno auth rather than raw headers
    if let Some(u) = user {
        let (name, pass) = u.split_once(':').unwrap_or((&u, ""));
        r.auth = crate::model::Auth { mode: "basic".into(), username: name.into(), password: pass.into(), token: String::new() };
    }
    if let Some(i) = headers.iter().position(|h| h.key.eq_ignore_ascii_case("authorization") && h.value.to_lowercase().starts_with("bearer ")) {
        let h = headers.remove(i);
        r.auth = crate::model::Auth { mode: "bearer".into(), token: h.value[7..].trim().to_string(), ..Default::default() };
    }
    let ct = headers.iter().find(|h| h.key.eq_ignore_ascii_case("content-type")).map(|h| h.value.to_lowercase()).unwrap_or_default();
    let body = data.join("&");
    if !form.is_empty() {
        r.body_mode = "multipartForm".into();
        r.multipart = form;
        headers.retain(|h| !h.key.eq_ignore_ascii_case("content-type"));
    } else if let Some(f) = binary_file {
        r.body_mode = "file".into();
        r.files = vec![Pair { key: "file".into(), value: f, content_type: ct.clone(), enabled: true, ..Default::default() }];
    } else if !urlencoded.is_empty() || (!body.is_empty() && (ct.contains("x-www-form-urlencoded") || (ct.is_empty() && is_form_pairs(&body)))) {
        r.body_mode = "formUrlEncoded".into();
        r.form = body.split('&').filter(|s| !s.is_empty()).map(|kv| {
            let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
            Pair::new(&percent_decode(k), &percent_decode(v))
        }).chain(urlencoded.into_iter().map(|(k, v)| Pair::new(&k, &v))).collect();
    } else if !body.is_empty() {
        let trimmed = body.trim_start();
        if ct.contains("json") || ((trimmed.starts_with('{') || trimmed.starts_with('[')) && serde_json::from_str::<serde_json::Value>(&body).is_ok()) {
            r.body_mode = "json".into();
            r.body_json = serde_json::from_str::<serde_json::Value>(&body).ok().and_then(|v| serde_json::to_string_pretty(&v).ok()).unwrap_or(body);
        } else if ct.contains("xml") || trimmed.starts_with('<') {
            r.body_mode = "xml".into();
            r.body_xml = body;
        } else {
            r.body_mode = "text".into();
            r.body_text = body;
        }
    }
    r.headers = headers;
    r.name = default_name(&r.method, &url);
    Ok(r)
}

/// "GET users-1" style name from method + last path segments.
pub fn default_name(method: &str, url: &str) -> String {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let path = path.split_once("://").map(|(_, r)| r).unwrap_or(path);
    let segs: Vec<&str> = path.split('/').skip(1).filter(|s| !s.is_empty()).collect();
    let tail = if segs.is_empty() { path.split('/').next().unwrap_or("request").to_string() } else { segs[segs.len().saturating_sub(2)..].join("/") };
    format!("{} {}", method.to_uppercase(), tail)
}

fn q(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// Bash cURL command for a fully prepared (interpolated) request.
pub fn to_curl(p: &Prepared) -> String {
    let mut parts = vec![format!("curl --request {}", p.method), format!("  --url {}", q(&p.url))];
    for (k, v) in &p.headers {
        if matches!(p.body, BodyData::Multipart(_)) && k.eq_ignore_ascii_case("content-type") {
            continue;
        }
        parts.push(format!("  --header {}", q(&format!("{k}: {v}"))));
    }
    if let Some((u, pw)) = &p.basic_auth {
        parts.push(format!("  --user {}", q(&format!("{u}:{pw}"))));
    }
    match &p.body {
        BodyData::None => {}
        BodyData::Raw(s) if s.is_empty() => {}
        BodyData::Raw(s) => parts.push(format!("  --data {}", q(s))),
        BodyData::Form(f) => {
            for (k, v) in f {
                parts.push(format!("  --data-urlencode {}", q(&format!("{k}={v}"))));
            }
        }
        BodyData::Multipart(ps) => {
            for part in ps {
                if part.files.is_empty() {
                    let ct = if part.content_type.is_empty() { String::new() } else { format!(";type={}", part.content_type) };
                    parts.push(format!("  --form {}", q(&format!("{}={}{ct}", part.name, part.value))));
                }
                for f in &part.files {
                    parts.push(format!("  --form {}", q(&format!("{}=@{}", part.name, f.display()))));
                }
            }
        }
        BodyData::File(path, _) => parts.push(format!("  --data-binary {}", q(&format!("@{}", path.display())))),
    }
    if p.follow_redirects {
        parts.insert(1, "  --location".into());
    }
    parts.join(" \\\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::Part;

    #[test]
    fn chrome_bash_copy_as_curl() {
        let cmd = r#"curl 'https://api.example.com/v1/users?page=2&q=a%20b' \
  -H 'accept: application/json' \
  -H 'authorization: Bearer abc.def' \
  -H 'content-type: application/json' \
  --data-raw $'{"name":"O\'Neil","tags":["x"]}' \
  --compressed"#;
        let r = parse_curl(cmd).unwrap();
        assert_eq!(r.method, "post");
        assert_eq!(r.url, "https://api.example.com/v1/users?page=2&q=a%20b");
        assert_eq!(r.query.len(), 2);
        assert_eq!((r.auth.mode.as_str(), r.auth.token.as_str()), ("bearer", "abc.def"));
        assert_eq!(r.body_mode, "json");
        assert!(r.body_json.contains("\"O'Neil\""));
        assert_eq!(r.headers.len(), 2);
        assert_eq!(r.name, "POST v1/users");
    }

    #[test]
    fn windows_cmd_copy_as_curl() {
        let cmd = "curl ^\"https://h.example/api/items^\" ^\r\n  -X PUT ^\r\n  -H ^\"x-k: 1^\" ^\r\n  --data-raw ^\"^{^\\^\"a^\\^\":1^}^\"";
        let r = parse_curl(cmd).unwrap();
        assert_eq!((r.method.as_str(), r.url.as_str()), ("put", "https://h.example/api/items"));
        assert_eq!(r.headers[0].value, "1");
        assert_eq!(r.body_json, "{\n  \"a\": 1\n}");
    }

    #[test]
    fn forms_auth_and_get() {
        let r = parse_curl("curl -u admin:pw -d 'a=1&b=x%20y' http://localhost:3000/login").unwrap();
        assert_eq!((r.auth.mode.as_str(), r.auth.username.as_str(), r.auth.password.as_str()), ("basic", "admin", "pw"));
        assert_eq!(r.body_mode, "formUrlEncoded");
        assert_eq!(r.form[1].value, "x y");

        let r = parse_curl("curl -F 'title=hi' -F 'doc=@/tmp/a.pdf;type=application/pdf' https://x/up").unwrap();
        assert_eq!(r.body_mode, "multipartForm");
        assert!(r.multipart[1].is_file);
        assert_eq!((r.multipart[1].value.as_str(), r.multipart[1].content_type.as_str()), ("/tmp/a.pdf", "application/pdf"));

        let r = parse_curl("curl -G https://x/search -d q=rust --data-urlencode 'n=a b'").unwrap();
        assert_eq!((r.method.as_str(), r.url.as_str()), ("get", "https://x/search?q=rust&n=a b"));

        assert!(parse_curl("wget http://x").is_err());
        assert!(parse_curl("curl -H 'a: b'").is_err());
    }

    #[test]
    fn export_then_import_round_trips() {
        let p = Prepared {
            method: "POST".into(),
            url: "https://api.example.com/items?x=1".into(),
            headers: vec![("Content-Type".into(), "application/json".into()), ("X-Note".into(), "it's ok".into())],
            body: BodyData::Raw("{\"a\":\"b'c\"}".into()),
            basic_auth: Some(("u".into(), "p".into())),
            timeout_ms: 0,
            follow_redirects: true,
            max_redirects: 5,
        };
        let cmd = to_curl(&p);
        assert!(cmd.starts_with("curl --request POST \\\n  --location"), "{cmd}");
        let r = parse_curl(&cmd).unwrap();
        assert_eq!((r.method.as_str(), r.url.as_str()), ("post", "https://api.example.com/items?x=1"));
        assert_eq!(r.headers.iter().find(|h| h.key == "X-Note").unwrap().value, "it's ok");
        assert_eq!(r.auth.mode, "basic");
        assert_eq!(serde_json::from_str::<serde_json::Value>(&r.body_json).unwrap()["a"], "b'c");

        let mp = Prepared { body: BodyData::Multipart(vec![Part { name: "f".into(), value: String::new(), files: vec!["a.txt".into()], content_type: String::new() }]), ..p };
        assert!(to_curl(&mp).contains("--form 'f=@a.txt'"));
    }
}

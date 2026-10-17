//! Request pipeline, in Bruno's order:
//! pre-request vars → pre-request scripts (collection → folders → request) → interpolation → send →
//! post-response vars → post-response scripts + tests (request → folders → collection) → assertions.

use crate::collection::{Collection, apply_path_params};
use crate::http::{self, BodyData, Part, Prepared, Response};
use crate::interp::{self, ResCtx, Vars, interpolate};
use crate::model::{Environment, FolderSettings, Pair, Request};
use crate::script;
use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};

pub type Runtime = Map<String, Value>;

#[derive(Debug, Clone)]
pub struct TestResult {
    pub name: String,
    pub passed: bool,
    pub error: String,
}

#[derive(Debug, Clone, Default)]
pub struct Outcome {
    pub response: Option<Response>,
    pub error: Option<String>,
    pub sent: Option<Prepared>,
    pub logs: Vec<(String, String)>,
    pub tests: Vec<TestResult>,
    pub asserts: Vec<(String, Result<(), String>)>,
    /// Full runtime variable map after the run.
    pub runtime: Runtime,
    /// Environment variables set by scripts (null = deleted); in-memory like Bruno unless saved.
    pub env_changes: Map<String, Value>,
    pub next_request: Option<String>,
    pub skipped: bool,
    pub stop: bool,
}

/// Everything a run needs, cloned so it can move to a worker thread.
#[derive(Clone)]
pub struct Job {
    pub collection: Collection,
    pub env: Option<Environment>,
    /// Values for the selected environment's secret variables (kept in memory only).
    pub secrets: Vars,
    pub runtime: Runtime,
    pub path: PathBuf,
    pub request: Request,
}

fn enabled(p: &[Pair]) -> impl Iterator<Item = (String, String)> + '_ {
    p.iter().filter(|x| x.enabled && !x.key.is_empty()).map(|x| (x.key.trim_start_matches('@').to_string(), x.value.clone()))
}

fn as_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

struct Scopes {
    collection: Vars,
    env: Vars,
    folder: Vars,
    request: Vars,
    process_env: Vars,
}

impl Scopes {
    fn new(job: &Job, chain: &[FolderSettings]) -> Self {
        let mut process_env: Vars = std::env::vars().collect();
        process_env.extend(job.collection.dotenv.clone());
        let mut s = Scopes { collection: enabled(&chain[0].vars_req).collect(), env: Vars::new(), folder: Vars::new(), request: enabled(&job.request.vars_req).collect(), process_env };
        for f in &chain[1..] {
            s.folder.extend(enabled(&f.vars_req));
        }
        if let Some(env) = &job.env {
            s.env.extend(enabled(&env.vars));
            for name in &env.secrets {
                if let Some(v) = job.secrets.get(name) {
                    s.env.insert(name.clone(), v.clone());
                }
            }
        }
        s
    }

    /// Flat map in Bruno precedence: collection < env < folder < request < runtime, plus process.env.*.
    fn flat(&self, runtime: &Runtime) -> Vars {
        let mut vars = Vars::new();
        for (k, v) in &self.process_env {
            vars.insert(format!("process.env.{k}"), v.clone());
        }
        vars.extend(self.collection.clone());
        // env vars may reference process.env
        let env: Vec<_> = self.env.iter().map(|(k, v)| (k.clone(), interpolate(v, &vars, false))).collect();
        vars.extend(env);
        vars.extend(self.folder.clone());
        vars.extend(self.request.clone());
        vars.extend(runtime.iter().map(|(k, v)| (k.clone(), as_text(v))));
        vars
    }

    fn json(&self, runtime: &Runtime) -> Value {
        json!({ "runtime": runtime, "env": self.env, "collection": self.collection, "folder": self.folder, "request": self.request, "global": {}, "processEnv": self.process_env })
    }
}

/// Merge headers and auth from collection → folders → request (not yet interpolated).
pub fn build(c: &Collection, chain: &[FolderSettings], req: &Request) -> Result<Prepared, String> {
    let mut headers: Vec<(String, String)> = Vec::new();
    for list in chain.iter().map(|s| &s.headers).chain(std::iter::once(&req.headers)) {
        for (k, v) in enabled(list) {
            headers.retain(|(h, _)| !h.eq_ignore_ascii_case(&k));
            headers.push((k, v));
        }
    }
    let auth = if req.auth.mode == "inherit" {
        chain.iter().rev().map(|s| &s.auth).find(|a| a.mode != "inherit" && !a.mode.is_empty()).cloned().unwrap_or_default()
    } else {
        req.auth.clone()
    };
    let mut basic_auth = None;
    match auth.mode.as_str() {
        "basic" => basic_auth = Some((auth.username, auth.password)),
        "bearer" => headers.push(("Authorization".into(), format!("Bearer {}", auth.token))),
        _ => {}
    }
    let has_ct = headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("content-type"));
    let ct = |v: &str, headers: &mut Vec<(String, String)>| {
        if !has_ct {
            headers.push(("content-type".into(), v.into()));
        }
    };
    let resolve = |p: &str| -> PathBuf {
        let p = Path::new(p);
        if p.is_absolute() { p.to_path_buf() } else { c.root.join(p) }
    };
    let body = match req.body_mode.as_str() {
        "json" => {
            ct("application/json", &mut headers);
            BodyData::Raw(req.body_json.clone())
        }
        "text" => {
            ct("text/plain", &mut headers);
            BodyData::Raw(req.body_text.clone())
        }
        "xml" => {
            ct("application/xml", &mut headers);
            BodyData::Raw(req.body_xml.clone())
        }
        "sparql" => {
            ct("application/sparql-query", &mut headers);
            BodyData::Raw(req.body_sparql.clone())
        }
        "graphql" => {
            ct("application/json", &mut headers);
            // variables are spliced in as raw JSON text after interpolation (see finish)
            BodyData::Raw(String::new())
        }
        "formUrlEncoded" => BodyData::Form(enabled(&req.form).collect()),
        "multipartForm" => BodyData::Multipart(
            req.multipart
                .iter()
                .filter(|p| p.enabled && !p.key.is_empty())
                .map(|p| Part {
                    name: p.key.clone(),
                    value: if p.is_file { String::new() } else { p.value.clone() },
                    files: if p.is_file { p.value.split('|').filter(|f| !f.is_empty()).map(resolve).collect() } else { vec![] },
                    content_type: p.content_type.clone(),
                })
                .collect(),
        ),
        "file" => match req.files.iter().find(|f| f.enabled) {
            Some(f) => BodyData::File(resolve(&f.value), if f.content_type.is_empty() { "application/octet-stream".into() } else { f.content_type.clone() }),
            None => BodyData::None,
        },
        _ => BodyData::None,
    };
    let num = |k: &str, d: u64| req.setting(k).and_then(|v| v.parse().ok()).unwrap_or(d);
    Ok(Prepared {
        method: req.method.to_uppercase(),
        url: req.url.clone(),
        headers,
        body,
        basic_auth,
        timeout_ms: num("timeout", 0),
        follow_redirects: req.setting("followRedirects") != Some("false"),
        max_redirects: num("maxRedirects", 5) as usize,
    })
}

/// The request exactly as it would be sent (variables resolved, auth applied), without running scripts.
/// Used for cURL export and load tests.
pub fn prepare(job: &Job) -> Result<Prepared, String> {
    let chain = job.collection.settings_chain(&job.path);
    let scopes = Scopes::new(job, &chain);
    let p = build(&job.collection, &chain, &job.request)?;
    finish(p, &job.request, &scopes.flat(&job.runtime))
}

/// Interpolate everything and apply path params; GraphQL bodies are assembled here.
fn finish(mut p: Prepared, req: &Request, vars: &Vars) -> Result<Prepared, String> {
    let i = |s: &str| interpolate(s, vars, false);
    let mut url = i(&p.url).trim().to_string();
    if url.is_empty() {
        return Err("URL is empty".into());
    }
    if !url.contains("://") {
        url = format!("http://{url}");
    }
    p.url = apply_path_params(&url, &req.path_params.iter().map(|x| (x.key.clone(), i(&x.value))).collect::<Vec<_>>());
    p.headers = p.headers.iter().map(|(k, v)| (i(k), i(v))).collect();
    p.basic_auth = p.basic_auth.map(|(u, pw)| (i(&u), i(&pw)));
    p.body = match p.body {
        BodyData::Raw(_) if req.body_mode == "graphql" => {
            let query = interpolate(&req.gql_query, vars, true);
            let vars_text = interpolate(&req.gql_vars, vars, true);
            let variables: Value = if vars_text.trim().is_empty() { json!({}) } else { serde_json::from_str(&vars_text).map_err(|e| format!("GraphQL variables are not valid JSON: {e}"))? };
            BodyData::Raw(json!({ "query": query, "variables": variables }).to_string())
        }
        BodyData::Raw(s) => BodyData::Raw(interpolate(&s, vars, req.body_mode == "json")),
        BodyData::Form(f) => BodyData::Form(f.iter().map(|(k, v)| (i(k), i(v))).collect()),
        BodyData::Multipart(parts) => BodyData::Multipart(parts.into_iter().map(|x| Part { name: i(&x.name), value: i(&x.value), ..x }).collect()),
        other => other,
    };
    Ok(p)
}

fn script_input(job: &Job, scopes: &Scopes, runtime: &Runtime, p: &Prepared, res: Option<(&Response, &Value)>) -> Value {
    let mut headers = Map::new();
    for (k, v) in &p.headers {
        headers.insert(k.clone(), Value::String(v.clone()));
    }
    let body = match &p.body {
        BodyData::Raw(_) if job.request.body_mode == "graphql" => json!({ "query": job.request.gql_query, "variables": job.request.gql_vars }),
        BodyData::Raw(s) => Value::String(s.clone()),
        BodyData::Form(f) => Value::Object(f.iter().map(|(k, v)| (k.clone(), Value::String(v.clone()))).collect()),
        _ => Value::Null,
    };
    let res = res.map(|(r, body)| {
        let headers: Map<String, Value> = r.headers.iter().map(|(k, v)| (k.to_ascii_lowercase(), Value::String(v.clone()))).collect();
        json!({ "status": r.status, "statusText": r.status_text, "headers": headers, "body": body, "responseTime": r.time_ms as u64, "url": r.url, "size": r.body.len() })
    });
    json!({
        "req": { "url": p.url, "method": p.method, "headers": headers, "body": body, "bodyMode": job.request.body_mode, "timeout": p.timeout_ms, "name": job.request.name, "authMode": job.request.auth.mode },
        "res": res,
        "vars": scopes.json(runtime),
        "envName": job.env.as_ref().map(|e| e.name.clone()),
        "collectionName": job.collection.name,
        "cwd": job.collection.root.to_string_lossy(),
    })
}

fn apply_script_out(out: &Value, o: &mut Outcome, runtime: &mut Runtime, scopes: &mut Scopes) -> Option<String> {
    for (k, v) in out["runtime"].as_object().into_iter().flatten() {
        if v.is_null() { runtime.remove(k) } else { runtime.insert(k.clone(), v.clone()) };
    }
    for (k, v) in out["env"].as_object().into_iter().flatten() {
        if v.is_null() { scopes.env.remove(k) } else { scopes.env.insert(k.clone(), as_text(v)) };
        o.env_changes.insert(k.clone(), v.clone());
    }
    for l in out["logs"].as_array().into_iter().flatten() {
        o.logs.push((as_text(&l[0]), as_text(&l[1])));
    }
    for t in out["tests"].as_array().into_iter().flatten() {
        o.tests.push(TestResult { name: as_text(&t["name"]), passed: t["passed"].as_bool() == Some(true), error: t["error"].as_str().unwrap_or_default().to_string() });
    }
    if let Some(n) = out["nextRequest"].as_str() {
        o.next_request = Some(n.to_string());
    }
    o.skipped |= out["skip"].as_bool() == Some(true);
    o.stop |= out["stop"].as_bool() == Some(true);
    out["error"].as_str().map(str::to_string)
}

pub fn run(job: Job) -> Outcome {
    let mut o = Outcome::default();
    let mut runtime = job.runtime.clone();
    let chain = job.collection.settings_chain(&job.path);
    let mut scopes = Scopes::new(&job, &chain);
    let req = &job.request;

    let mut prepared = match build(&job.collection, &chain, req) {
        Ok(p) => p,
        Err(e) => {
            o.error = Some(e);
            o.runtime = runtime;
            return o;
        }
    };

    let pre: Vec<&str> = chain.iter().map(|f| f.script_req.as_str()).chain(std::iter::once(req.script_req.as_str())).collect();
    if pre.iter().any(|s| !s.trim().is_empty()) {
        let out = script::run(&pre, &script_input(&job, &scopes, &runtime, &prepared, None));
        if let Some(err) = apply_script_out(&out, &mut o, &mut runtime, &mut scopes) {
            o.error = Some(format!("Error in pre-request script: {err}"));
            o.runtime = runtime;
            return o;
        }
        let r = &out["req"];
        if r.is_object() {
            prepared.url = as_text(&r["url"]);
            prepared.method = as_text(&r["method"]).to_uppercase();
            prepared.headers = r["headers"].as_object().into_iter().flatten().map(|(k, v)| (k.clone(), as_text(v))).collect();
            if let (Some(t), true) = (r["timeout"].as_u64(), r["timeout"].is_number()) {
                prepared.timeout_ms = t;
            }
            if matches!(prepared.body, BodyData::Raw(_)) && req.body_mode != "graphql" && r["body"].is_string() {
                prepared.body = BodyData::Raw(as_text(&r["body"]));
            }
        }
        if o.skipped || o.stop {
            o.runtime = runtime;
            return o;
        }
    }

    let vars = scopes.flat(&runtime);
    let prepared = match finish(prepared, req, &vars) {
        Ok(p) => p,
        Err(e) => {
            o.error = Some(e);
            o.runtime = runtime;
            return o;
        }
    };
    o.sent = Some(prepared.clone());
    let resp = match http::send(&prepared) {
        Ok(r) => r,
        Err(e) => {
            o.error = Some(e);
            o.runtime = runtime;
            return o;
        }
    };

    let text = String::from_utf8_lossy(&resp.body).into_owned();
    let mut body: Value = serde_json::from_str(&text).unwrap_or(Value::String(text));

    // post-response vars run as JS so any expression (res('a.b'), res.body.items.length) works
    let post_vars: String = req
        .vars_res
        .iter()
        .filter(|v| v.enabled && !v.key.is_empty())
        .map(|v| format!("try {{ bru.setVar({}, ({})); }} catch (e) {{ console.error('vars:post-response {}: ' + e.message); }}", json!(v.key.trim_start_matches('@')), v.value, v.key.replace('\'', "")))
        .collect::<Vec<_>>()
        .join("\n");
    let mut post: Vec<&str> = vec![post_vars.as_str(), req.script_res.as_str()];
    post.extend(chain.iter().rev().map(|f| f.script_res.as_str()));
    post.push(req.tests.as_str());
    post.extend(chain.iter().rev().map(|f| f.tests.as_str()));
    if post.iter().any(|s| !s.trim().is_empty()) {
        let out = script::run(&post, &script_input(&job, &scopes, &runtime, &prepared, Some((&resp, &body))));
        if let Some(err) = apply_script_out(&out, &mut o, &mut runtime, &mut scopes) {
            o.error = Some(format!("Error in post-response script: {err}"));
        }
        if !out["resBody"].is_null() {
            body = out["resBody"].clone();
        }
    }

    let vars = scopes.flat(&runtime);
    let ctx = ResCtx { status: resp.status, headers: &resp.headers, body: &body, time_ms: resp.time_ms };
    for a in req.asserts.iter().filter(|a| a.enabled && !a.key.is_empty()) {
        let rhs = interpolate(&a.value, &vars, false);
        o.asserts.push((format!("{}: {}", a.key, a.value), interp::assert(&a.key, &rhs, &ctx)));
    }
    o.response = Some(resp);
    o.runtime = runtime;
    o
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    /// Tiny HTTP server answering every request with a fixed JSON body; returns the requests it saw.
    fn serve(responses: usize) -> (String, std::thread::JoinHandle<Vec<String>>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = format!("127.0.0.1:{}", listener.local_addr().unwrap().port());
        let handle = std::thread::spawn(move || {
            let mut seen = vec![];
            for _ in 0..responses {
                let (mut s, _) = listener.accept().unwrap();
                let mut data = Vec::new();
                let mut buf = [0u8; 8192];
                // read headers, then exactly content-length body bytes
                loop {
                    let n = s.read(&mut buf).unwrap();
                    data.extend_from_slice(&buf[..n]);
                    let text = String::from_utf8_lossy(&data).to_lowercase();
                    if let Some(end) = text.find("\r\n\r\n") {
                        let len = text.lines().find_map(|l| l.strip_prefix("content-length:")).map(|v| v.trim().parse::<usize>().unwrap()).unwrap_or(0);
                        if data.len() >= end + 4 + len || n == 0 {
                            break;
                        }
                    }
                }
                seen.push(String::from_utf8_lossy(&data).to_string());
                let body = r#"{"id":7,"items":[1,2]}"#;
                write!(s, "HTTP/1.1 201 Created\r\ncontent-type: application/json\r\nset-cookie: sid=abc\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len()).unwrap();
            }
            seen
        });
        (addr, handle)
    }

    #[test]
    fn full_pipeline_with_scripts_vars_asserts_tests() {
        let (addr, server) = serve(1);
        let dir = std::env::temp_dir().join(format!("zenprobe-exec-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut c = Collection::create(&dir, "Demo", crate::model::Format::Yaml).unwrap();
        let mut root = c.load_folder(&dir);
        root.headers.push(Pair::new("x-col", "1"));
        root.vars_req.push(Pair::new("host", &addr));
        root.script_req = "bru.setVar('fromCollection', 'c');".into();
        root.tests = "test('collection test', () => expect(res.status).to.equal(201));".into();
        c.save_folder(&dir, &root).unwrap();
        let path = c.new_request(&dir, "Create", "http").unwrap();
        let mut req = crate::collection::load_request(&path).unwrap();
        req.method = "post".into();
        req.url = "{{host}}/users/:id".into();
        req.path_params.push(Pair::new("id", "{{uid}}"));
        req.body_mode = "json".into();
        req.body_json = r#"{"name":"{{name}}"}"#.into();
        req.script_req = "bru.setVar('uid', '9'); bru.setVar('name', 'zen'); req.setHeader('x-script', 'yes');".into();
        req.vars_res.push(Pair::new("newId", "res.body.id"));
        req.asserts.push(Pair::new("res.status", "eq 201"));
        req.asserts.push(Pair::new("res.body.items", "length 3"));
        req.tests = "test('id', () => { expect(res('id')).to.equal(7); expect(bru.getVar('newId')).to.equal(7); });".into();

        let job = Job { collection: c.clone(), env: None, secrets: Vars::new(), runtime: Runtime::new(), path: path.clone(), request: req };
        let o = run(job);
        assert_eq!(o.error, None, "{:?}", o.logs);
        let seen = server.join().unwrap();
        let raw = seen[0].to_lowercase();
        assert!(raw.starts_with("post /users/9 http/1.1"), "{raw}");
        assert!(raw.contains("x-col: 1") && raw.contains("x-script: yes") && raw.contains(r#"{"name":"zen"}"#), "{raw}");
        assert_eq!(o.response.as_ref().unwrap().status, 201);
        assert_eq!(o.runtime["newId"], json!(7));
        assert_eq!(o.runtime["fromCollection"], json!("c"));
        assert!(o.asserts[0].1.is_ok());
        assert!(o.asserts[1].1.is_err());
        let names: Vec<(&str, bool)> = o.tests.iter().map(|t| (t.name.as_str(), t.passed)).collect();
        assert_eq!(names, vec![("id", true), ("collection test", true)]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn multipart_and_graphql_bodies() {
        let (addr, server) = serve(2);
        let dir = std::env::temp_dir().join(format!("zenprobe-exec2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut c = Collection::create(&dir, "Demo", crate::model::Format::Bru).unwrap();
        std::fs::write(dir.join("upload.txt"), "FILEDATA").unwrap();
        let path = c.new_request(&dir, "Upload", "http").unwrap();
        let mut req = crate::collection::load_request(&path).unwrap();
        req.method = "post".into();
        req.url = format!("http://{addr}/up");
        req.body_mode = "multipartForm".into();
        req.multipart.push(Pair::new("title", "hello"));
        req.multipart.push(Pair { is_file: true, ..Pair::new("doc", "upload.txt") });
        let mut job = Job { collection: c.clone(), env: None, secrets: Vars::new(), runtime: Runtime::new(), path: path.clone(), request: req };
        assert_eq!(run(job.clone()).error, None);

        let mut gql = Request::new("q", 1, "graphql");
        gql.url = format!("http://{addr}/graphql");
        gql.gql_query = "query { user(id: \"{{id}}\") { name } }".into();
        gql.gql_vars = "{\"limit\": 2}".into();
        job.request = gql;
        job.runtime.insert("id".into(), json!("u1"));
        assert_eq!(run(job).error, None);

        let seen = server.join().unwrap();
        assert!(seen[0].contains("multipart/form-data; boundary=") && seen[0].contains("FILEDATA") && seen[0].contains("hello"), "{}", seen[0]);
        assert!(seen[1].contains(r#"{"query":"query { user(id: \"u1\") { name } }","variables":{"limit":2}}"#), "{}", seen[1]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

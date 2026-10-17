//! "Generate code" targets (Bruno uses httpsnippet; these cover its common targets)
//! built from a fully prepared request.

use crate::http::{BodyData, Prepared};

pub const LANGUAGES: &[&str] = &["Shell - cURL", "Shell - HTTPie", "PowerShell", "Python - requests", "JavaScript - fetch", "Node.js - axios", "Go - net/http", "Java - HttpClient", "C# - HttpClient", "Rust - reqwest", "PHP - cURL"];

/// Double-quoted string literal valid in JS, Python, Go, Java and C#.
fn s(v: &str) -> String {
    serde_json::Value::String(v.to_string()).to_string()
}

fn php(v: &str) -> String {
    format!("'{}'", v.replace('\\', "\\\\").replace('\'', "\\'"))
}

fn ps(v: &str) -> String {
    format!("'{}'", v.replace('\'', "''"))
}

/// Headers including basic auth and, for raw bodies, nothing else implied.
fn headers(p: &Prepared) -> Vec<(String, String)> {
    let mut h: Vec<(String, String)> = p
        .headers
        .iter()
        .filter(|(k, _)| !(matches!(p.body, BodyData::Multipart(_)) && k.eq_ignore_ascii_case("content-type")))
        .cloned()
        .collect();
    if let Some((u, pw)) = &p.basic_auth {
        h.push(("Authorization".into(), format!("Basic {}", crate::app::base64(format!("{u}:{pw}").as_bytes()))));
    }
    h
}

fn raw_body(p: &Prepared) -> Option<String> {
    match &p.body {
        BodyData::Raw(b) if !b.is_empty() => Some(b.clone()),
        BodyData::Form(f) => Some(f.iter().map(|(k, v)| format!("{}={}", enc(k), enc(v))).collect::<Vec<_>>().join("&")),
        _ => None,
    }
}

fn enc(v: &str) -> String {
    v.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            b' ' => "+".into(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

pub fn generate(lang: &str, p: &Prepared) -> String {
    let m = p.method.to_uppercase();
    let h = headers(p);
    let body = raw_body(p);
    let mut o = String::new();
    let mut line = |l: &str| {
        o.push_str(l);
        o.push('\n');
    };
    match lang {
        "Shell - HTTPie" => {
            let mut parts = vec![format!("http {m} '{}'", p.url.replace('\'', r"'\''"))];
            parts.extend(h.iter().map(|(k, v)| format!("  '{k}:{}'", v.replace('\'', r"'\''"))));
            let mut cmd = parts.join(" \\\n");
            if let Some(b) = &body {
                cmd = format!("printf '%s' '{}' | {cmd}", b.replace('\'', r"'\''"));
            }
            line(&cmd);
        }
        "PowerShell" => {
            line("$headers = @{");
            for (k, v) in &h {
                line(&format!("    {} = {}", ps(k), ps(v)));
            }
            line("}");
            let mut call = format!("$response = Invoke-WebRequest -Uri {} -Method {m} -Headers $headers", ps(&p.url));
            if let Some(b) = &body {
                line(&format!("$body = {}", ps(b)));
                call.push_str(" -Body $body");
            }
            if let BodyData::File(path, _) = &p.body {
                call.push_str(&format!(" -InFile {}", ps(&path.display().to_string())));
            }
            line(&call);
            line("$response.Content");
        }
        "Python - requests" => {
            line("import requests");
            line("");
            line(&format!("url = {}", s(&p.url)));
            line("headers = {");
            for (k, v) in &h {
                line(&format!("    {}: {},", s(k), s(v)));
            }
            line("}");
            let mut args = String::from("url, headers=headers");
            match (&p.body, &body) {
                (BodyData::Multipart(parts), _) => {
                    line("files = {");
                    for part in parts {
                        if part.files.is_empty() {
                            line(&format!("    {}: (None, {}),", s(&part.name), s(&part.value)));
                        }
                        for f in &part.files {
                            line(&format!("    {}: open({}, \"rb\"),", s(&part.name), s(&f.display().to_string())));
                        }
                    }
                    line("}");
                    args.push_str(", files=files");
                }
                (BodyData::File(path, _), _) => args.push_str(&format!(", data=open({}, \"rb\")", s(&path.display().to_string()))),
                (_, Some(b)) => {
                    line(&format!("payload = {}", s(b)));
                    args.push_str(", data=payload");
                }
                _ => {}
            }
            line("");
            line(&format!("response = requests.request({}, {args})", s(&m)));
            line("print(response.status_code)");
            line("print(response.text)");
        }
        "JavaScript - fetch" | "Node.js - axios" => {
            let axios = lang.starts_with("Node");
            if axios {
                line("const axios = require('axios');");
                line("");
            }
            let mut fields = vec![format!("  method: {}", s(&m))];
            if axios {
                fields.push(format!("  url: {}", s(&p.url)));
            }
            if !h.is_empty() {
                let hs: Vec<String> = h.iter().map(|(k, v)| format!("    {}: {}", s(k), s(v))).collect();
                fields.push(format!("  headers: {{\n{}\n  }}", hs.join(",\n")));
            }
            if let BodyData::Multipart(parts) = &p.body {
                line("const form = new FormData();");
                for part in parts {
                    if part.files.is_empty() {
                        line(&format!("form.append({}, {});", s(&part.name), s(&part.value)));
                    }
                    for f in &part.files {
                        line(&format!("form.append({}, /* file */ {});", s(&part.name), s(&f.display().to_string())));
                    }
                }
                line("");
                fields.push(format!("  {}: form", if axios { "data" } else { "body" }));
            } else if let Some(b) = &body {
                fields.push(format!("  {}: {}", if axios { "data" } else { "body" }, s(b)));
            }
            line(&format!("const options = {{\n{}\n}};", fields.join(",\n")));
            line("");
            if axios {
                line("axios.request(options)");
                line("  .then(response => console.log(response.data))");
                line("  .catch(error => console.error(error));");
            } else {
                line(&format!("fetch({}, options)", s(&p.url)));
                line("  .then(response => response.text())");
                line("  .then(text => console.log(text))");
                line("  .catch(error => console.error(error));");
            }
        }
        "Go - net/http" => {
            line("package main");
            line("");
            line("import (");
            line("\t\"fmt\"");
            line("\t\"io\"");
            line("\t\"net/http\"");
            if body.is_some() {
                line("\t\"strings\"");
            }
            line(")");
            line("");
            line("func main() {");
            let reader = match &body {
                Some(b) => format!("strings.NewReader({})", s(b)),
                None => "nil".into(),
            };
            line(&format!("\treq, err := http.NewRequest({}, {}, {reader})", s(&m), s(&p.url)));
            line("\tif err != nil {");
            line("\t\tpanic(err)");
            line("\t}");
            for (k, v) in &h {
                line(&format!("\treq.Header.Add({}, {})", s(k), s(v)));
            }
            line("\tres, err := http.DefaultClient.Do(req)");
            line("\tif err != nil {");
            line("\t\tpanic(err)");
            line("\t}");
            line("\tdefer res.Body.Close()");
            line("\tbody, _ := io.ReadAll(res.Body)");
            line("\tfmt.Println(res.Status)");
            line("\tfmt.Println(string(body))");
            line("}");
        }
        "Java - HttpClient" => {
            line("HttpRequest request = HttpRequest.newBuilder()");
            line(&format!("    .uri(URI.create({}))", s(&p.url)));
            for (k, v) in &h {
                line(&format!("    .header({}, {})", s(k), s(v)));
            }
            let publisher = match &body {
                Some(b) => format!("HttpRequest.BodyPublishers.ofString({})", s(b)),
                None => "HttpRequest.BodyPublishers.noBody()".into(),
            };
            line(&format!("    .method({}, {publisher})", s(&m)));
            line("    .build();");
            line("HttpResponse<String> response = HttpClient.newHttpClient().send(request, HttpResponse.BodyHandlers.ofString());");
            line("System.out.println(response.body());");
        }
        "C# - HttpClient" => {
            line("var client = new HttpClient();");
            line(&format!("var request = new HttpRequestMessage(new HttpMethod({}), {});", s(&m), s(&p.url)));
            let mut content_type = None;
            for (k, v) in &h {
                if k.eq_ignore_ascii_case("content-type") {
                    content_type = Some(v.clone());
                    continue;
                }
                line(&format!("request.Headers.TryAddWithoutValidation({}, {});", s(k), s(v)));
            }
            if let Some(b) = &body {
                line(&format!("request.Content = new StringContent({});", s(b)));
                if let Some(ct) = content_type {
                    line(&format!("request.Content.Headers.ContentType = System.Net.Http.Headers.MediaTypeHeaderValue.Parse({});", s(&ct)));
                }
            }
            line("var response = await client.SendAsync(request);");
            line("response.EnsureSuccessStatusCode();");
            line("Console.WriteLine(await response.Content.ReadAsStringAsync());");
        }
        "Rust - reqwest" => {
            line("let client = reqwest::Client::new();");
            line(&format!("let response = client.request(reqwest::Method::from_bytes(b{:?})?, {:?})", m, p.url));
            for (k, v) in &h {
                line(&format!("    .header({k:?}, {v:?})"));
            }
            if let Some(b) = &body {
                line(&format!("    .body({b:?})"));
            }
            line("    .send()");
            line("    .await?;");
            line("println!(\"{}\", response.text().await?);");
        }
        "PHP - cURL" => {
            line("<?php");
            line("$curl = curl_init();");
            line("curl_setopt_array($curl, [");
            line(&format!("    CURLOPT_URL => {},", php(&p.url)));
            line("    CURLOPT_RETURNTRANSFER => true,");
            line(&format!("    CURLOPT_CUSTOMREQUEST => {},", php(&m)));
            if p.follow_redirects {
                line("    CURLOPT_FOLLOWLOCATION => true,");
            }
            if let Some(b) = &body {
                line(&format!("    CURLOPT_POSTFIELDS => {},", php(b)));
            }
            if !h.is_empty() {
                line("    CURLOPT_HTTPHEADER => [");
                for (k, v) in &h {
                    line(&format!("        {},", php(&format!("{k}: {v}"))));
                }
                line("    ],");
            }
            line("]);");
            line("$response = curl_exec($curl);");
            line("curl_close($curl);");
            line("echo $response;");
        }
        _ => line(&crate::curl::to_curl(p)),
    }
    o.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prepared() -> Prepared {
        Prepared {
            method: "post".into(),
            url: "https://api.example.com/users?q=1".into(),
            headers: vec![("content-type".into(), "application/json".into())],
            body: BodyData::Raw("{\"name\":\"a'b\"}".into()),
            basic_auth: None,
            timeout_ms: 0,
            follow_redirects: true,
            max_redirects: 5,
        }
    }

    #[test]
    fn every_language_mentions_url_and_body() {
        let p = prepared();
        for lang in LANGUAGES {
            let code = generate(lang, &p);
            assert!(code.contains("api.example.com/users"), "{lang}");
            assert!(code.contains("name"), "{lang}: {code}");
        }
        assert!(generate("Python - requests", &p).contains("requests.request(\"POST\""));
        assert!(generate("PowerShell", &p).contains("'{\"name\":\"a''b\"}'"));
    }
}

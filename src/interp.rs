//! `{{var}}` interpolation (port of @usebruno/common interpolate) and the
//! `res.*` expressions used by `vars:post-response` and `assert` blocks.

use serde_json::Value;
use crate::faker::mock;
use std::collections::HashMap;

pub type Vars = HashMap<String, String>;

fn escape_json(s: &str) -> String {
    let quoted = Value::String(s.to_string()).to_string();
    quoted[1..quoted.len() - 1].to_string()
}

/// Replace `{{name}}` recursively; unknown names are left as-is (same as Bruno).
pub fn interpolate(s: &str, vars: &Vars, escape_json_strings: bool) -> String {
    fn go(s: &str, vars: &Vars, esc: bool, depth: usize) -> String {
        if depth > 16 {
            return s.to_string();
        }
        let mut out = String::new();
        let mut rest = s;
        while let Some(start) = rest.find("{{") {
            out.push_str(&rest[..start]);
            let after = &rest[start + 2..];
            match after.find("}}") {
                Some(end) if end > 0 && !after[..end].contains('}') => {
                    let key = &after[..end];
                    // Bruno JSON-escapes only generated mock values, never user variables
                    let found = match key.strip_prefix('$') {
                        Some(m) => mock(m).map(|v| if esc { escape_json(&v) } else { v }),
                        None => vars.get(key).map(|v| go(v, vars, esc, depth + 1)),
                    };
                    match found {
                        Some(v) => out.push_str(&v),
                        None => out.push_str(&rest[start..start + end + 4]),
                    }
                    rest = &after[end + 2..];
                }
                _ => {
                    out.push_str("{{");
                    rest = after;
                }
            }
        }
        out.push_str(rest);
        out
    }
    go(s, vars, escape_json_strings, 0)
}

pub struct ResCtx<'a> {
    pub status: u16,
    pub headers: &'a [(String, String)],
    pub body: &'a Value,
    pub time_ms: u128,
}

/// Evaluate `res.status`, `res.headers.x`, `res.body.a[0].b`, `res.responseTime`.
/// Returns None when the expression is not a `res` expression.
pub fn eval_res(expr: &str, res: &ResCtx) -> Option<Value> {
    let rest = expr.trim().strip_prefix("res.")?;
    let (head, path) = match rest.find(['.', '[']) {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };
    let root = match head {
        "status" => Value::from(res.status),
        "responseTime" => Value::from(res.time_ms as u64),
        "body" => res.body.clone(),
        "headers" => Value::Object(
            res.headers.iter().map(|(k, v)| (k.to_ascii_lowercase(), Value::String(v.clone()))).collect(),
        ),
        _ => return Some(Value::Null),
    };
    Some(walk(&root, path).cloned().unwrap_or(Value::Null))
}

fn walk<'v>(mut v: &'v Value, path: &str) -> Option<&'v Value> {
    let mut p = path;
    while !p.is_empty() {
        if let Some(r) = p.strip_prefix('.') {
            let end = r.find(['.', '[']).unwrap_or(r.len());
            v = v.get(&r[..end])?;
            p = &r[end..];
        } else if let Some(r) = p.strip_prefix('[') {
            let end = r.find(']')?;
            let key = r[..end].trim_matches(|c| c == '"' || c == '\'');
            v = match key.parse::<usize>() {
                Ok(i) => v.get(i)?,
                Err(_) => v.get(key)?,
            };
            p = &r[end + 1..];
        } else {
            return None;
        }
    }
    Some(v)
}

/// JSONPath subset used by the response filter: `$`, `.key`, `['key']`, `[0]`, `[-1]`,
/// `[*]`, `.*`, `..key` and `[a,b]`. Returns every match as an array (like jsonpath-plus).
pub fn json_path(root: &Value, path: &str) -> Result<Value, String> {
    let path = path.trim();
    let mut p = path.strip_prefix('$').unwrap_or(path);
    let mut cur: Vec<&Value> = vec![root];
    fn children(v: &Value) -> Vec<&Value> {
        match v {
            Value::Array(a) => a.iter().collect(),
            Value::Object(o) => o.values().collect(),
            _ => vec![],
        }
    }
    fn descend<'v>(v: &'v Value, out: &mut Vec<&'v Value>) {
        out.push(v);
        for c in children(v) {
            descend(c, out);
        }
    }
    fn select<'v>(v: &'v Value, key: &str) -> Vec<&'v Value> {
        if key == "*" {
            return children(v);
        }
        if let (Value::Array(a), Ok(i)) = (v, key.parse::<i64>()) {
            let i = if i < 0 { a.len() as i64 + i } else { i };
            return usize::try_from(i).ok().and_then(|i| a.get(i)).into_iter().collect();
        }
        v.get(key).into_iter().collect()
    }
    while !p.is_empty() {
        let recursive = p.starts_with("..");
        if recursive {
            let mut all = vec![];
            for v in &cur {
                descend(v, &mut all);
            }
            cur = all;
            p = &p[1..];
        }
        let keys: Vec<String> = if let Some(r) = p.strip_prefix('.') {
            let end = r.find(['.', '[']).unwrap_or(r.len());
            if end == 0 && !recursive {
                return Err(format!("unexpected '.' in {path}"));
            }
            p = &r[end..];
            if end == 0 { continue } else { vec![r[..end].to_string()] }
        } else if let Some(r) = p.strip_prefix('[') {
            let end = r.find(']').ok_or_else(|| format!("missing ']' in {path}"))?;
            p = &r[end + 1..];
            r[..end].split(',').map(|k| k.trim().trim_matches(|c| c == '"' || c == '\'').to_string()).collect()
        } else {
            return Err(format!("cannot parse JSONPath near `{p}`"));
        };
        cur = cur.iter().flat_map(|v| keys.iter().flat_map(move |k| select(v, k))).collect();
    }
    Ok(Value::Array(cur.into_iter().cloned().collect()))
}

/// RHS literal: JSON (number, bool, null, "string", array) or bare string.
fn literal(s: &str) -> Value {
    serde_json::from_str(s.trim()).unwrap_or_else(|_| Value::String(s.trim().to_string()))
}

fn as_f64(v: &Value) -> Option<f64> {
    v.as_f64().or_else(|| v.as_str()?.parse().ok())
}

fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64() != Some(0.0),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}

fn text(v: &Value) -> String {
    v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string())
}

/// Evaluate an assert like key `res.status`, value `eq 200`. Ok(()) = pass, Err(reason) = fail.
pub fn assert(lhs_expr: &str, op_and_rhs: &str, res: &ResCtx) -> Result<(), String> {
    let lhs = eval_res(lhs_expr, res).unwrap_or_else(|| literal(lhs_expr));
    let (op, rhs_raw) = op_and_rhs.trim().split_once(' ').unwrap_or((op_and_rhs.trim(), ""));
    let rhs = literal(rhs_raw);
    let cmp = |f: fn(f64, f64) -> bool| match (as_f64(&lhs), as_f64(&rhs)) {
        (Some(a), Some(b)) => f(a, b),
        _ => false,
    };
    let ok = match op {
        "eq" => lhs == rhs,
        "neq" => lhs != rhs,
        "gt" => cmp(|a, b| a > b),
        "gte" => cmp(|a, b| a >= b),
        "lt" => cmp(|a, b| a < b),
        "lte" => cmp(|a, b| a <= b),
        "in" => rhs.as_array().is_some_and(|a| a.contains(&lhs)),
        "notIn" => !rhs.as_array().is_some_and(|a| a.contains(&lhs)),
        "contains" => match &lhs {
            Value::Array(a) => a.contains(&rhs),
            _ => text(&lhs).contains(&text(&rhs)),
        },
        "notContains" => !text(&lhs).contains(&text(&rhs)),
        "startsWith" => text(&lhs).starts_with(&text(&rhs)),
        "endsWith" => text(&lhs).ends_with(&text(&rhs)),
        "length" => match &lhs {
            Value::Array(a) => Some(a.len() as f64) == as_f64(&rhs),
            Value::String(s) => Some(s.chars().count() as f64) == as_f64(&rhs),
            _ => false,
        },
        "isEmpty" => matches!(&lhs, Value::String(s) if s.is_empty()) || matches!(&lhs, Value::Array(a) if a.is_empty()) || matches!(&lhs, Value::Object(o) if o.is_empty()),
        "isNotEmpty" => !(matches!(&lhs, Value::String(s) if s.is_empty()) || matches!(&lhs, Value::Array(a) if a.is_empty()) || matches!(&lhs, Value::Object(o) if o.is_empty()) || lhs.is_null()),
        "isNull" => lhs.is_null(),
        "isUndefined" => lhs.is_null(),
        "isDefined" => !lhs.is_null(),
        "isTruthy" => truthy(&lhs),
        "isFalsy" => !truthy(&lhs),
        "isNumber" => lhs.is_number(),
        "isString" => lhs.is_string(),
        "isBoolean" => lhs.is_boolean(),
        "isArray" => lhs.is_array(),
        "isJson" => lhs.is_object() || lhs.is_array(),
        other => return Err(format!("unsupported operator `{other}`")),
    };
    if ok { Ok(()) } else { Err(format!("expected {lhs_expr} ({}) {op} {rhs_raw}", text(&lhs))) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn json_path_filter() {
        let v = json!({"store": {"books": [{"author": "A", "price": 5}, {"author": "B", "price": 9}], "owner": {"author": "C"}}});
        assert_eq!(json_path(&v, "$.store.books[0].author").unwrap(), json!(["A"]));
        assert_eq!(json_path(&v, "$.store.books[*].price").unwrap(), json!([5, 9]));
        assert_eq!(json_path(&v, "$.store.books[-1].author").unwrap(), json!(["B"]));
        assert_eq!(json_path(&v, "$..author").unwrap(), json!(["A", "B", "C"]));
        assert_eq!(json_path(&v, "$.store['owner']").unwrap(), json!([{"author": "C"}]));
        assert_eq!(json_path(&v, "$.missing").unwrap(), json!([]));
        assert!(json_path(&v, "$.store[0").is_err());
    }

    #[test]
    fn interpolates_recursively() {
        let vars: Vars = [("host", "{{proto}}://x"), ("proto", "https"), ("q", "a\"b")]
            .into_iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect();
        assert_eq!(interpolate("{{host}}/{{missing}}", &vars, false), "https://x/{{missing}}");
        assert_eq!(interpolate("{\"v\":\"{{q}}\"}", &vars, true), "{\"v\":\"a\"b\"}");
        // same as Bruno's /\{\{([^}]+)\}\}/: " {{host" is one unknown placeholder
        assert_eq!(interpolate("{{ {{host}}", &vars, false), "{{ {{host}}");
        let loop_vars: Vars = [("a".into(), "{{a}}".into())].into_iter().collect();
        assert_eq!(interpolate("{{a}}", &loop_vars, false), "{{a}}");
        assert_eq!(interpolate("{{$timestamp}}", &vars, false).len(), 10);
    }

    #[test]
    fn evaluates_res_and_asserts() {
        let body = json!({"data": {"items": [{"id": 7}]}, "name": "bru"});
        let headers = vec![("Content-Type".to_string(), "application/json".to_string())];
        let res = ResCtx { status: 200, headers: &headers, body: &body, time_ms: 12 };
        assert_eq!(eval_res("res.body.data.items[0].id", &res), Some(json!(7)));
        assert_eq!(eval_res("res.headers.content-type", &res), Some(json!("application/json")));
        assert_eq!(eval_res("plain", &res), None);
        assert!(assert("res.status", "eq 200", &res).is_ok());
        assert!(assert("res.status", "eq 201", &res).is_err());
        assert!(assert("res.body.name", "eq \"bru\"", &res).is_ok());
        assert!(assert("res.body.data.items", "isArray", &res).is_ok());
        assert!(assert("res.responseTime", "lt 1000", &res).is_ok());
        assert!(assert("res.body.name", "startsWith b", &res).is_ok());
        assert!(assert("res.status", "bogus 1", &res).is_err());
    }
}

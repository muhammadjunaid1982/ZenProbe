//! Pre-request / post-response scripts and tests, run in the embedded boa JS engine.
//! Implements Bruno's script API (bru, req, res, test, expect, console) in a JS prelude;
//! state goes in and out as JSON so no native bindings are needed.

use boa_engine::{Context, Source};
use serde_json::{Value, json};

const PRELUDE: &str = r#"
const __out = { logs: [], tests: [], runtime: {}, env: {}, nextRequest: undefined, skip: false, stop: false, req: null, resBody: undefined };
const __pending = [];
const __fmt = (v) => { if (typeof v === 'string') return v; if (v === undefined) return 'undefined'; try { return JSON.stringify(v); } catch (e) { return String(v); } };
const __show = (v) => typeof v === 'string' ? "'" + v + "'" : __fmt(v);
const console = {};
['log', 'info', 'warn', 'error', 'debug'].forEach((l) => { console[l] = (...a) => __out.logs.push([l, a.map(__fmt).join(' ')]); });
function __deq(a, b) {
  if (a === b) return true;
  if (typeof a !== 'object' || typeof b !== 'object' || a === null || b === null) return Number.isNaN(a) && Number.isNaN(b);
  if (Array.isArray(a) !== Array.isArray(b)) return false;
  const ka = Object.keys(a), kb = Object.keys(b);
  return ka.length === kb.length && ka.every((k) => __deq(a[k], b[k]));
}
function __get(obj, path) {
  if (path === undefined || path === null || path === '') return obj;
  const parts = String(path).replace(/\[(\w+)\]/g, '.$1').replace(/^\./, '').split('.');
  let cur = obj;
  for (const p of parts) { if (cur === null || cur === undefined) return undefined; cur = cur[p]; }
  return cur;
}
const __typeOf = (v) => v === null ? 'null' : Array.isArray(v) ? 'array' : typeof v;
class AssertionError extends Error { constructor(m) { super(m); this.name = 'AssertionError'; } }
function Assertion(obj) { this._obj = obj; this._not = false; this._deep = false; this._nested = false; }
const P = Assertion.prototype;
['to', 'be', 'been', 'is', 'that', 'which', 'and', 'has', 'have', 'with', 'at', 'of', 'same', 'but', 'does', 'still', 'also', 'any', 'all', 'own'].forEach((w) => Object.defineProperty(P, w, { get() { return this; } }));
Object.defineProperty(P, 'not', { get() { this._not = !this._not; return this; } });
Object.defineProperty(P, 'deep', { get() { this._deep = true; return this; } });
Object.defineProperty(P, 'nested', { get() { this._nested = true; return this; } });
P._assert = function (ok, msg, neg) { if (this._not ? ok : !ok) throw new AssertionError(this._not ? neg : msg); return this; };
const __cmp = (name, fn, word) => function (n) { return this._assert(fn(this._obj, n), `expected ${__show(this._obj)} to be ${word} ${__show(n)}`, `expected ${__show(this._obj)} to not be ${word} ${__show(n)}`); };
P.equal = P.equals = P.eq = function (v) { const ok = this._deep ? __deq(this._obj, v) : this._obj === v; return this._assert(ok, `expected ${__show(this._obj)} to ${this._deep ? 'deeply ' : ''}equal ${__show(v)}`, `expected ${__show(this._obj)} to not equal ${__show(v)}`); };
P.eql = P.eqls = function (v) { return this._assert(__deq(this._obj, v), `expected ${__show(this._obj)} to deeply equal ${__show(v)}`, `expected ${__show(this._obj)} to not deeply equal ${__show(v)}`); };
P.above = P.gt = P.greaterThan = __cmp('above', (a, b) => a > b, 'above');
P.least = P.gte = P.greaterThanOrEqual = __cmp('least', (a, b) => a >= b, 'at least');
P.below = P.lt = P.lessThan = __cmp('below', (a, b) => a < b, 'below');
P.most = P.lte = P.lessThanOrEqual = __cmp('most', (a, b) => a <= b, 'at most');
P.within = function (a, b) { return this._assert(this._obj >= a && this._obj <= b, `expected ${__show(this._obj)} to be within ${a}..${b}`, `expected ${__show(this._obj)} to not be within ${a}..${b}`); };
P.closeTo = P.approximately = function (e, d) { return this._assert(Math.abs(this._obj - e) <= d, `expected ${__show(this._obj)} to be close to ${e} +/- ${d}`, `expected ${__show(this._obj)} not to be close to ${e}`); };
P.a = P.an = function (t) { t = String(t).toLowerCase(); return this._assert(__typeOf(this._obj) === t, `expected ${__show(this._obj)} to be a ${t}`, `expected ${__show(this._obj)} not to be a ${t}`); };
P.include = P.includes = P.contain = P.contains = function (v) {
  const o = this._obj; let ok = false;
  if (typeof o === 'string') ok = o.includes(v);
  else if (Array.isArray(o)) ok = this._deep ? o.some((x) => __deq(x, v)) : o.includes(v);
  else if (o && typeof o === 'object') ok = (v && typeof v === 'object') ? Object.keys(v).every((k) => __deq(o[k], v[k])) : v in o;
  return this._assert(ok, `expected ${__show(o)} to include ${__show(v)}`, `expected ${__show(o)} to not include ${__show(v)}`);
};
P.property = function (name, ...val) {
  const o = this._obj;
  const value = this._nested ? __get(o, name) : (o === null || o === undefined ? undefined : o[name]);
  const has = this._nested ? value !== undefined : (o !== null && o !== undefined && (typeof o === 'object' ? name in o : value !== undefined));
  const ok = val.length ? has && (this._deep ? __deq(value, val[0]) : value === val[0]) : has;
  const want = val.length ? ` of ${__show(val[0])}` : '';
  this._assert(ok, `expected ${__show(o)} to have property '${name}'${want}`, `expected ${__show(o)} to not have property '${name}'${want}`);
  if (!this._not) this._obj = value;
  return this;
};
P.ownProperty = P.haveOwnProperty = function (name) { const o = this._obj; return this._assert(o != null && Object.prototype.hasOwnProperty.call(o, name), `expected ${__show(o)} to have own property '${name}'`, `expected ${__show(o)} to not have own property '${name}'`); };
P.keys = P.key = function (...k) { if (k.length === 1 && Array.isArray(k[0])) k = k[0]; const have = Object.keys(this._obj || {}); return this._assert(k.every((x) => have.includes(x)), `expected ${__show(this._obj)} to have keys ${__show(k)}`, `expected ${__show(this._obj)} to not have keys ${__show(k)}`); };
P.lengthOf = P.length = function (n) { const o = this._obj; const l = o == null ? undefined : (o.length !== undefined ? o.length : o.size); return this._assert(l === n, `expected ${__show(o)} to have length ${n} but got ${l}`, `expected ${__show(o)} to not have length ${n}`); };
P.match = P.matches = function (re) { return this._assert(re.test(this._obj), `expected ${__show(this._obj)} to match ${re}`, `expected ${__show(this._obj)} not to match ${re}`); };
P.string = function (s) { return this._assert(String(this._obj).includes(s), `expected ${__show(this._obj)} to contain ${__show(s)}`, `expected ${__show(this._obj)} to not contain ${__show(s)}`); };
P.oneOf = function (arr) { return this._assert(arr.some((x) => this._deep ? __deq(x, this._obj) : x === this._obj), `expected ${__show(this._obj)} to be one of ${__show(arr)}`, `expected ${__show(this._obj)} to not be one of ${__show(arr)}`); };
P.members = function (arr) { const o = this._obj || []; return this._assert(o.length === arr.length && arr.every((x) => o.some((y) => __deq(x, y))), `expected ${__show(o)} to have the same members as ${__show(arr)}`, `expected ${__show(o)} to not have the same members as ${__show(arr)}`); };
P.instanceOf = P.instanceof = function (C) { return this._assert(this._obj instanceof C, `expected ${__show(this._obj)} to be an instance of ${C.name}`, `expected ${__show(this._obj)} to not be an instance of ${C.name}`); };
P.satisfy = P.satisfies = function (fn) { return this._assert(!!fn(this._obj), `expected ${__show(this._obj)} to satisfy the given function`, `expected ${__show(this._obj)} to not satisfy the given function`); };
P.throw = P.throws = P.Throw = function () { let threw = false; try { this._obj(); } catch (e) { threw = true; } return this._assert(threw, 'expected function to throw', 'expected function to not throw'); };
P.status = function (code) { const s = this._obj && (typeof this._obj.getStatus === 'function' ? this._obj.getStatus() : this._obj.status); return this._assert(s === code, `expected response to have status ${code} but got ${s}`, `expected response to not have status ${code}`); };
P.header = function (name, ...val) { const r = this._obj; const v = r && (typeof r.getHeader === 'function' ? r.getHeader(name) : (r.headers || {})[String(name).toLowerCase()]); const ok = val.length ? v === val[0] : v !== undefined; return this._assert(ok, `expected response to have header ${name}`, `expected response to not have header ${name}`); };
const __getter = (name, fn, word) => Object.defineProperty(P, name, { get() { return this._assert(fn(this._obj), `expected ${__show(this._obj)} to be ${word}`, `expected ${__show(this._obj)} to not be ${word}`); } });
__getter('ok', (v) => !!v, 'truthy');
__getter('true', (v) => v === true, 'true');
__getter('false', (v) => v === false, 'false');
__getter('null', (v) => v === null, 'null');
__getter('undefined', (v) => v === undefined, 'undefined');
__getter('NaN', (v) => Number.isNaN(v), 'NaN');
__getter('exist', (v) => v !== null && v !== undefined, 'existing');
__getter('finite', (v) => Number.isFinite(v), 'finite');
__getter('json', (v) => { if (typeof v === 'object' && v !== null) return true; try { JSON.parse(v); return true; } catch (e) { return false; } }, 'json');
__getter('empty', (v) => (typeof v === 'string' || Array.isArray(v)) ? v.length === 0 : (v instanceof Map || v instanceof Set) ? v.size === 0 : (v && typeof v === 'object') ? Object.keys(v).length === 0 : false, 'empty');
function expect(v) { return new Assertion(v); }
expect.fail = (m) => { throw new AssertionError(m || 'expect.fail()'); };
function assert(v, m) { if (!v) throw new AssertionError(m || 'assertion failed'); }
assert.ok = assert;
assert.equal = (a, b, m) => { if (a != b) throw new AssertionError(m || `expected ${__show(a)} to equal ${__show(b)}`); };
assert.notEqual = (a, b, m) => { if (a == b) throw new AssertionError(m || `expected ${__show(a)} to not equal ${__show(b)}`); };
assert.strictEqual = (a, b, m) => { if (a !== b) throw new AssertionError(m || `expected ${__show(a)} to strictly equal ${__show(b)}`); };
assert.deepEqual = (a, b, m) => { if (!__deq(a, b)) throw new AssertionError(m || `expected ${__show(a)} to deeply equal ${__show(b)}`); };
assert.isTrue = (v, m) => assert(v === true, m || `expected ${__show(v)} to be true`);
assert.isFalse = (v, m) => assert(v === false, m || `expected ${__show(v)} to be false`);
assert.exists = (v, m) => assert(v !== null && v !== undefined, m || 'expected value to exist');
assert.fail = (m) => { throw new AssertionError(m || 'assert.fail()'); };
function test(name, fn) {
  const r = { name: String(name), passed: true, error: null };
  __out.tests.push(r);
  const fail = (e) => { r.passed = false; r.error = String((e && e.message) || e); };
  try { const p = fn(); if (p && typeof p.then === 'function') __pending.push(p.then(() => {}, fail)); } catch (e) { fail(e); }
}
const __b64 = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';
function btoa(s) { s = String(s); let o = ''; for (let i = 0; i < s.length; i += 3) { const a = s.charCodeAt(i), b = s.charCodeAt(i + 1), c = s.charCodeAt(i + 2); const n = (a << 16) | ((b || 0) << 8) | (c || 0); o += __b64[(n >> 18) & 63] + __b64[(n >> 12) & 63] + (isNaN(b) ? '=' : __b64[(n >> 6) & 63]) + (isNaN(c) ? '=' : __b64[n & 63]); } return o; }
function atob(s) { s = String(s).replace(/=+$/, ''); let o = '', bits = 0, buf = 0; for (const ch of s) { buf = (buf << 6) | __b64.indexOf(ch); bits += 6; if (bits >= 8) { bits -= 8; o += String.fromCharCode((buf >> bits) & 255); } } return o; }
function __sleepSync(ms) { const end = Date.now() + Number(ms || 0); while (Date.now() < end) {} }
function setTimeout(fn, ms, ...args) { __sleepSync(ms); Promise.resolve().then(() => fn(...args)); return 0; }
function clearTimeout() {}
function require(name) {
  if (name === 'chai') return { expect, assert, AssertionError };
  throw new Error(`require('${name}') is not available in ZenBruno scripts`);
}
"#;

const SETUP: &str = r#"
const __v = __ctx.vars;
const __interp = (s) => {
  const all = Object.assign({}, __v.global, __v.collection, __v.env, __v.folder, __v.request, __v.runtime);
  let out = String(s), guard = 0, prev;
  do { prev = out; out = out.replace(/\{\{([^}]+)\}\}/g, (m, k) => { const v = k.startsWith('process.env.') ? __v.processEnv[k.slice(12)] : all[k]; return v === undefined ? m : (typeof v === 'object' ? JSON.stringify(v) : String(v)); }); } while (out !== prev && ++guard < 10);
  return out;
};
const bru = {
  cwd: () => __ctx.cwd,
  getEnvName: () => __ctx.envName,
  getCollectionName: () => __ctx.collectionName,
  getProcessEnv: (k) => __v.processEnv[k],
  hasEnvVar: (k) => Object.prototype.hasOwnProperty.call(__v.env, k),
  getEnvVar: (k) => __v.env[k],
  setEnvVar: (k, v) => { __v.env[k] = v; __out.env[k] = v; },
  deleteEnvVar: (k) => { delete __v.env[k]; __out.env[k] = null; },
  getGlobalEnvVar: (k) => __v.global[k],
  setGlobalEnvVar: (k, v) => { __v.global[k] = v; },
  hasVar: (k) => Object.prototype.hasOwnProperty.call(__v.runtime, k),
  getVar: (k) => __v.runtime[k],
  setVar: (k, v) => { if (!/^[\w\-.]+$/.test(k)) throw new Error(`Variable name '${k}' contains invalid characters`); __v.runtime[k] = v; __out.runtime[k] = v; },
  deleteVar: (k) => { delete __v.runtime[k]; __out.runtime[k] = null; },
  deleteAllVars: () => { for (const k of Object.keys(__v.runtime)) { delete __v.runtime[k]; __out.runtime[k] = null; } },
  getAllVars: () => Object.assign({}, __v.runtime),
  getCollectionVar: (k) => __v.collection[k],
  getFolderVar: (k) => __v.folder[k],
  getRequestVar: (k) => __v.request[k],
  getRequestName: () => __ctx.req.name,
  setNextRequest: (n) => { __out.nextRequest = n; },
  runner: { setNextRequest: (n) => { __out.nextRequest = n; }, skipRequest: () => { __out.skip = true; }, stopExecution: () => { __out.stop = true; } },
  sleep: (ms) => { __sleepSync(ms); return Promise.resolve(); },
  interpolate: (s) => typeof s === 'string' ? __interp(s) : JSON.parse(__interp(JSON.stringify(s))),
};
const req = (() => {
  const r = __ctx.req;
  let body = r.body;
  if (r.bodyMode === 'json' && typeof body === 'string') { try { body = JSON.parse(body); } catch (e) {} }
  const o = {
    url: r.url, method: r.method, headers: r.headers, body, timeout: r.timeout, name: r.name,
    getUrl() { return this.url; }, setUrl(u) { this.url = u; },
    getMethod() { return this.method; }, setMethod(m) { this.method = m; },
    getName() { return this.name; }, getAuthMode() { return r.authMode; },
    getHeader(n) { const k = Object.keys(this.headers).find((h) => h.toLowerCase() === String(n).toLowerCase()); return k === undefined ? undefined : this.headers[k]; },
    getHeaders() { return this.headers; },
    setHeader(n, v) { const k = Object.keys(this.headers).find((h) => h.toLowerCase() === String(n).toLowerCase()); if (k !== undefined) delete this.headers[k]; this.headers[n] = String(v); },
    setHeaders(h) { this.headers = h; },
    deleteHeader(n) { const k = Object.keys(this.headers).find((h) => h.toLowerCase() === String(n).toLowerCase()); if (k !== undefined) delete this.headers[k]; },
    getBody() { return this.body; }, setBody(b) { this.body = b; },
    getTimeout() { return this.timeout; }, setTimeout(t) { this.timeout = t; },
    setMaxRedirects(n) { this.maxRedirects = n; },
  };
  return o;
})();
const res = (() => {
  const r = __ctx.res;
  if (!r) return undefined;
  const f = (path) => __get(f.body, path);
  Object.assign(f, { status: r.status, statusText: r.statusText, headers: r.headers, body: r.body, responseTime: r.responseTime, url: r.url });
  f.getStatus = () => f.status; f.getStatusText = () => f.statusText;
  f.getHeader = (n) => f.headers[String(n).toLowerCase()]; f.getHeaders = () => f.headers;
  f.getBody = () => f.body; f.setBody = (b) => { f.body = b; __out.resBody = b; };
  f.getResponseTime = () => f.responseTime; f.getUrl = () => f.url; f.getSize = () => r.size;
  return f;
})();
function __finish(e) {
  if (e) __out.error = String((e && e.stack && e.message ? e.name + ': ' + e.message : e));
  __out.req = { url: req.url, method: req.method, headers: req.headers, timeout: req.timeout,
    body: req.body === undefined || req.body === null ? req.body : (typeof req.body === 'string' ? req.body : JSON.stringify(req.body)) };
}
"#;

/// Input: { req, res?, vars: {runtime, env, collection, folder, request, global, processEnv}, envName, collectionName, cwd }
/// Output: { logs, tests, runtime, env, nextRequest, skip, stop, req, resBody, error }
pub fn run(segments: &[&str], input: &Value) -> Value {
    let code: Vec<String> = segments
        .iter()
        .filter(|s| !s.trim().is_empty())
        .map(|s| format!("await (async () => {{\n{s}\n}})();"))
        .collect();
    let mut ctx = Context::default();
    let src = format!(
        "{PRELUDE}\nconst __ctx = {input};\n{SETUP}\n(async () => {{\n{}\n}})().then(() => Promise.all(__pending)).then(() => __finish(null), (e) => __finish(e));",
        code.join("\n\n")
    );
    let failure = |msg: String| json!({ "error": msg, "logs": [], "tests": [], "runtime": {}, "env": {} });
    if let Err(e) = ctx.eval(Source::from_bytes(&src)) {
        return failure(e.to_string());
    }
    if let Err(e) = ctx.run_jobs() {
        return failure(e.to_string());
    }
    match ctx.eval(Source::from_bytes("JSON.stringify(__out)")).and_then(|v| v.to_string(&mut ctx)) {
        Ok(s) => serde_json::from_str(&s.to_std_string_escaped()).unwrap_or_else(|e| failure(e.to_string())),
        Err(e) => failure(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> Value {
        json!({
            "req": { "url": "{{host}}/a", "method": "GET", "headers": { "X-A": "1" }, "body": "{\"n\":1}", "bodyMode": "json", "timeout": 0, "name": "r" },
            "res": { "status": 200, "statusText": "OK", "headers": { "content-type": "application/json" }, "body": { "items": [{ "id": 7 }] }, "responseTime": 5, "url": "u", "size": 10 },
            "vars": { "runtime": {}, "env": { "host": "h" }, "collection": {}, "folder": {}, "request": {}, "global": {}, "processEnv": { "HOME": "x" } },
            "envName": "dev", "collectionName": "c", "cwd": "."
        })
    }

    #[test]
    fn bru_api_and_req_mutation() {
        let out = run(
            &[
                "bru.setVar('token', 'abc'); bru.setEnvVar('host', 'h2');",
                "req.setHeader('x-a', '2'); req.setUrl(bru.interpolate('{{host}}/b')); const b = req.getBody(); b.n = 2; req.setBody(b); console.log('hi', {a: 1});",
            ],
            &input(),
        );
        assert_eq!(out["error"], Value::Null, "{out}");
        assert_eq!(out["runtime"]["token"], "abc");
        assert_eq!(out["env"]["host"], "h2");
        assert_eq!(out["req"]["url"], "h2/b");
        assert_eq!(out["req"]["headers"]["x-a"], "2");
        assert_eq!(out["req"]["body"], "{\"n\":2}");
        assert_eq!(out["logs"][0][1], "hi {\"a\":1}");
    }

    #[test]
    fn tests_and_expect() {
        let out = run(
            &[r#"
            test("status", () => expect(res.getStatus()).to.equal(200));
            test("body", () => { expect(res('items[0].id')).to.eql(7); expect(res.body).to.have.property('items').that.is.an('array').with.lengthOf(1); });
            test("fails", () => expect(res.status).to.not.equal(200));
            test("async", async () => { await bru.sleep(1); expect([1, 2]).to.deep.include(2); expect({a: {b: 1}}).to.have.nested.property('a.b', 1); });
            test("chain", () => { expect('abc').to.be.a('string').and.include('b'); expect(null).to.be.null; expect(5).to.be.within(1, 10); expect(res).to.have.status(200); });
            bru.runner.skipRequest();
            "#],
            &input(),
        );
        assert_eq!(out["error"], Value::Null, "{out}");
        let tests = out["tests"].as_array().unwrap();
        let passed: Vec<bool> = tests.iter().map(|t| t["passed"].as_bool().unwrap()).collect();
        assert_eq!(passed, vec![true, true, false, true, true], "{out}");
        assert!(tests[2]["error"].as_str().unwrap().contains("to not equal 200"));
        assert_eq!(out["skip"], true);
    }

    #[test]
    fn errors_are_reported() {
        let out = run(&["throw new Error('boom')"], &input());
        assert!(out["error"].as_str().unwrap().contains("boom"), "{out}");
        let out = run(&["this is not js"], &input());
        assert!(out["error"].is_string());
    }
}

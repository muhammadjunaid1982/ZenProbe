//! A collection on disk, in either Bruno format:
//! .bru  — bruno.json, collection.bru, folder.bru, *.bru, environments/*.bru
//! YAML  — opencollection.yml, folder.yml, *.yml, environments/*.yml

use crate::interp::Vars;
use crate::model::{Environment, FolderSettings, Format, Pair, Request};
use crate::{bru, yml};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub enum Node {
    Folder { name: String, path: PathBuf, children: Vec<Node> },
    Request { name: String, method: String, kind: String, path: PathBuf },
}

#[derive(Debug, Clone, Default)]
pub struct Collection {
    pub root: PathBuf,
    pub name: String,
    pub format: Format,
    pub tree: Vec<Node>,
    pub envs: Vec<Environment>,
    pub dotenv: Vars,
}

const SKIP_DIRS: &[&str] = &["environments", "node_modules", ".git"];

fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

fn write(path: &Path, text: &str) -> Result<(), String> {
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}

pub fn format_of(path: &Path) -> Format {
    if path.extension().is_some_and(|e| e == "yml" || e == "yaml") { Format::Yaml } else { Format::Bru }
}

pub fn load_request(path: &Path) -> Result<Request, String> {
    let text = read(path)?;
    match format_of(path) {
        Format::Bru => bru::parse_request(&text),
        Format::Yaml => yml::parse_request(&text),
    }
    .map_err(|e| format!("{}: {e}", path.display()))
}

pub fn request_text(path: &Path, r: &Request) -> String {
    match format_of(path) {
        Format::Bru => bru::write_request(r),
        Format::Yaml => yml::write_request(r),
    }
}

pub fn save_request(path: &Path, r: &Request) -> Result<(), String> {
    write(path, &request_text(path, r))
}

/// Windows-safe file name from a display name (Bruno replaces the same characters).
pub fn safe_name(name: &str) -> String {
    name.trim().chars().map(|c| if "<>:\"/\\|?*".contains(c) || c.is_control() { '-' } else { c }).collect()
}

pub fn parse_dotenv(src: &str) -> Vars {
    src.lines()
        .filter_map(|l| {
            let l = l.trim();
            if l.starts_with('#') {
                return None;
            }
            let (k, v) = l.split_once('=')?;
            let v = v.trim();
            let v = v.strip_prefix('"').and_then(|x| x.strip_suffix('"')).unwrap_or(v);
            Some((k.trim().to_string(), v.to_string()))
        })
        .collect()
}

impl Collection {
    pub fn open(root: &Path) -> Result<Self, String> {
        let (format, name) = if root.join("opencollection.yml").exists() {
            let f = yml::parse_folder(&read(&root.join("opencollection.yml"))?)?;
            (Format::Yaml, f.name)
        } else if root.join("bruno.json").exists() {
            let cfg: serde_json::Value = serde_json::from_str(&read(&root.join("bruno.json"))?).map_err(|e| format!("bruno.json: {e}"))?;
            (Format::Bru, cfg["name"].as_str().unwrap_or_default().to_string())
        } else {
            return Err(format!("{} is not a Bruno collection (no bruno.json or opencollection.yml)", root.display()));
        };
        let name = if name.is_empty() { root.file_name().unwrap_or_default().to_string_lossy().to_string() } else { name };
        let mut c = Collection { root: root.to_path_buf(), name, format, ..Default::default() };
        c.reload();
        Ok(c)
    }

    pub fn create(root: &Path, name: &str, format: Format) -> Result<Self, String> {
        if root.exists() && std::fs::read_dir(root).map(|mut d| d.next().is_some()).unwrap_or(false) {
            return Err(format!("{} already exists and is not empty", root.display()));
        }
        std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
        match format {
            Format::Bru => {
                let cfg = serde_json::json!({ "version": "1", "name": name, "type": "collection", "ignore": ["node_modules", ".git"] });
                write(&root.join("bruno.json"), &serde_json::to_string_pretty(&cfg).unwrap())?;
            }
            Format::Yaml => {
                let f = FolderSettings { name: name.into(), ..Default::default() };
                let mut text = yml::write_folder(&f, true);
                text += "bundled: false\nextensions:\n  bruno:\n    ignore:\n      - node_modules\n      - .git\n";
                write(&root.join("opencollection.yml"), &text)?;
            }
        }
        Self::open(root)
    }

    pub fn reload(&mut self) {
        self.tree = self.scan(&self.root.clone());
        self.dotenv = std::fs::read_to_string(self.root.join(".env")).map(|s| parse_dotenv(&s)).unwrap_or_default();
        self.envs.clear();
        if let Ok(rd) = std::fs::read_dir(self.root.join("environments")) {
            for e in rd.flatten() {
                let path = e.path();
                if path.extension().is_some_and(|x| x == self.format.ext() || (self.format == Format::Yaml && x == "yaml")) {
                    // an unparsable env file is skipped rather than shown empty and overwritten
                    if let Ok(env) = load_env(&path) {
                        self.envs.push(env);
                    }
                }
            }
        }
        self.envs.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    }

    fn is_request_file(&self, path: &Path) -> bool {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let ext_ok = match self.format {
            Format::Bru => name.ends_with(".bru"),
            Format::Yaml => name.ends_with(".yml") || name.ends_with(".yaml"),
        };
        ext_ok && !matches!(name.as_ref(), "collection.bru" | "folder.bru" | "opencollection.yml" | "folder.yml")
    }

    fn scan(&self, dir: &Path) -> Vec<Node> {
        let mut items: Vec<(f64, String, Node)> = Vec::new();
        let Ok(rd) = std::fs::read_dir(dir) else { return vec![] };
        for entry in rd.flatten() {
            let path = entry.path();
            let fname = entry.file_name().to_string_lossy().to_string();
            if path.is_dir() {
                if fname.starts_with('.') || SKIP_DIRS.contains(&fname.as_str()) && dir == self.root {
                    continue;
                }
                let fs = self.load_folder(&path);
                let name = if fs.name.is_empty() { fname.clone() } else { fs.name.clone() };
                let seq = fs.seq.parse().unwrap_or(f64::MAX);
                items.push((seq, name.to_lowercase(), Node::Folder { name, children: self.scan(&path), path }));
            } else if self.is_request_file(&path) {
                match load_request(&path) {
                    Ok(r) => {
                        let name = if r.name.is_empty() { path.file_stem().unwrap_or_default().to_string_lossy().to_string() } else { r.name };
                        items.push((r.seq.parse().unwrap_or(f64::MAX), name.to_lowercase(), Node::Request { name, method: r.method, kind: r.kind, path }));
                    }
                    Err(_) => {
                        let name = format!("⚠ {fname}");
                        items.push((f64::MAX, name.to_lowercase(), Node::Request { name, method: String::new(), kind: "http".into(), path }));
                    }
                }
            }
        }
        items.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        items.into_iter().map(|(_, _, n)| n).collect()
    }

    pub fn folder_file(&self, dir: &Path) -> PathBuf {
        let is_root = dir == self.root;
        dir.join(match (self.format, is_root) {
            (Format::Bru, true) => "collection.bru",
            (Format::Bru, false) => "folder.bru",
            (Format::Yaml, true) => "opencollection.yml",
            (Format::Yaml, false) => "folder.yml",
        })
    }

    /// Settings of the collection root or a folder (empty defaults when the file is missing).
    pub fn load_folder(&self, dir: &Path) -> FolderSettings {
        let file = self.folder_file(dir);
        let parsed = std::fs::read_to_string(&file).ok().and_then(|t| match self.format {
            Format::Bru => bru::parse_folder(&t).ok(),
            Format::Yaml => yml::parse_folder(&t).ok(),
        });
        let mut f = parsed.unwrap_or_default();
        if f.auth.mode.is_empty() {
            f.auth.mode = "none".into();
        }
        f
    }

    pub fn save_folder(&self, dir: &Path, f: &FolderSettings) -> Result<(), String> {
        let is_root = dir == self.root;
        let text = match self.format {
            Format::Bru => bru::write_folder(f, is_root),
            Format::Yaml => yml::write_folder(f, is_root),
        };
        write(&self.folder_file(dir), &text)
    }

    /// Collection root first, then each folder down to the request's directory.
    pub fn settings_chain(&self, request_path: &Path) -> Vec<FolderSettings> {
        let mut chain = vec![self.load_folder(&self.root)];
        let rel = request_path.parent().and_then(|p| p.strip_prefix(&self.root).ok());
        let mut dir = self.root.clone();
        for part in rel.into_iter().flat_map(|r| r.components()) {
            dir.push(part);
            chain.push(self.load_folder(&dir));
        }
        chain
    }

    pub fn request_count(&self) -> usize {
        fn count(n: &[Node]) -> usize {
            n.iter().map(|x| match x { Node::Folder { children, .. } => count(children), _ => 1 }).sum()
        }
        count(&self.tree)
    }

    /// Requests in sidebar order under `dir` (the collection runner's order).
    pub fn requests_under(&self, dir: &Path) -> Vec<PathBuf> {
        fn walk(nodes: &[Node], dir: &Path, inside: bool, out: &mut Vec<PathBuf>) {
            for n in nodes {
                match n {
                    Node::Folder { path, children, .. } => walk(children, dir, inside || path == dir, out),
                    Node::Request { path, .. } if inside => out.push(path.clone()),
                    _ => {}
                }
            }
        }
        let mut out = vec![];
        walk(&self.tree, dir, dir == self.root, &mut out);
        out
    }

    fn next_seq(&self, dir: &Path) -> usize {
        std::fs::read_dir(dir).map(|rd| rd.flatten().filter(|e| self.is_request_file(&e.path()) || e.path().is_dir()).count()).unwrap_or(0) + 1
    }

    pub fn new_request(&mut self, dir: &Path, name: &str, kind: &str) -> Result<PathBuf, String> {
        self.add_request(dir, Request::new(name.trim(), 0, kind))
    }

    /// Saves a ready-made request (e.g. imported from cURL) as a new file in `dir`.
    pub fn add_request(&mut self, dir: &Path, mut req: Request) -> Result<PathBuf, String> {
        let path = dir.join(format!("{}.{}", safe_name(&req.name), self.format.ext()));
        if path.exists() {
            return Err(format!("{} already exists", path.display()));
        }
        req.seq = self.next_seq(dir).to_string();
        save_request(&path, &req)?;
        self.reload();
        Ok(path)
    }

    pub fn new_folder(&mut self, parent: &Path, name: &str) -> Result<PathBuf, String> {
        let dir = parent.join(safe_name(name));
        if dir.exists() {
            return Err(format!("{} already exists", dir.display()));
        }
        let seq = self.next_seq(parent).to_string();
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let f = FolderSettings { name: name.trim().into(), seq, auth: crate::model::Auth { mode: "inherit".into(), ..Default::default() }, ..Default::default() };
        self.save_folder(&dir, &f)?;
        self.reload();
        Ok(dir)
    }

    /// Renames the display name and the file; returns the new path.
    pub fn rename_request(&mut self, path: &Path, name: &str) -> Result<PathBuf, String> {
        let mut r = load_request(path)?;
        r.name = name.trim().into();
        let new_path = path.with_file_name(format!("{}.{}", safe_name(name), path.extension().unwrap_or_default().to_string_lossy()));
        if new_path != path && new_path.exists() {
            return Err(format!("{} already exists", new_path.display()));
        }
        save_request(path, &r)?;
        if new_path != path {
            std::fs::rename(path, &new_path).map_err(|e| e.to_string())?;
        }
        self.reload();
        Ok(new_path)
    }

    pub fn clone_request(&mut self, path: &Path, name: &str) -> Result<PathBuf, String> {
        let mut r = load_request(path)?;
        r.name = name.trim().into();
        let dir = path.parent().unwrap_or(&self.root).to_path_buf();
        r.seq = self.next_seq(&dir).to_string();
        let new_path = dir.join(format!("{}.{}", safe_name(name), self.format.ext()));
        if new_path.exists() {
            return Err(format!("{} already exists", new_path.display()));
        }
        save_request(&new_path, &r)?;
        self.reload();
        Ok(new_path)
    }

    pub fn rename_folder(&mut self, dir: &Path, name: &str) -> Result<PathBuf, String> {
        let mut f = self.load_folder(dir);
        f.name = name.trim().into();
        self.save_folder(dir, &f)?;
        let new_dir = dir.with_file_name(safe_name(name));
        if new_dir != dir {
            if new_dir.exists() {
                return Err(format!("{} already exists", new_dir.display()));
            }
            std::fs::rename(dir, &new_dir).map_err(|e| e.to_string())?;
        }
        self.reload();
        Ok(new_dir)
    }

    pub fn delete(&mut self, path: &Path) -> Result<(), String> {
        if !path.starts_with(&self.root) || path == self.root {
            return Err("refusing to delete outside the collection".into());
        }
        if path.is_dir() { std::fs::remove_dir_all(path) } else { std::fs::remove_file(path) }.map_err(|e| e.to_string())?;
        self.reload();
        Ok(())
    }

    pub fn new_env(&mut self, name: &str) -> Result<(), String> {
        let dir = self.root.join("environments");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = dir.join(format!("{}.{}", safe_name(name), self.format.ext()));
        if path.exists() {
            return Err(format!("{} already exists", path.display()));
        }
        save_env(&Environment { name: name.trim().into(), path, ..Default::default() })?;
        self.reload();
        Ok(())
    }
}

pub fn load_env(path: &Path) -> Result<Environment, String> {
    let text = read(path)?;
    let mut env = match format_of(path) {
        Format::Bru => bru::parse_env(&text)?,
        Format::Yaml => yml::parse_env(&text)?,
    };
    if env.name.is_empty() {
        env.name = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
    }
    env.path = path.to_path_buf();
    Ok(env)
}

pub fn save_env(env: &Environment) -> Result<(), String> {
    let text = match format_of(&env.path) {
        Format::Bru => bru::write_env(env),
        Format::Yaml => yml::write_env(env),
    };
    write(&env.path, &text)
}

// ---------- URL helpers ----------

/// Replace `/:name` path segments (Bruno's traditional path params).
pub fn apply_path_params(url: &str, params: &[(String, String)]) -> String {
    let (base, query) = match url.find(['?', '#']) {
        Some(i) => url.split_at(i),
        None => (url, ""),
    };
    let (scheme_host, path) = match base.find("://").map(|i| i + 3).and_then(|s| base[s..].find('/').map(|p| s + p)) {
        Some(i) => base.split_at(i),
        None => (base, ""),
    };
    let path: Vec<String> = path
        .split('/')
        .map(|seg| match seg.strip_prefix(':').and_then(|n| params.iter().find(|(k, v)| k == n && !v.is_empty())) {
            Some((_, v)) => v.clone(),
            None => seg.to_string(),
        })
        .collect();
    format!("{scheme_host}{}{query}", path.join("/"))
}

/// Query string pairs in the URL (Bruno keeps params:query in sync with the URL).
pub fn query_from_url(url: &str) -> Vec<(String, String)> {
    let Some(q) = url.split_once('?').map(|(_, q)| q.split('#').next().unwrap_or("")) else { return vec![] };
    q.split('&')
        .filter(|s| !s.is_empty())
        .map(|kv| {
            let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
            (k.to_string(), v.to_string())
        })
        .collect()
}

pub fn url_with_query(url: &str, params: &[Pair]) -> String {
    let base = url.split('?').next().unwrap_or(url);
    let q: Vec<String> = params
        .iter()
        .filter(|p| p.enabled && !p.key.is_empty())
        .map(|p| if p.value.is_empty() { p.key.clone() } else { format!("{}={}", p.key, p.value) })
        .collect();
    if q.is_empty() { base.to_string() } else { format!("{base}?{}", q.join("&")) }
}

/// Path param names (`:id`) present in the URL path.
pub fn path_params_in_url(url: &str) -> Vec<String> {
    let base = url.split(['?', '#']).next().unwrap_or(url);
    let path = base.split_once("://").map(|(_, r)| r).unwrap_or(base);
    path.split('/').skip(1).filter_map(|s| s.strip_prefix(':')).filter(|s| !s.is_empty()).map(str::to_string).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_helpers() {
        let p = vec![("id".to_string(), "42".to_string())];
        assert_eq!(apply_path_params("http://h:8080/users/:id/x?y=:id", &p), "http://h:8080/users/42/x?y=:id");
        assert_eq!(query_from_url("http://h/a?x=1&y#f"), vec![("x".into(), "1".into()), ("y".into(), "".into())]);
        assert_eq!(url_with_query("http://h/a?old=1", &[Pair::new("x", "1"), Pair { enabled: false, ..Pair::new("z", "") }]), "http://h/a?x=1");
        assert_eq!(path_params_in_url("https://h/users/:id/:post?q=:no"), vec!["id", "post"]);
        assert_eq!(parse_dotenv("# c\nA=1\nB=\"x y\"\n")["B"], "x y");
        assert_eq!(safe_name("get /api/x?y"), "get -api-x-y");
    }

    #[test]
    fn file_operations_both_formats() {
        for format in [Format::Bru, Format::Yaml] {
            let dir = std::env::temp_dir().join(format!("zenbruno-ops-{}-{:?}", std::process::id(), format));
            let _ = std::fs::remove_dir_all(&dir);
            let mut c = Collection::create(&dir, "Demo", format).unwrap();
            assert_eq!(Collection::open(&dir).unwrap().name, "Demo");
            let folder = c.new_folder(&dir, "Users").unwrap();
            let req = c.new_request(&folder, "Get user", "http").unwrap();
            c.new_request(&dir, "Socket", "ws").unwrap();
            assert_eq!(c.request_count(), 2);
            assert!(matches!(&c.tree[0], Node::Folder { name, children, .. } if name == "Users" && children.len() == 1), "{format:?} {:#?}", c.tree);
            let renamed = c.rename_request(&req, "Fetch user").unwrap();
            assert_eq!(load_request(&renamed).unwrap().name, "Fetch user");
            let copy = c.clone_request(&renamed, "Fetch copy").unwrap();
            assert_eq!(c.requests_under(&folder), vec![renamed.clone(), copy.clone()]);
            let folder2 = c.rename_folder(&folder, "People").unwrap();
            assert_eq!(c.load_folder(&folder2).name, "People");
            c.new_env("dev").unwrap();
            let mut env = c.envs[0].clone();
            env.vars.push(Pair::new("host", "x"));
            env.secrets.push("token".into());
            save_env(&env).unwrap();
            c.reload();
            assert_eq!((c.envs[0].vars.len(), c.envs[0].secrets.len()), (1, 1));
            c.delete(&folder2).unwrap();
            assert_eq!(c.request_count(), 1);
            assert!(c.delete(&dir).is_err());
            std::fs::remove_dir_all(&dir).unwrap();
        }
    }
}

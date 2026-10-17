use crate::collection::{self, Collection, Node};
use crate::exec::{self, Job, Outcome, Runtime};
use crate::grpc;
use crate::interp::Vars;
use crate::loadtest;
use crate::model::{Environment, FolderSettings, Format, Message, Pair, Request};
use crate::ui::{self, BORDER, BRAND, GREEN, MUTED, RED, SIDEBAR, TEXT, TableCfg};
use crate::ws;
use eframe::egui::{self, Color32, CornerRadius, Key, KeyboardShortcut, Modifiers, RichText, Stroke, TextEdit};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::Arc;
use std::time::Instant;

const MAX_SHOWN_BODY: usize = 2 * 1024 * 1024;

#[derive(PartialEq, Clone, Copy)]
enum ReqSub {
    Params,
    Body,
    Headers,
    Auth,
    Vars,
    Script,
    Assert,
    Tests,
    Docs,
    Settings,
    Messages,
}

#[derive(PartialEq, Clone, Copy)]
enum ResSub {
    Response,
    Headers,
    Timeline,
    Tests,
}

#[derive(PartialEq, Clone, Copy)]
enum SetSub {
    Overview,
    Headers,
    Vars,
    Auth,
    Script,
    Tests,
    Docs,
}

#[derive(Default)]
struct GrpcState {
    pool: Option<prost_reflect::DescriptorPool>,
    methods: Vec<grpc::MethodInfo>,
    loading: Option<Receiver<Result<prost_reflect::DescriptorPool, String>>>,
    load_error: Option<String>,
    events: Vec<grpc::Event>,
    rx: Option<Receiver<grpc::Event>>,
    started: Option<Instant>,
    elapsed_ms: u128,
}

#[derive(Default)]
struct WsState {
    tx: Option<Sender<ws::Command>>,
    rx: Option<Receiver<ws::Event>>,
    log: Vec<(String, ws::Event)>,
    connected: bool,
}

struct ReqTab {
    path: PathBuf,
    req: Request,
    saved: String,
    sub: ReqSub,
    res_sub: ResSub,
    outcome: Option<Outcome>,
    pretty: String,
    pending: Option<Receiver<Outcome>>,
    sent_at: Option<Instant>,
    grpc: GrpcState,
    ws: WsState,
}

struct SettingsTab {
    dir: PathBuf,
    is_collection: bool,
    data: FolderSettings,
    saved: FolderSettings,
    sub: SetSub,
}

struct RunRow {
    name: String,
    method: String,
    outcome: Option<Outcome>,
    iteration: usize,
}

enum RunMsg {
    Row(RunRow),
    Done { runtime: Runtime, env: Option<Environment> },
}

struct RunnerTab {
    dir: PathBuf,
    delay: String,
    iterations: String,
    include_tags: String,
    exclude_tags: String,
    items: Vec<(PathBuf, String, String, Vec<String>, bool)>,
    results: Vec<RunRow>,
    rx: Option<Receiver<RunMsg>>,
    stop: Arc<AtomicBool>,
    picked: Option<usize>,
}

enum TabKind {
    Request(Box<ReqTab>),
    Settings(Box<SettingsTab>),
    Runner(Box<RunnerTab>),
    Load(Box<LoadTab>),
}

struct LoadTab {
    path: PathBuf,
    name: String,
    method: String,
    users: String,
    duration: String,
    max_requests: String,
    ramp_up: String,
    think: String,
    rx: Option<Receiver<loadtest::Sample>>,
    stop: Arc<AtomicBool>,
    stats: loadtest::Stats,
    started: Option<Instant>,
    elapsed: std::time::Duration,
    target: String,
    error: Option<String>,
}

struct Tab {
    root: PathBuf,
    kind: TabKind,
}

impl Tab {
    fn dirty(&self) -> bool {
        match &self.kind {
            TabKind::Request(t) => collection::request_text(&t.path, &t.req) != t.saved,
            TabKind::Settings(t) => t.data != t.saved,
            TabKind::Runner(_) | TabKind::Load(_) => false,
        }
    }

    fn path(&self) -> Option<&Path> {
        match &self.kind {
            TabKind::Request(t) => Some(&t.path),
            TabKind::Settings(t) => Some(&t.dir),
            TabKind::Runner(_) | TabKind::Load(_) => None,
        }
    }
}

enum Dialog {
    NewRequest { root: PathBuf, dir: PathBuf, name: String, kind: String },
    ImportCurl { root: PathBuf, dir: PathBuf, text: String, name: String },
    NewFolder { root: PathBuf, dir: PathBuf, name: String },
    Rename { root: PathBuf, path: PathBuf, name: String, folder: bool },
    Clone { root: PathBuf, path: PathBuf, name: String },
    Delete { root: PathBuf, path: PathBuf, label: String },
    NewCollection { name: String, format: Format },
    NewEnv { root: PathBuf, name: String },
    DeleteEnv { root: PathBuf, index: usize },
    CloseTab { index: usize },
    Quit,
}

enum Act {
    OpenRequest(PathBuf, PathBuf),
    OpenSettings(PathBuf, PathBuf),
    OpenRunner(PathBuf, PathBuf),
    OpenLoad(PathBuf, PathBuf),
    Dialog(Dialog),
    CloseCollection(PathBuf),
    Reload(PathBuf),
    /// toggle expand/collapse
    Activate(PathBuf),
    Expand(PathBuf),
}

pub struct ZenApp {
    collections: Vec<Collection>,
    expanded: HashSet<PathBuf>,
    env_sel: HashMap<PathBuf, usize>,
    runtime: HashMap<PathBuf, Runtime>,
    secrets: HashMap<PathBuf, Vars>,
    tabs: Vec<Tab>,
    active: usize,
    current_root: Option<PathBuf>,
    dialog: Option<Dialog>,
    env_window: Option<(PathBuf, usize)>,
    status: String,
    filter: String,
    acts: Vec<Act>,
    quitting: bool,
    /// Generated code window: (title, code)
    code_window: Option<(String, String)>,
}

fn state_file() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join("ZenBruno").join("state.json")
}

fn now_hms() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    format!("{:02}:{:02}:{:02} UTC", secs / 3600 % 24, secs / 60 % 60, secs % 60)
}

fn pretty_body(body: &[u8]) -> String {
    let text = String::from_utf8_lossy(body).into_owned();
    let mut pretty = serde_json::from_str::<serde_json::Value>(&text).ok().and_then(|v| serde_json::to_string_pretty(&v).ok()).unwrap_or(text);
    if pretty.len() > MAX_SHOWN_BODY {
        let mut cut = MAX_SHOWN_BODY;
        while !pretty.is_char_boundary(cut) {
            cut -= 1;
        }
        pretty.truncate(cut);
        pretty.push_str("\n… (truncated for display)");
    }
    pretty
}

/// URL edited: rebuild params:query (keeping disabled rows) and params:path names.
fn sync_from_url(req: &mut Request) {
    let old_query = std::mem::take(&mut req.query);
    req.query = collection::query_from_url(&req.url)
        .into_iter()
        .map(|(k, v)| {
            let desc = old_query.iter().find(|p| p.key == k).map(|p| p.description.clone()).unwrap_or_default();
            Pair { description: desc, ..Pair::new(&k, &v) }
        })
        .chain(old_query.iter().filter(|p| !p.enabled).cloned())
        .collect();
    let old = std::mem::take(&mut req.path_params);
    req.path_params = collection::path_params_in_url(&req.url).into_iter().map(|name| old.iter().find(|p| p.key == name).cloned().unwrap_or_else(|| Pair::new(&name, ""))).collect();
}

impl ZenApp {
    pub fn new() -> Self {
        let mut app = ZenApp {
            collections: vec![],
            expanded: HashSet::new(),
            env_sel: HashMap::new(),
            runtime: HashMap::new(),
            secrets: HashMap::new(),
            tabs: vec![],
            active: 0,
            current_root: None,
            dialog: None,
            env_window: None,
            status: String::new(),
            filter: String::new(),
            acts: vec![],
            quitting: false,
            code_window: None,
        };
        if let Ok(text) = std::fs::read_to_string(state_file()) {
            let state: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
            for p in state["collections"].as_array().into_iter().flatten().filter_map(|p| p.as_str()) {
                if let Ok(c) = Collection::open(Path::new(p)) {
                    app.collections.push(c);
                }
            }
        }
        app.current_root = app.collections.first().map(|c| c.root.clone());
        // `zenbruno <collection dir | request file>` opens it (also used by "Open with")
        for arg in std::env::args().skip(1) {
            let path = std::fs::canonicalize(&arg).map(|p| PathBuf::from(p.to_string_lossy().trim_start_matches(r"\\?\"))).unwrap_or_else(|_| PathBuf::from(&arg));
            let root = path.ancestors().find(|d| d.join("bruno.json").exists() || d.join("opencollection.yml").exists()).map(Path::to_path_buf);
            let Some(root) = root else {
                app.status = format!("{} is not inside a Bruno collection", path.display());
                continue;
            };
            if app.col(&root).is_none() {
                match Collection::open(&root) {
                    Ok(c) => {
                        app.collections.push(c);
                        app.save_state();
                    }
                    Err(e) => {
                        app.status = e;
                        continue;
                    }
                }
            }
            app.expanded.insert(root.clone());
            app.current_root = Some(root.clone());
            if path.is_file() {
                for dir in path.ancestors().skip(1).take_while(|d| d.starts_with(&root)) {
                    app.expanded.insert(dir.to_path_buf());
                }
                app.open_request(&root, &path);
            }
        }
        app
    }

    fn save_state(&self) {
        let paths: Vec<String> = self.collections.iter().map(|c| c.root.to_string_lossy().to_string()).collect();
        let file = state_file();
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(file, serde_json::json!({ "collections": paths }).to_string());
    }

    fn col(&self, root: &Path) -> Option<&Collection> {
        self.collections.iter().find(|c| c.root == root)
    }

    fn col_mut(&mut self, root: &Path) -> Option<&mut Collection> {
        self.collections.iter_mut().find(|c| c.root == root)
    }

    fn env_of(&self, root: &Path) -> Option<&Environment> {
        self.col(root).and_then(|c| self.env_sel.get(root).and_then(|&i| c.envs.get(i)))
    }

    fn add_collection(&mut self, c: Collection) {
        let root = c.root.clone();
        if self.col(&root).is_none() {
            self.collections.push(c);
            self.save_state();
        }
        self.expanded.insert(root.clone());
        self.current_root = Some(root.clone());
        self.acts.push(Act::OpenSettings(root.clone(), root));
    }

    fn open_collection_dialog(&mut self) {
        if let Some(dir) = rfd::FileDialog::new().set_title("Open Bruno collection folder").pick_folder() {
            match Collection::open(&dir) {
                Ok(c) => self.add_collection(c),
                Err(e) => self.status = e,
            }
        }
    }

    fn focus_or_open(&mut self, root: &Path, path: &Path, make: impl FnOnce(&Self) -> Result<TabKind, String>) {
        if let Some(i) = self.tabs.iter().position(|t| t.path() == Some(path) && matches!((&t.kind, path.is_dir()), (TabKind::Settings(_), true) | (TabKind::Request(_), false))) {
            self.active = i;
            return;
        }
        match make(self) {
            Ok(kind) => {
                self.tabs.push(Tab { root: root.to_path_buf(), kind });
                self.active = self.tabs.len() - 1;
            }
            Err(e) => self.status = e,
        }
    }

    fn open_request(&mut self, root: &Path, path: &Path) {
        self.focus_or_open(root, path, |_| {
            let req = collection::load_request(path)?;
            let saved = collection::request_text(path, &req);
            let sub = match req.kind.as_str() {
                "grpc" | "ws" => ReqSub::Messages,
                _ if req.body_mode != "none" => ReqSub::Body,
                _ => ReqSub::Params,
            };
            Ok(TabKind::Request(Box::new(ReqTab { path: path.to_path_buf(), req, saved, sub, res_sub: ResSub::Response, outcome: None, pretty: String::new(), pending: None, sent_at: None, grpc: GrpcState::default(), ws: WsState::default() })))
        });
        self.current_root = Some(root.to_path_buf());
    }

    fn open_settings(&mut self, root: &Path, dir: &Path) {
        self.focus_or_open(root, dir, |s| {
            let c = s.col(root).ok_or("collection not open")?;
            let data = c.load_folder(dir);
            Ok(TabKind::Settings(Box::new(SettingsTab { dir: dir.to_path_buf(), is_collection: dir == root, saved: data.clone(), data, sub: SetSub::Overview })))
        });
        self.current_root = Some(root.to_path_buf());
    }

    fn open_runner(&mut self, root: &Path, dir: &Path) {
        let Some(c) = self.col(root) else { return };
        let items = c
            .requests_under(dir)
            .into_iter()
            .filter_map(|p| collection::load_request(&p).ok().map(|r| (p, r)))
            .filter(|(_, r)| r.kind == "http" || r.kind == "graphql")
            .map(|(p, r)| (p, r.name, r.method, r.tags, true))
            .collect();
        if let Some(i) = self.tabs.iter().position(|t| matches!(&t.kind, TabKind::Runner(r) if r.dir == dir)) {
            self.active = i;
            return;
        }
        let tab = RunnerTab { dir: dir.to_path_buf(), delay: String::new(), iterations: "1".into(), include_tags: String::new(), exclude_tags: String::new(), items, results: vec![], rx: None, stop: Arc::new(AtomicBool::new(false)), picked: None };
        self.tabs.push(Tab { root: root.to_path_buf(), kind: TabKind::Runner(Box::new(tab)) });
        self.active = self.tabs.len() - 1;
        self.current_root = Some(root.to_path_buf());
    }

    fn open_load(&mut self, root: &Path, path: &Path) {
        if let Some(i) = self.tabs.iter().position(|t| matches!(&t.kind, TabKind::Load(l) if l.path == path)) {
            self.active = i;
            return;
        }
        let req = match collection::load_request(path) {
            Ok(r) if r.kind == "http" || r.kind == "graphql" => r,
            Ok(_) => return self.status = "Load tests support HTTP and GraphQL requests.".into(),
            Err(e) => return self.status = e,
        };
        let tab = LoadTab {
            path: path.to_path_buf(),
            name: req.name,
            method: req.method,
            users: "10".into(),
            duration: "30".into(),
            max_requests: "0".into(),
            ramp_up: "0".into(),
            think: "0".into(),
            rx: None,
            stop: Arc::new(AtomicBool::new(false)),
            stats: loadtest::Stats::default(),
            started: None,
            elapsed: std::time::Duration::ZERO,
            target: String::new(),
            error: None,
        };
        self.tabs.push(Tab { root: root.to_path_buf(), kind: TabKind::Load(Box::new(tab)) });
        self.active = self.tabs.len() - 1;
        self.current_root = Some(root.to_path_buf());
    }

    /// Request as currently edited (open tab, possibly unsaved) or from disk.
    fn current_request(&self, path: &Path) -> Result<Request, String> {
        self.tabs
            .iter()
            .find_map(|t| match &t.kind {
                TabKind::Request(r) if r.path == path => Some(r.req.clone()),
                _ => None,
            })
            .map(Ok)
            .unwrap_or_else(|| collection::load_request(path))
    }

    fn start_load(&mut self, ctx: &egui::Context) {
        let i = self.active;
        let root = self.tabs[i].root.clone();
        let TabKind::Load(l) = &self.tabs[i].kind else { return };
        let path = l.path.clone();
        let num = |s: &str, d: f64| s.trim().parse::<f64>().ok().filter(|v| *v >= 0.0).unwrap_or(d);
        let cfg = loadtest::Config {
            users: num(&l.users, 1.0).clamp(1.0, 2000.0) as usize,
            duration: std::time::Duration::from_secs_f64(num(&l.duration, 30.0).max(1.0)),
            max_requests: num(&l.max_requests, 0.0) as u64,
            ramp_up: std::time::Duration::from_secs_f64(num(&l.ramp_up, 0.0)),
            think: std::time::Duration::from_millis(num(&l.think, 0.0) as u64),
        };
        let prepared = self.current_request(&path).and_then(|req| self.job_for(&root, &path, &req).ok_or("collection not open".into())).and_then(|job| exec::prepare(&job));
        let TabKind::Load(l) = &mut self.tabs[i].kind else { return };
        match prepared {
            Ok(p) => {
                l.target = format!("{} {}", p.method, p.url);
                l.stats = loadtest::Stats::default();
                l.error = None;
                l.stop = Arc::new(AtomicBool::new(false));
                let (tx, rx) = channel();
                l.rx = Some(rx);
                l.started = Some(Instant::now());
                l.elapsed = std::time::Duration::ZERO;
                let c = ctx.clone();
                loadtest::run(p, cfg, l.stop.clone(), tx, move || c.request_repaint_after(std::time::Duration::from_millis(250)));
            }
            Err(e) => l.error = Some(e),
        }
    }

    fn save_tab(&mut self, i: usize) -> bool {
        let Some(tab) = self.tabs.get_mut(i) else { return false };
        let root = tab.root.clone();
        let result = match &mut tab.kind {
            TabKind::Request(t) => collection::save_request(&t.path, &t.req).map(|()| {
                t.saved = collection::request_text(&t.path, &t.req);
                format!("Saved {}", t.req.name)
            }),
            TabKind::Settings(t) => {
                let (dir, data) = (t.dir.clone(), t.data.clone());
                let r = self.collections.iter().find(|c| c.root == root).map(|c| c.save_folder(&dir, &data)).unwrap_or(Err("collection not open".into()));
                if r.is_ok()
                    && let TabKind::Settings(t) = &mut self.tabs[i].kind
                {
                    t.saved = t.data.clone();
                }
                r.map(|()| "Saved settings".to_string())
            }
            TabKind::Runner(_) | TabKind::Load(_) => return true,
        };
        match result {
            Ok(msg) => {
                self.status = msg;
                if let Some(c) = self.col_mut(&root) {
                    c.reload();
                }
                true
            }
            Err(e) => {
                self.status = format!("Save failed: {e}");
                false
            }
        }
    }

    fn close_tab(&mut self, i: usize) {
        if i < self.tabs.len() {
            if let TabKind::Request(t) = &self.tabs[i].kind
                && let Some(tx) = &t.ws.tx
            {
                let _ = tx.send(ws::Command::Close);
            }
            self.tabs.remove(i);
            if self.active >= self.tabs.len() {
                self.active = self.tabs.len().saturating_sub(1);
            }
        }
    }

    fn request_close_tab(&mut self, i: usize) {
        if self.tabs.get(i).is_some_and(Tab::dirty) {
            self.dialog = Some(Dialog::CloseTab { index: i });
        } else {
            self.close_tab(i);
        }
    }

    fn job_for(&self, root: &Path, path: &Path, req: &Request) -> Option<Job> {
        let c = self.col(root)?.clone();
        let env = self.env_of(root).cloned();
        let secrets = env.as_ref().and_then(|e| self.secrets.get(&e.path).cloned()).unwrap_or_default();
        Some(Job { collection: c, env, secrets, runtime: self.runtime.get(root).cloned().unwrap_or_default(), path: path.to_path_buf(), request: req.clone() })
    }

    /// Script `bru.setEnvVar` changes update the in-memory environment (like Bruno).
    fn apply_env_changes(&mut self, root: &Path, changes: &serde_json::Map<String, serde_json::Value>) {
        let Some(i) = self.env_sel.get(root).copied() else { return };
        let Some(env) = self.col_mut(root).and_then(|c| c.envs.get_mut(i)) else { return };
        for (k, v) in changes {
            if v.is_null() {
                env.vars.retain(|p| &p.key != k);
                continue;
            }
            let value = v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string());
            match env.vars.iter_mut().find(|p| &p.key == k) {
                Some(p) => p.value = value,
                None => env.vars.push(Pair::new(k, &value)),
            }
        }
    }

    fn send_active(&mut self, ctx: &egui::Context) {
        let i = self.active;
        let Some(tab) = self.tabs.get(i) else { return };
        let TabKind::Request(t) = &tab.kind else { return };
        match t.req.kind.as_str() {
            "grpc" => return self.grpc_invoke(ctx),
            "ws" => return self.ws_toggle(ctx),
            _ => {}
        }
        if t.pending.is_some() {
            return;
        }
        let Some(job) = self.job_for(&tab.root, &t.path, &t.req) else { return };
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(exec::run(job));
            ctx.request_repaint();
        });
        if let TabKind::Request(t) = &mut self.tabs[i].kind {
            t.pending = Some(rx);
            t.sent_at = Some(Instant::now());
        }
    }

    fn poll(&mut self, ctx: &egui::Context) {
        let mut finished: Vec<(PathBuf, Outcome)> = vec![];
        let mut run_done: Vec<(PathBuf, Runtime, Option<Environment>)> = vec![];
        for tab in &mut self.tabs {
            match &mut tab.kind {
                TabKind::Request(t) => {
                    if let Some(rx) = &t.pending
                        && let Ok(o) = rx.try_recv()
                    {
                        t.pending = None;
                        t.pretty = o.response.as_ref().map(|r| pretty_body(&r.body)).unwrap_or_default();
                        if !o.tests.is_empty() && o.tests.iter().any(|x| !x.passed) || o.asserts.iter().any(|a| a.1.is_err()) {
                            t.res_sub = ResSub::Tests;
                        } else if t.res_sub == ResSub::Tests && o.tests.is_empty() && o.asserts.is_empty() {
                            t.res_sub = ResSub::Response;
                        }
                        finished.push((tab.root.clone(), o.clone()));
                        t.outcome = Some(o);
                    }
                    if t.pending.is_some() {
                        ctx.request_repaint_after(std::time::Duration::from_millis(100));
                    }
                    if let Some(rx) = &t.grpc.rx {
                        while let Ok(e) = rx.try_recv() {
                            let end = matches!(e, grpc::Event::Status(..) | grpc::Event::Error(_));
                            t.grpc.events.push(e);
                            if end {
                                t.grpc.elapsed_ms = t.grpc.started.map(|s| s.elapsed().as_millis()).unwrap_or(0);
                                t.grpc.rx = None;
                                break;
                            }
                        }
                    }
                    if let Some(rx) = &t.grpc.loading
                        && let Ok(r) = rx.try_recv()
                    {
                        t.grpc.loading = None;
                        match r {
                            Ok(pool) => {
                                t.grpc.methods = grpc::methods(&pool);
                                t.grpc.pool = Some(pool);
                                t.grpc.load_error = None;
                                if t.req.method.is_empty()
                                    && let Some(m) = t.grpc.methods.first()
                                {
                                    t.req.method = m.path.clone();
                                    t.req.grpc_method_type = m.kind.to_string();
                                }
                            }
                            Err(e) => t.grpc.load_error = Some(e),
                        }
                    }
                    if let Some(rx) = &t.ws.rx {
                        while let Ok(e) = rx.try_recv() {
                            match &e {
                                ws::Event::Opened { .. } => t.ws.connected = true,
                                ws::Event::Closed(_) | ws::Event::Error(_) => {
                                    t.ws.connected = false;
                                    t.ws.tx = None;
                                }
                                _ => {}
                            }
                            t.ws.log.push((now_hms(), e));
                        }
                    }
                }
                TabKind::Runner(r) => {
                    if let Some(rx) = &r.rx {
                        while let Ok(m) = rx.try_recv() {
                            match m {
                                RunMsg::Row(row) => r.results.push(row),
                                RunMsg::Done { runtime, env } => {
                                    r.rx = None;
                                    run_done.push((tab.root.clone(), runtime, env));
                                    break;
                                }
                            }
                        }
                        ctx.request_repaint_after(std::time::Duration::from_millis(200));
                    }
                }
                TabKind::Load(l) => {
                    if let Some(rx) = &l.rx {
                        // bounded per frame so a fast server can't stall the UI
                        let mut done = false;
                        for _ in 0..200_000 {
                            match rx.try_recv() {
                                Ok(s) => l.stats.add(&s),
                                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                                    done = true;
                                    break;
                                }
                            }
                        }
                        if let Some(s) = l.started {
                            l.elapsed = s.elapsed();
                        }
                        if done {
                            l.rx = None;
                            l.elapsed = l.stats.last_at.max(std::time::Duration::from_millis(1));
                        } else {
                            ctx.request_repaint_after(std::time::Duration::from_millis(250));
                        }
                    }
                }
                TabKind::Settings(_) => {}
            }
        }
        for (root, o) in finished {
            self.runtime.insert(root.clone(), o.runtime.clone());
            self.apply_env_changes(&root, &o.env_changes);
        }
        for (root, rt, env) in run_done {
            self.runtime.insert(root.clone(), rt);
            if let (Some(env), Some(i)) = (env, self.env_sel.get(&root).copied())
                && let Some(slot) = self.col_mut(&root).and_then(|c| c.envs.get_mut(i))
            {
                slot.vars = env.vars;
            }
        }
    }

    // ---------- sidebar ----------

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("📦 Collections").strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.menu_button(RichText::new("⋯").strong(), |ui| {
                    if ui.button("Create collection").clicked() {
                        self.dialog = Some(Dialog::NewCollection { name: String::new(), format: Format::Yaml });
                        ui.close();
                    }
                    if ui.button("Open collection").clicked() {
                        ui.close();
                        self.open_collection_dialog();
                    }
                });
                if ui.add(egui::Button::new(RichText::new("+").size(16.0)).frame(false)).on_hover_text("Create collection").clicked() {
                    self.dialog = Some(Dialog::NewCollection { name: String::new(), format: Format::Yaml });
                }
                if ui.add(egui::Button::new("📂").frame(false)).on_hover_text("Open collection").clicked() {
                    self.open_collection_dialog();
                }
            });
        });
        ui.add(TextEdit::singleline(&mut self.filter).hint_text("🔍 Search requests").desired_width(f32::INFINITY));
        ui.add_space(4.0);
        let current = self.tabs.get(self.active).and_then(|t| t.path().map(Path::to_path_buf));
        let filter = self.filter.to_lowercase();
        let mut acts = std::mem::take(&mut self.acts);
        egui::ScrollArea::vertical().id_salt("sidebar").auto_shrink([false, false]).show(ui, |ui| {
            if self.collections.is_empty() {
                ui.add_space(20.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("No collections open").color(MUTED));
                    if ui::link_button(ui, "Open collection").clicked() {
                        acts.push(Act::Activate(PathBuf::new()));
                    }
                    if ui::link_button(ui, "Create collection").clicked() {
                        self.dialog = Some(Dialog::NewCollection { name: String::new(), format: Format::Yaml });
                    }
                });
            }
            for c in &self.collections {
                let open = self.expanded.contains(&c.root);
                let r = tree_row(ui, 0, Some(open), RichText::new(&c.name).strong(), None, self.current_root.as_ref() == Some(&c.root) && current.as_deref() == Some(c.root.as_path()));
                if chevron_clicked(&r, 0) {
                    acts.push(Act::Activate(c.root.clone()));
                } else if r.clicked() {
                    acts.push(Act::Expand(c.root.clone()));
                    acts.push(Act::OpenSettings(c.root.clone(), c.root.clone()));
                }
                r.context_menu(|ui| {
                    folder_menu(ui, &c.root, &c.root, true, &mut acts);
                    ui.separator();
                    if ui.button("Reload from disk").clicked() {
                        acts.push(Act::Reload(c.root.clone()));
                        ui.close();
                    }
                    if ui.button("Close collection").clicked() {
                        acts.push(Act::CloseCollection(c.root.clone()));
                        ui.close();
                    }
                });
                if open {
                    tree(ui, &c.root, &c.tree, 1, &self.expanded, current.as_deref(), &filter, &mut acts);
                }
            }
        });
        self.acts = acts;
    }

    // ---------- main area ----------

    fn collection_header(&mut self, ui: &mut egui::Ui) {
        let Some(root) = self.current_root.clone() else { return };
        let Some(c) = self.col(&root) else { return };
        let name = c.name.clone();
        let envs: Vec<String> = c.envs.iter().map(|e| e.name.clone()).collect();
        ui.horizontal(|ui| {
            ui.label(RichText::new("📦").size(15.0));
            if ui.add(egui::Label::new(RichText::new(&name).strong().size(14.0)).sense(egui::Sense::click())).on_hover_text("Collection settings").clicked() {
                self.acts.push(Act::OpenSettings(root.clone(), root.clone()));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let sel = self.env_sel.get(&root).copied();
                let label = sel.and_then(|i| envs.get(i)).cloned().unwrap_or("No Environment".into());
                egui::ComboBox::from_id_salt(("env", &root)).selected_text(label).width(170.0).show_ui(ui, |ui| {
                    if ui.selectable_label(sel.is_none(), "No Environment").clicked() {
                        self.env_sel.remove(&root);
                    }
                    for (i, e) in envs.iter().enumerate() {
                        if ui.selectable_label(sel == Some(i), e).clicked() {
                            self.env_sel.insert(root.clone(), i);
                        }
                    }
                    ui.separator();
                    if ui.button("⚙ Configure").clicked() {
                        self.env_window = Some((root.clone(), sel.unwrap_or(0)));
                    }
                });
                if ui.add(egui::Button::new("🏃").frame(false)).on_hover_text("Runner").clicked() {
                    self.acts.push(Act::OpenRunner(root.clone(), root.clone()));
                }
                if ui.add(egui::Button::new("⚙").frame(false)).on_hover_text("Collection settings").clicked() {
                    self.acts.push(Act::OpenSettings(root.clone(), root.clone()));
                }
            });
        });
    }

    fn tab_bar(&mut self, ui: &mut egui::Ui) {
        let mut close = None;
        let mut new_req = false;
        egui::ScrollArea::horizontal().id_salt("tabs").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for (i, tab) in self.tabs.iter().enumerate() {
                    let active = i == self.active;
                    let dirty = tab.dirty();
                    let fill = if active { Color32::WHITE } else { Color32::from_rgb(246, 246, 246) };
                    let frame = egui::Frame::new().fill(fill).stroke(Stroke::new(1.0, if active { BORDER } else { Color32::TRANSPARENT })).corner_radius(CornerRadius { nw: 6, ne: 6, sw: 0, se: 0 }).inner_margin(egui::Margin::symmetric(10, 5));
                    let r = frame
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 5.0;
                                match &tab.kind {
                                    TabKind::Request(t) => {
                                        ui.label(ui::method_tag(&t.req.method, &t.req.kind));
                                        ui.label(truncate(&t.req.name, 26));
                                    }
                                    TabKind::Settings(t) => {
                                        ui.label(RichText::new("⚙").color(BRAND));
                                        ui.label(if t.is_collection { "Collection".to_string() } else { truncate(&t.data.name, 24) });
                                    }
                                    TabKind::Runner(_) => {
                                        ui.label(RichText::new("🏃").color(BRAND));
                                        ui.label("Runner");
                                    }
                                    TabKind::Load(l) => {
                                        ui.label(RichText::new("⚡").color(BRAND));
                                        ui.label(format!("Load · {}", truncate(&l.name, 20)));
                                    }
                                }
                                let x = if dirty { RichText::new("●").color(BRAND).size(10.0) } else { RichText::new("×").color(MUTED) };
                                if ui.add(egui::Button::new(x).frame(false).small()).on_hover_text("Close (Ctrl+W)").clicked() {
                                    close = Some(i);
                                }
                            });
                        })
                        .response
                        .interact(egui::Sense::click());
                    if r.clicked() {
                        self.active = i;
                    }
                    if r.middle_clicked() {
                        close = Some(i);
                    }
                }
                if self.current_root.is_some() && ui.add(egui::Button::new(RichText::new("+").size(16.0).color(MUTED)).frame(false)).on_hover_text("New request").clicked() {
                    new_req = true;
                }
            });
        });
        if let Some(i) = close {
            self.request_close_tab(i);
        }
        if new_req && let Some(root) = self.current_root.clone() {
            self.dialog = Some(Dialog::NewRequest { dir: root.clone(), root, name: String::new(), kind: "http".into() });
        }
        if let Some(t) = self.tabs.get(self.active) {
            self.current_root = Some(t.root.clone());
        }
    }

    fn empty_state(ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.3);
            // Bruno's paper-plane illustration, drawn as an outline
            let (rect, _) = ui.allocate_exact_size(egui::vec2(96.0, 80.0), egui::Sense::hover());
            let p = |x: f32, y: f32| rect.min + egui::vec2(x, y);
            let stroke = Stroke::new(2.0, Color32::from_rgb(205, 205, 205));
            let outline = vec![p(4.0, 34.0), p(92.0, 4.0), p(60.0, 76.0), p(4.0, 34.0)];
            ui.painter().add(egui::Shape::line(outline, stroke));
            ui.painter().line_segment([p(92.0, 4.0), p(40.0, 46.0)], stroke);
            ui.painter().line_segment([p(40.0, 46.0), p(60.0, 76.0)], stroke);
            ui.add_space(18.0);
            for (a, b) in [("Send Request", "Ctrl + Enter"), ("Save", "Ctrl + S"), ("Close Tab", "Ctrl + W"), ("Edit Environments", "Ctrl + E")] {
                let mut job = egui::text::LayoutJob::default();
                job.append(a, 0.0, egui::TextFormat { color: MUTED, font_id: egui::FontId::proportional(13.0), ..Default::default() });
                job.append(b, 16.0, egui::TextFormat { color: TEXT, font_id: egui::FontId::proportional(13.0), ..Default::default() });
                ui.label(job);
            }
        });
    }

    fn main_area(&mut self, ui: &mut egui::Ui) {
        if self.tabs.is_empty() {
            self.collection_header(ui);
            ui.separator();
            Self::empty_state(ui);
            return;
        }
        self.collection_header(ui);
        self.tab_bar(ui);
        ui.add(egui::Separator::default().spacing(0.0));
        ui.add_space(8.0);
        let i = self.active.min(self.tabs.len() - 1);
        self.active = i;
        let root = self.tabs[i].root.clone();
        let mut send = false;
        let mut save = false;
        let col = self.col(&root).cloned();
        match &mut self.tabs[i].kind {
            TabKind::Request(t) => request_view(ui, t, col.as_ref(), &mut send, &mut save),
            TabKind::Settings(t) => {
                if let Some(c) = &col {
                    settings_view(ui, t, c, &mut save, &mut self.acts)
                }
            }
            TabKind::Runner(r) => {
                if runner_view(ui, r, col.as_ref()) {
                    self.start_runner(ui.ctx());
                }
            }
            TabKind::Load(l) => {
                if load_view(ui, l) {
                    self.start_load(ui.ctx());
                }
            }
        }
        if send {
            self.send_active(ui.ctx());
        }
        if save {
            self.save_tab(i);
        }
    }

    fn start_runner(&mut self, ctx: &egui::Context) {
        let i = self.active;
        let root = self.tabs[i].root.clone();
        let Some(c) = self.col(&root).cloned() else { return };
        let env = self.env_of(&root).cloned();
        let secrets = env.as_ref().and_then(|e| self.secrets.get(&e.path).cloned()).unwrap_or_default();
        let runtime = self.runtime.get(&root).cloned().unwrap_or_default();
        let TabKind::Runner(r) = &mut self.tabs[i].kind else { return };
        let tags = |s: &str| s.split(',').map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect::<Vec<_>>();
        let (inc, exc) = (tags(&r.include_tags), tags(&r.exclude_tags));
        let list: Vec<(PathBuf, String, String)> = r
            .items
            .iter()
            .filter(|(_, _, _, t, on)| *on && (inc.is_empty() || t.iter().any(|x| inc.contains(x))) && !t.iter().any(|x| exc.contains(x)))
            .map(|(p, n, m, _, _)| (p.clone(), n.clone(), m.clone()))
            .collect();
        let delay = r.delay.trim().parse::<u64>().unwrap_or(0);
        let iterations = r.iterations.trim().parse::<usize>().unwrap_or(1).max(1);
        let (tx, rx) = channel();
        let stop = Arc::new(AtomicBool::new(false));
        r.stop = stop.clone();
        r.rx = Some(rx);
        r.results.clear();
        r.picked = None;
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let mut runtime = runtime;
            let mut env = env;
            'outer: for iteration in 1..=iterations {
                let mut idx = 0;
                let mut guard = 0;
                while idx < list.len() && guard < 10_000 {
                    guard += 1;
                    if stop.load(Ordering::Relaxed) {
                        break 'outer;
                    }
                    let (path, name, method) = &list[idx];
                    let outcome = match collection::load_request(path) {
                        Ok(request) => exec::run(Job { collection: c.clone(), env: env.clone(), secrets: secrets.clone(), runtime: runtime.clone(), path: path.clone(), request }),
                        Err(e) => Outcome { error: Some(e), runtime: runtime.clone(), ..Default::default() },
                    };
                    runtime = outcome.runtime.clone();
                    if let Some(env) = &mut env {
                        for (k, v) in &outcome.env_changes {
                            env.vars.retain(|p| &p.key != k);
                            if !v.is_null() {
                                env.vars.push(Pair::new(k, &v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string())));
                            }
                        }
                    }
                    let next = outcome.next_request.clone();
                    let stop_now = outcome.stop;
                    let _ = tx.send(RunMsg::Row(RunRow { name: name.clone(), method: method.clone(), outcome: Some(outcome), iteration }));
                    ctx.request_repaint();
                    if stop_now {
                        break 'outer;
                    }
                    idx = match next {
                        // bru.setNextRequest(null) ends the run
                        Some(n) if n.is_empty() || n == "null" => break,
                        Some(n) => match list.iter().position(|(_, nm, _)| *nm == n) {
                            Some(j) => j,
                            None => idx + 1,
                        },
                        None => idx + 1,
                    };
                    if delay > 0 {
                        std::thread::sleep(std::time::Duration::from_millis(delay));
                    }
                }
            }
            let _ = tx.send(RunMsg::Done { runtime, env });
            ctx.request_repaint();
        });
    }

    fn grpc_load(&mut self, ctx: &egui::Context, reflection: bool) {
        let i = self.active;
        let root = self.tabs[i].root.clone();
        let TabKind::Request(t) = &self.tabs[i].kind else { return };
        let (tx, rx) = channel();
        let proto = t.req.proto_path.clone();
        let proto_path = if Path::new(&proto).is_absolute() { PathBuf::from(&proto) } else { root.join(&proto) };
        let url = t.req.url.clone();
        let vars = self.job_for(&root, &t.path, &t.req).map(|j| exec_vars(&j)).unwrap_or_default();
        let url = crate::interp::interpolate(&url, &vars, false);
        let ctx = ctx.clone();
        let root2 = root.clone();
        std::thread::spawn(move || {
            let r = if reflection || proto.is_empty() { grpc::from_reflection(&url) } else { grpc::from_proto(&proto_path, &[root2]) };
            let _ = tx.send(r);
            ctx.request_repaint();
        });
        if let TabKind::Request(t) = &mut self.tabs[i].kind {
            t.grpc.loading = Some(rx);
            t.grpc.load_error = None;
        }
    }

    fn grpc_invoke(&mut self, ctx: &egui::Context) {
        let i = self.active;
        let root = self.tabs[i].root.clone();
        let TabKind::Request(t) = &self.tabs[i].kind else { return };
        if t.grpc.rx.is_some() {
            return;
        }
        let Some(pool) = t.grpc.pool.clone() else {
            self.grpc_load(ctx, t.req.proto_path.is_empty());
            self.status = "Loading gRPC methods… press Invoke again once they are loaded.".into();
            return;
        };
        let Some(job) = self.job_for(&root, &t.path, &t.req) else { return };
        let vars = exec_vars(&job);
        let i_ = |s: &str| crate::interp::interpolate(s, &vars, false);
        let url = i_(&t.req.url);
        let mut metadata: Vec<(String, String)> = vec![];
        let chain = job.collection.settings_chain(&t.path);
        if let Ok(p) = exec::build(&job.collection, &chain, &t.req) {
            metadata = p.headers.iter().map(|(k, v)| (i_(k), i_(v))).collect();
            if let Some((u, pw)) = p.basic_auth {
                metadata.push(("authorization".into(), format!("Basic {}", base64(format!("{}:{}", i_(&u), i_(&pw)).as_bytes()))));
            }
        }
        let messages: Vec<String> = t.req.messages.iter().map(|m| crate::interp::interpolate(&m.content, &vars, true)).collect();
        let method = t.req.method.clone();
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            grpc::call(&url, &pool, &method, &messages, &metadata, &tx);
            ctx.request_repaint();
        });
        if let TabKind::Request(t) = &mut self.tabs[i].kind {
            t.grpc.events.clear();
            t.grpc.rx = Some(rx);
            t.grpc.started = Some(Instant::now());
            t.res_sub = ResSub::Response;
        }
    }

    fn ws_toggle(&mut self, ctx: &egui::Context) {
        let i = self.active;
        let root = self.tabs[i].root.clone();
        let TabKind::Request(t) = &mut self.tabs[i].kind else { return };
        if let Some(tx) = t.ws.tx.take() {
            let _ = tx.send(ws::Command::Close);
            return;
        }
        let TabKind::Request(t) = &self.tabs[i].kind else { return };
        let Some(job) = self.job_for(&root, &t.path, &t.req) else { return };
        let vars = exec_vars(&job);
        let i_ = |s: &str| crate::interp::interpolate(s, &vars, false);
        let chain = job.collection.settings_chain(&t.path);
        let mut headers: Vec<(String, String)> = vec![];
        if let Ok(p) = exec::build(&job.collection, &chain, &t.req) {
            headers = p.headers.iter().filter(|(k, _)| !k.eq_ignore_ascii_case("content-type")).map(|(k, v)| (i_(k), i_(v))).collect();
            if let Some((u, pw)) = p.basic_auth {
                headers.push(("Authorization".into(), format!("Basic {}", base64(format!("{}:{}", i_(&u), i_(&pw)).as_bytes()))));
            }
        }
        let keep_alive = t.req.setting("keepAliveInterval").and_then(|v| v.parse().ok()).unwrap_or(0);
        let url = i_(&t.req.url);
        let (etx, erx) = channel();
        let c2 = ctx.clone();
        let cmd = ws::connect(&url, headers, keep_alive, etx, move || c2.request_repaint());
        if let TabKind::Request(t) = &mut self.tabs[i].kind {
            t.ws.tx = Some(cmd);
            t.ws.rx = Some(erx);
            t.ws.log.clear();
        }
    }

    fn ws_send(&mut self, idx: usize) {
        let i = self.active;
        let root = self.tabs[i].root.clone();
        let TabKind::Request(t) = &self.tabs[i].kind else { return };
        let Some(job) = self.job_for(&root, &t.path, &t.req) else { return };
        let vars = exec_vars(&job);
        if let (Some(tx), Some(m)) = (&t.ws.tx, t.req.messages.get(idx)) {
            let _ = tx.send(ws::Command::Send(crate::interp::interpolate(&m.content, &vars, m.kind == "json")));
        }
    }

    fn apply_acts(&mut self) {
        for act in std::mem::take(&mut self.acts) {
            match act {
                Act::OpenRequest(root, path) => self.open_request(&root, &path),
                Act::OpenLoad(root, path) => self.open_load(&root, &path),
                Act::OpenSettings(root, dir) => self.open_settings(&root, &dir),
                Act::OpenRunner(root, dir) => self.open_runner(&root, &dir),
                Act::Dialog(d) => self.dialog = Some(d),
                Act::Reload(root) => {
                    if let Some(c) = self.col_mut(&root) {
                        c.reload();
                    }
                }
                Act::CloseCollection(root) => {
                    if self.tabs.iter().any(|t| t.root == root && t.dirty()) {
                        self.status = "Save or close the collection's unsaved tabs first.".into();
                        continue;
                    }
                    self.tabs.retain(|t| t.root != root);
                    self.active = self.active.min(self.tabs.len().saturating_sub(1));
                    self.collections.retain(|c| c.root != root);
                    if self.current_root.as_ref() == Some(&root) {
                        self.current_root = self.collections.first().map(|c| c.root.clone());
                    }
                    self.save_state();
                }
                Act::Expand(path) => {
                    self.expanded.insert(path);
                }
                Act::Activate(root) => {
                    if root.as_os_str().is_empty() {
                        self.open_collection_dialog();
                    } else if !self.expanded.remove(&root) {
                        self.expanded.insert(root.clone());
                        self.current_root = Some(root.clone());
                    }
                }
            }
        }
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.dialog.take() else { return };
        let mut keep = true;
        let mut result: Option<Result<(), String>> = None;
        egui::Modal::new(egui::Id::unique("zen-dialog")).show(ctx, |ui| {
            ui.set_min_width(380.0);
            let enter = ui.input(|i| i.key_pressed(Key::Enter));
            let esc = ui.input(|i| i.key_pressed(Key::Escape));
            match &mut d {
                Dialog::NewRequest { root, dir, name, kind } => {
                    ui.heading("New Request");
                    ui.horizontal(|ui| {
                        for (k, label) in [("http", "HTTP"), ("graphql", "GraphQL"), ("grpc", "gRPC"), ("ws", "WebSocket")] {
                            ui.radio_value(kind, k.to_string(), label);
                        }
                    });
                    ui.label("Request name");
                    ui.add(TextEdit::singleline(name).desired_width(f32::INFINITY)).request_focus();
                    let ok = !name.trim().is_empty();
                    if dialog_buttons(ui, "Create", ok) || ok && enter {
                        keep = false;
                        let (root, dir, name, kind) = (root.clone(), dir.clone(), name.clone(), kind.clone());
                        result = Some(self.col_mut(&root).ok_or("collection not open".to_string()).and_then(|c| c.new_request(&dir, &name, &kind)).map(|p| self.acts.push(Act::OpenRequest(root, p))));
                    }
                }
                Dialog::ImportCurl { root, dir, text, name } => {
                    ui.heading("Import cURL");
                    ui.label("Paste a cURL command (bash or Windows cmd \"Copy as cURL\")");
                    egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                        ui.add(TextEdit::multiline(text).code_editor().desired_rows(8).desired_width(560.0));
                    });
                    let parsed = crate::curl::parse_curl(text);
                    match &parsed {
                        Ok(r) => {
                            ui.label(RichText::new(format!("{} {}", r.method.to_uppercase(), r.url)).color(GREEN).monospace());
                        }
                        Err(e) if !text.trim().is_empty() => {
                            ui.colored_label(RED, e);
                        }
                        Err(_) => {}
                    }
                    ui.label("Request name");
                    let hint = parsed.as_ref().map(|r| r.name.clone()).unwrap_or_default();
                    ui.add(TextEdit::singleline(name).hint_text(hint).desired_width(f32::INFINITY));
                    if dialog_buttons(ui, "Import", parsed.is_ok()) {
                        keep = false;
                        let (root, dir) = (root.clone(), dir.clone());
                        let mut req = parsed.unwrap();
                        if !name.trim().is_empty() {
                            req.name = name.trim().to_string();
                        }
                        result = Some(self.col_mut(&root).ok_or("collection not open".to_string()).and_then(|c| c.add_request(&dir, req)).map(|p| self.acts.push(Act::OpenRequest(root, p))));
                    }
                }
                Dialog::NewFolder { root, dir, name } => {
                    ui.heading("New Folder");
                    ui.label("Folder name");
                    ui.add(TextEdit::singleline(name).desired_width(f32::INFINITY)).request_focus();
                    let ok = !name.trim().is_empty();
                    if dialog_buttons(ui, "Create", ok) || ok && enter {
                        keep = false;
                        let (root, dir, name) = (root.clone(), dir.clone(), name.clone());
                        result = Some(self.col_mut(&root).ok_or("collection not open".to_string()).and_then(|c| c.new_folder(&dir, &name)).map(|p| {
                            self.expanded.insert(p);
                        }));
                    }
                }
                Dialog::Rename { root, path, name, folder } => {
                    ui.heading(if *folder { "Rename Folder" } else { "Rename Request" });
                    ui.add(TextEdit::singleline(name).desired_width(f32::INFINITY)).request_focus();
                    let ok = !name.trim().is_empty();
                    if dialog_buttons(ui, "Rename", ok) || ok && enter {
                        keep = false;
                        let (root, path, name, folder) = (root.clone(), path.clone(), name.clone(), *folder);
                        let r = self.col_mut(&root).ok_or("collection not open".to_string()).and_then(|c| if folder { c.rename_folder(&path, &name) } else { c.rename_request(&path, &name) });
                        result = Some(r.map(|new_path| self.retarget_tabs(&path, &new_path)));
                    }
                }
                Dialog::Clone { root, path, name } => {
                    ui.heading("Clone Request");
                    ui.add(TextEdit::singleline(name).desired_width(f32::INFINITY)).request_focus();
                    let ok = !name.trim().is_empty();
                    if dialog_buttons(ui, "Clone", ok) || ok && enter {
                        keep = false;
                        let (root, path, name) = (root.clone(), path.clone(), name.clone());
                        result = Some(self.col_mut(&root).ok_or("collection not open".to_string()).and_then(|c| c.clone_request(&path, &name)).map(|p| self.acts.push(Act::OpenRequest(root, p))));
                    }
                }
                Dialog::Delete { root, path, label } => {
                    ui.heading("Delete");
                    ui.label(format!("Are you sure you want to delete {label}? This removes it from disk."));
                    if dialog_buttons(ui, "Delete", true) {
                        keep = false;
                        let (root, path) = (root.clone(), path.clone());
                        result = Some(self.col_mut(&root).ok_or("collection not open".to_string()).and_then(|c| c.delete(&path)).map(|()| {
                            self.tabs.retain(|t| !t.path().is_some_and(|p| p.starts_with(&path)));
                            self.active = self.active.min(self.tabs.len().saturating_sub(1));
                        }));
                    }
                }
                Dialog::NewCollection { name, format } => {
                    ui.heading("Create Collection");
                    ui.label("Name");
                    ui.add(TextEdit::singleline(name).desired_width(f32::INFINITY)).request_focus();
                    ui.horizontal(|ui| {
                        ui.label("File format");
                        ui.radio_value(format, Format::Yaml, "YAML (OpenCollection)");
                        ui.radio_value(format, Format::Bru, "Bru");
                    });
                    ui.label(RichText::new("You'll choose the parent folder next.").color(MUTED));
                    let ok = !name.trim().is_empty();
                    if dialog_buttons(ui, "Choose location…", ok) {
                        keep = false;
                        if let Some(parent) = rfd::FileDialog::new().pick_folder() {
                            let r = Collection::create(&parent.join(collection::safe_name(name)), name.trim(), *format);
                            result = Some(r.map(|c| self.add_collection(c)));
                        }
                    }
                }
                Dialog::NewEnv { root, name } => {
                    ui.heading("Create Environment");
                    ui.add(TextEdit::singleline(name).desired_width(f32::INFINITY)).request_focus();
                    let ok = !name.trim().is_empty();
                    if dialog_buttons(ui, "Create", ok) || ok && enter {
                        keep = false;
                        let (root, name) = (root.clone(), name.clone());
                        result = Some(self.col_mut(&root).ok_or("collection not open".to_string()).and_then(|c| c.new_env(&name)).map(|()| {
                            if let Some(i) = self.col(&root).and_then(|c| c.envs.iter().position(|e| e.name == name.trim())) {
                                self.env_window = Some((root.clone(), i));
                            }
                        }));
                    }
                }
                Dialog::DeleteEnv { root, index } => {
                    ui.heading("Delete Environment");
                    let name = self.col(root).and_then(|c| c.envs.get(*index)).map(|e| e.name.clone()).unwrap_or_default();
                    ui.label(format!("Delete environment \"{name}\"? This removes its file."));
                    if dialog_buttons(ui, "Delete", true) {
                        keep = false;
                        let (root, index) = (root.clone(), *index);
                        let path = self.col(&root).and_then(|c| c.envs.get(index)).map(|e| e.path.clone());
                        result = Some(match path {
                            Some(p) => std::fs::remove_file(&p).map_err(|e| e.to_string()).map(|()| {
                                if let Some(c) = self.col_mut(&root) {
                                    c.reload();
                                }
                                self.env_sel.remove(&root);
                                self.env_window = Some((root.clone(), 0));
                            }),
                            None => Ok(()),
                        });
                    }
                }
                Dialog::CloseTab { index } => {
                    ui.heading("Unsaved changes");
                    ui.label("You have unsaved changes in this tab.");
                    ui.horizontal(|ui| {
                        if ui.button("Don't Save").clicked() {
                            keep = false;
                            self.close_tab(*index);
                        }
                        if ui.button("Cancel").clicked() {
                            keep = false;
                        }
                        if ui::primary_button(ui, "Save").clicked() {
                            keep = false;
                            if self.save_tab(*index) {
                                self.close_tab(*index);
                            }
                        }
                    });
                }
                Dialog::Quit => {
                    ui.heading("Unsaved changes");
                    ui.label("Some tabs have unsaved changes.");
                    ui.horizontal(|ui| {
                        if ui.button("Quit without saving").clicked() {
                            keep = false;
                            self.quitting = true;
                            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        if ui.button("Cancel").clicked() {
                            keep = false;
                        }
                        if ui::primary_button(ui, "Save all and quit").clicked() {
                            keep = false;
                            let ok = (0..self.tabs.len()).filter(|&i| self.tabs[i].dirty()).collect::<Vec<_>>().into_iter().all(|i| self.save_tab(i));
                            if ok {
                                self.quitting = true;
                                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        }
                    });
                }
            }
            if esc {
                keep = false;
            }
        });
        if let Some(Err(e)) = result {
            self.status = e;
        }
        if keep {
            self.dialog = Some(d);
        }
    }

    fn retarget_tabs(&mut self, old: &Path, new: &Path) {
        for t in &mut self.tabs {
            match &mut t.kind {
                TabKind::Request(r) if r.path.starts_with(old) => {
                    let rel = r.path.strip_prefix(old).unwrap_or(Path::new("")).to_path_buf();
                    r.path = if rel.as_os_str().is_empty() { new.to_path_buf() } else { new.join(rel) };
                    if let Ok(fresh) = collection::load_request(&r.path) {
                        // keep unsaved edits, adopt the new name
                        r.req.name = fresh.name.clone();
                        r.saved = collection::request_text(&r.path, &fresh);
                    }
                }
                TabKind::Settings(s) if s.dir.starts_with(old) => {
                    let rel = s.dir.strip_prefix(old).unwrap_or(Path::new("")).to_path_buf();
                    s.dir = if rel.as_os_str().is_empty() { new.to_path_buf() } else { new.join(rel) };
                }
                _ => {}
            }
        }
    }

    fn env_window(&mut self, ctx: &egui::Context) {
        let Some((root, mut sel)) = self.env_window.clone() else { return };
        let mut open = true;
        let mut action: Option<Dialog> = None;
        let mut status = None;
        egui::Window::new("Environments").collapsible(false).resizable(true).default_size([820.0, 480.0]).open(&mut open).show(ctx, |ui| {
            let Some(c) = self.collections.iter_mut().find(|c| c.root == root) else { return };
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(180.0);
                    for (i, e) in c.envs.iter().enumerate() {
                        if ui.selectable_label(i == sel, &e.name).clicked() {
                            sel = i;
                        }
                    }
                    ui.add_space(8.0);
                    if ui::link_button(ui, "+ Create").clicked() {
                        action = Some(Dialog::NewEnv { root: root.clone(), name: String::new() });
                    }
                });
                ui.separator();
                ui.vertical(|ui| {
                    let Some(env) = c.envs.get_mut(sel) else {
                        ui.label(RichText::new("No environments yet. Create one to get started.").color(MUTED));
                        return;
                    };
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&env.name).strong().size(15.0));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.add(egui::Button::new(RichText::new("Delete").color(RED)).frame(false)).clicked() {
                                action = Some(Dialog::DeleteEnv { root: root.clone(), index: sel });
                            }
                        });
                    });
                    ui.add_space(4.0);
                    ui::kv_table(ui, &format!("env-{}", env.path.display()), &mut env.vars, TableCfg { description: false, toggle: true, ..Default::default() });
                    if !env.secrets.is_empty() {
                        ui.add_space(8.0);
                        ui::section_title(ui, "Secrets (values stay in memory; Bruno keeps them encrypted in its own storage)");
                        let secrets = self.secrets.entry(env.path.clone()).or_default();
                        egui::Grid::new("secrets").num_columns(2).show(ui, |ui| {
                            for name in &env.secrets {
                                ui.label(name);
                                let v = secrets.entry(name.clone()).or_default();
                                ui.add(TextEdit::singleline(v).password(true).desired_width(300.0));
                                ui.end_row();
                            }
                        });
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui::primary_button(ui, "Save").clicked() {
                            status = Some(collection::save_env(env).map(|()| format!("Saved environment {}", env.name)).unwrap_or_else(|e| format!("Save failed: {e}")));
                        }
                        if ui.button("Reset").clicked()
                            && let Ok(fresh) = collection::load_env(&env.path)
                        {
                            *env = fresh;
                        }
                    });
                });
            });
            ui.separator();
            ui::section_title(ui, "Runtime variables (set by scripts and post-response vars)");
            let rt = self.runtime.entry(root.clone()).or_default();
            let mut keys: Vec<_> = rt.keys().cloned().collect();
            keys.sort();
            egui::Grid::new("runtime-vars").num_columns(2).striped(true).show(ui, |ui| {
                for k in keys {
                    ui.label(&k);
                    ui.label(RichText::new(rt[&k].as_str().map(str::to_string).unwrap_or_else(|| rt[&k].to_string())).monospace());
                    ui.end_row();
                }
            });
            if !rt.is_empty() && ui::link_button(ui, "Clear runtime variables").clicked() {
                rt.clear();
            }
        });
        if let Some(s) = status {
            self.status = s;
        }
        if let Some(a) = action {
            self.dialog = Some(a);
        }
        self.env_window = if open { Some((root, sel)) } else { None };
    }
}

fn exec_vars(job: &Job) -> Vars {
    // same precedence as exec::Scopes::flat (collection < env < folder < request < runtime)
    let chain = job.collection.settings_chain(&job.path);
    let mut vars = Vars::new();
    for (k, v) in std::env::vars().chain(job.collection.dotenv.clone()) {
        vars.insert(format!("process.env.{k}"), v);
    }
    let on = |p: &[Pair]| p.iter().filter(|x| x.enabled).map(|x| (x.key.trim_start_matches('@').to_string(), x.value.clone())).collect::<Vec<_>>();
    vars.extend(on(&chain[0].vars_req));
    if let Some(e) = &job.env {
        let resolved: Vec<_> = on(&e.vars).into_iter().map(|(k, v)| (k, crate::interp::interpolate(&v, &vars, false))).collect();
        vars.extend(resolved);
        vars.extend(job.secrets.clone());
    }
    for f in &chain[1..] {
        vars.extend(on(&f.vars_req));
    }
    vars.extend(on(&job.request.vars_req));
    vars.extend(job.runtime.iter().map(|(k, v)| (k.clone(), v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string()))));
    vars
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for i in 0..4 {
            out.push(if i <= chunk.len() { T[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' });
        }
    }
    out
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() > n { format!("{}…", s.chars().take(n).collect::<String>()) } else { s.to_string() }
}

fn dialog_buttons(ui: &mut egui::Ui, ok_label: &str, enabled: bool) -> bool {
    let mut ok = false;
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ok = ui.add_enabled(enabled, egui::Button::new(RichText::new(ok_label).color(Color32::WHITE).strong()).fill(BRAND)).clicked();
            if ui.button("Cancel").clicked() {
                ui.ctx().input_mut(|i| i.events.push(egui::Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }));
            }
        });
    });
    ok
}

/// Was this click on the row's ▸/▾ arrow (toggle) rather than its name (open)?
fn chevron_clicked(resp: &egui::Response, depth: usize) -> bool {
    let left = resp.rect.left() + 4.0 + depth as f32 * 14.0;
    resp.clicked() && resp.interact_pointer_pos().is_some_and(|p| p.x >= left - 4.0 && p.x <= left + 16.0)
}

fn tree_row(ui: &mut egui::Ui, depth: usize, open: Option<bool>, label: RichText, tag: Option<RichText>, selected: bool) -> egui::Response {
    let width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, 24.0), egui::Sense::click());
    if selected {
        ui.painter().rect_filled(rect, 4.0, Color32::from_rgb(232, 232, 232));
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, 4.0, Color32::from_rgb(240, 240, 240));
    }
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(egui::vec2(4.0, 0.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
    child.add_space(depth as f32 * 14.0);
    child.spacing_mut().item_spacing.x = 6.0;
    match open {
        Some(o) => child.label(RichText::new(if o { "▾" } else { "▸" }).color(MUTED)),
        None => child.label(" "),
    };
    if let Some(t) = tag {
        child.label(t);
    }
    child.add(egui::Label::new(label).truncate().selectable(false));
    resp
}

fn folder_menu(ui: &mut egui::Ui, root: &Path, dir: &Path, is_root: bool, acts: &mut Vec<Act>) {
    if ui.button("New Request").clicked() {
        acts.push(Act::Dialog(Dialog::NewRequest { root: root.into(), dir: dir.into(), name: String::new(), kind: "http".into() }));
        ui.close();
    }
    if ui.button("New Folder").clicked() {
        acts.push(Act::Dialog(Dialog::NewFolder { root: root.into(), dir: dir.into(), name: String::new() }));
        ui.close();
    }
    if ui.button("Import cURL").clicked() {
        acts.push(Act::Dialog(Dialog::ImportCurl { root: root.into(), dir: dir.into(), text: String::new(), name: String::new() }));
        ui.close();
    }
    if ui.button("Run").clicked() {
        acts.push(Act::OpenRunner(root.into(), dir.into()));
        ui.close();
    }
    if ui.button("Settings").clicked() {
        acts.push(Act::OpenSettings(root.into(), dir.into()));
        ui.close();
    }
    if !is_root {
        if ui.button("Rename").clicked() {
            let name = collection::Collection::open(root).ok().map(|c| c.load_folder(dir).name).filter(|n| !n.is_empty()).unwrap_or_else(|| dir.file_name().unwrap_or_default().to_string_lossy().to_string());
            acts.push(Act::Dialog(Dialog::Rename { root: root.into(), path: dir.into(), name, folder: true }));
            ui.close();
        }
        if ui.button(RichText::new("Delete").color(RED)).clicked() {
            acts.push(Act::Dialog(Dialog::Delete { root: root.into(), path: dir.into(), label: format!("folder \"{}\" and everything in it", dir.file_name().unwrap_or_default().to_string_lossy()) }));
            ui.close();
        }
    }
}

fn matches_filter(n: &Node, filter: &str) -> bool {
    filter.is_empty()
        || match n {
            Node::Request { name, .. } => name.to_lowercase().contains(filter),
            Node::Folder { name, children, .. } => name.to_lowercase().contains(filter) || children.iter().any(|c| matches_filter(c, filter)),
        }
}

#[allow(clippy::too_many_arguments)]
fn tree(ui: &mut egui::Ui, root: &Path, nodes: &[Node], depth: usize, expanded: &HashSet<PathBuf>, current: Option<&Path>, filter: &str, acts: &mut Vec<Act>) {
    for n in nodes.iter().filter(|n| matches_filter(n, filter)) {
        match n {
            Node::Folder { name, path, children } => {
                let open = expanded.contains(path) || !filter.is_empty();
                let r = tree_row(ui, depth, Some(open), RichText::new(name), None, current == Some(path.as_path()));
                // arrow toggles; the name opens folder settings and only ever expands
                if chevron_clicked(&r, depth) {
                    acts.push(Act::Activate(path.clone()));
                } else if r.clicked() {
                    acts.push(Act::Expand(path.clone()));
                    acts.push(Act::OpenSettings(root.into(), path.clone()));
                }
                r.context_menu(|ui| folder_menu(ui, root, path, false, acts));
                if open {
                    tree(ui, root, children, depth + 1, expanded, current, filter, acts);
                }
            }
            Node::Request { name, method, kind, path } => {
                let r = tree_row(ui, depth, None, RichText::new(name), Some(ui::method_tag(method, kind)), current == Some(path.as_path()));
                if r.clicked() {
                    acts.push(Act::OpenRequest(root.into(), path.clone()));
                }
                r.context_menu(|ui| {
                    if ui.button("Rename").clicked() {
                        acts.push(Act::Dialog(Dialog::Rename { root: root.into(), path: path.clone(), name: name.clone(), folder: false }));
                        ui.close();
                    }
                    if ui.button("Clone").clicked() {
                        acts.push(Act::Dialog(Dialog::Clone { root: root.into(), path: path.clone(), name: format!("{name} copy") }));
                        ui.close();
                    }
                    if ui.button("Run").clicked() {
                        acts.push(Act::OpenRequest(root.into(), path.clone()));
                        ui.close();
                    }
                    if (kind == "http" || kind == "graphql") && ui.button("Load test").clicked() {
                        acts.push(Act::OpenLoad(root.into(), path.clone()));
                        ui.close();
                    }
                    if ui.button(RichText::new("Delete").color(RED)).clicked() {
                        acts.push(Act::Dialog(Dialog::Delete { root: root.into(), path: path.clone(), label: format!("request \"{name}\"") }));
                        ui.close();
                    }
                });
            }
        }
    }
}

// ---------- request view ----------

fn url_bar(ui: &mut egui::Ui, t: &mut ReqTab, send: &mut bool, save: &mut bool) {
    let kind = t.req.kind.clone();
    egui::Frame::new().stroke(Stroke::new(1.0, BORDER)).corner_radius(CornerRadius::same(5)).inner_margin(egui::Margin::symmetric(8, 3)).show(ui, |ui| {
        ui.horizontal(|ui| {
            match kind.as_str() {
                "grpc" => {
                    ui.label(ui::method_tag("", "grpc"));
                }
                "ws" => {
                    ui.label(ui::method_tag("", "ws"));
                }
                _ => {
                    egui::ComboBox::from_id_salt(("method", &t.path))
                        .width(70.0)
                        .selected_text(RichText::new(t.req.method.to_uppercase()).color(ui::method_color(&t.req.method)).strong())
                        .show_ui(ui, |ui| {
                            for m in crate::bru::METHODS {
                                ui.selectable_value(&mut t.req.method, m.to_string(), RichText::new(m.to_uppercase()).color(ui::method_color(m)));
                            }
                        });
                }
            }
            let right = match kind.as_str() {
                "grpc" => 360.0,
                _ => 150.0,
            };
            let hint = match kind.as_str() {
                "grpc" => "localhost:50051",
                "ws" => "ws://localhost:8080",
                _ => "Enter URL or paste a cURL request",
            };
            let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, w: f32| ui::highlight_vars(ui, buf.as_str(), w);
            let r = ui.add(TextEdit::singleline(&mut t.req.url).hint_text(hint).frame(egui::Frame::NONE).desired_width(ui.available_width() - right).font(egui::TextStyle::Monospace).layouter(&mut layouter));
            if r.changed() && (kind == "http" || kind == "graphql") {
                // pasting a cURL command into the URL bar imports it, like Bruno
                let pasted = t.req.url.trim_start();
                if pasted.starts_with("curl ") || pasted.starts_with("curl.exe ") {
                    if let Ok(parsed) = crate::curl::parse_curl(pasted) {
                        apply_curl(&mut t.req, parsed);
                    }
                } else {
                    sync_from_url(&mut t.req);
                }
            }
            if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                *send = true;
            }
            if kind == "grpc" {
                let label = if t.req.method.is_empty() { "Select method".to_string() } else { t.req.method.clone() };
                egui::ComboBox::from_id_salt(("grpc-method", &t.path)).width(200.0).selected_text(truncate(&label, 30)).show_ui(ui, |ui| {
                    if t.grpc.methods.is_empty() {
                        ui.label(RichText::new("Load methods from a proto file or reflection").color(MUTED));
                    }
                    for m in &t.grpc.methods {
                        if ui.selectable_label(t.req.method == m.path, format!("{}  ·  {}", m.path, m.kind)).clicked() {
                            t.req.method = m.path.clone();
                            t.req.grpc_method_type = m.kind.to_string();
                        }
                    }
                });
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let label = match kind.as_str() {
                    "grpc" if t.grpc.rx.is_some() => "Running…",
                    "grpc" => "Invoke",
                    "ws" if t.ws.connected => "Close",
                    "ws" if t.ws.tx.is_some() => "Connecting…",
                    "ws" => "Connect",
                    _ if t.pending.is_some() => "Sending…",
                    _ => "Send",
                };
                if ui::primary_button(ui, label).clicked() {
                    *send = true;
                }
                if ui.add(egui::Button::new(RichText::new("💾").color(MUTED)).frame(false)).on_hover_text("Save (Ctrl+S)").clicked() {
                    *save = true;
                }
                if kind == "http" || kind == "graphql" {
                    if ui.add(egui::Button::new(RichText::new("</>").color(MUTED).monospace()).frame(false)).on_hover_text("Generate code (cURL)").clicked() {
                        ui.data_mut(|d| d.insert_temp(egui::Id::unique("gen-code"), t.path.clone()));
                    }
                    if ui.add(egui::Button::new(RichText::new("⚡").color(MUTED)).frame(false)).on_hover_text("Load test this request").clicked() {
                        ui.data_mut(|d| d.insert_temp(egui::Id::unique("load-test"), t.path.clone()));
                    }
                }
            });
        });
    });
}

/// Replace the request's HTTP parts with an imported cURL request (name, file and scripts stay).
fn apply_curl(req: &mut Request, c: Request) {
    req.method = c.method;
    req.url = c.url;
    req.query = c.query;
    req.path_params = c.path_params;
    req.headers = c.headers;
    req.auth = c.auth;
    req.body_mode = c.body_mode;
    req.body_json = c.body_json;
    req.body_text = c.body_text;
    req.body_xml = c.body_xml;
    req.form = c.form;
    req.multipart = c.multipart;
    req.files = c.files;
}

fn request_view(ui: &mut egui::Ui, t: &mut ReqTab, col: Option<&Collection>, send: &mut bool, save: &mut bool) {
    url_bar(ui, t, send, save);
    ui.add_space(6.0);
    egui::Panel::right(("response-panel", &t.path)).resizable(true).default_size(ui.available_width() * 0.48).min_size(260.0).show(ui, |ui| {
        ui.add_space(2.0);
        match t.req.kind.as_str() {
            "grpc" => grpc_response(ui, t),
            "ws" => ws_response(ui, t),
            _ => http_response(ui, t),
        }
    });
    egui::CentralPanel::no_frame().show(ui, |ui| request_tabs(ui, t, col));
}

fn request_tabs(ui: &mut egui::Ui, t: &mut ReqTab, col: Option<&Collection>) {
    let r = &t.req;
    let enabled = |p: &[Pair]| p.iter().filter(|x| x.enabled).count();
    let body_count = usize::from(r.body_mode != "none" && !r.body_mode.is_empty());
    let vars = enabled(&r.vars_req) + enabled(&r.vars_res);
    let script = usize::from(!r.script_req.trim().is_empty()) + usize::from(!r.script_res.trim().is_empty());
    let tests = usize::from(!r.tests.trim().is_empty());
    let docs = usize::from(!r.docs.trim().is_empty());
    let auth = usize::from(!matches!(r.auth.mode.as_str(), "none" | "inherit" | ""));
    let items: Vec<(ReqSub, &str, usize)> = match r.kind.as_str() {
        "grpc" => vec![(ReqSub::Messages, "Message", r.messages.len()), (ReqSub::Headers, "Metadata", enabled(&r.headers)), (ReqSub::Auth, "Auth", auth), (ReqSub::Docs, "Docs", docs)],
        "ws" => vec![(ReqSub::Messages, "Message", r.messages.len()), (ReqSub::Headers, "Headers", enabled(&r.headers)), (ReqSub::Auth, "Auth", auth), (ReqSub::Settings, "Settings", 0), (ReqSub::Docs, "Docs", docs)],
        kind => vec![
            (ReqSub::Params, "Params", enabled(&r.query) + r.path_params.len()),
            (ReqSub::Body, if kind == "graphql" { "Query" } else { "Body" }, body_count),
            (ReqSub::Headers, "Headers", enabled(&r.headers)),
            (ReqSub::Auth, "Auth", auth),
            (ReqSub::Vars, "Vars", vars),
            (ReqSub::Script, "Script", script),
            (ReqSub::Assert, "Assert", enabled(&r.asserts)),
            (ReqSub::Tests, "Tests", tests),
            (ReqSub::Docs, "Docs", docs),
            (ReqSub::Settings, "Settings", 0),
        ],
    };
    if !items.iter().any(|(s, _, _)| *s == t.sub) {
        t.sub = items[0].0;
    }
    ui::tab_strip(ui, &items, &mut t.sub);
    let root = col.map(|c| c.root.clone()).unwrap_or_default();
    egui::ScrollArea::vertical().id_salt(("req-scroll", &t.path)).auto_shrink([false, false]).show(ui, |ui| {
        let req = &mut t.req;
        let table = TableCfg { description: true, toggle: true, ..Default::default() };
        match t.sub {
            ReqSub::Params => {
                ui::section_title(ui, "Query");
                if ui::kv_table(ui, "query", &mut req.query, table) {
                    req.url = collection::url_with_query(&req.url, &req.query);
                }
                ui.add_space(10.0);
                ui::section_title(ui, "Path");
                if req.path_params.is_empty() {
                    ui.label(RichText::new("Add :name segments to the URL to create path parameters.").color(MUTED).size(12.0));
                } else {
                    egui::Grid::new("path-params").num_columns(3).spacing([12.0, 4.0]).show(ui, |ui| {
                        for p in &mut req.path_params {
                            ui.label(&p.key);
                            ui.add(TextEdit::singleline(&mut p.value).hint_text("Value").desired_width(300.0).font(egui::TextStyle::Monospace));
                            ui.add(TextEdit::singleline(&mut p.description).hint_text("Description").desired_width(200.0));
                            ui.end_row();
                        }
                    });
                }
            }
            ReqSub::Body if req.kind == "graphql" => {
                ui::section_title(ui, "Query");
                ui::code_editor(ui, &mut req.gql_query, 14);
                ui.add_space(8.0);
                ui::section_title(ui, "Variables (JSON)");
                ui::code_editor(ui, &mut req.gql_vars, 6);
            }
            ReqSub::Body => {
                let modes = [("none", "No Body"), ("json", "JSON"), ("xml", "XML"), ("text", "TEXT"), ("sparql", "SPARQL"), ("formUrlEncoded", "Form URL Encoded"), ("multipartForm", "Multipart Form"), ("file", "File / Binary")];
                let label = modes.iter().find(|(m, _)| *m == req.body_mode).map(|(_, l)| *l).unwrap_or("No Body");
                egui::ComboBox::from_id_salt("body-mode").selected_text(label).width(170.0).show_ui(ui, |ui| {
                    for (m, l) in modes {
                        ui.selectable_value(&mut req.body_mode, m.to_string(), l);
                    }
                });
                ui.add_space(4.0);
                match req.body_mode.as_str() {
                    "json" => {
                        ui::code_editor(ui, &mut req.body_json, 18);
                        if ui::link_button(ui, "Prettify").clicked()
                            && let Ok(v) = serde_json::from_str::<serde_json::Value>(&req.body_json)
                        {
                            req.body_json = serde_json::to_string_pretty(&v).unwrap_or_default();
                        }
                    }
                    "xml" => {
                        ui::code_editor(ui, &mut req.body_xml, 18);
                    }
                    "text" => {
                        ui::code_editor(ui, &mut req.body_text, 18);
                    }
                    "sparql" => {
                        ui::code_editor(ui, &mut req.body_sparql, 18);
                    }
                    "formUrlEncoded" => {
                        ui::kv_table(ui, "form", &mut req.form, table);
                    }
                    "multipartForm" => {
                        ui::kv_table(ui, "multipart", &mut req.multipart, TableCfg { multipart: true, ..table });
                    }
                    "file" => file_body(ui, req, &root),
                    _ => {
                        ui.label(RichText::new("No body").color(MUTED));
                    }
                }
            }
            ReqSub::Headers => {
                ui::kv_table(ui, "headers", &mut req.headers, table);
            }
            ReqSub::Auth => auth_editor(ui, &mut req.auth, true),
            ReqSub::Vars => {
                ui::section_title(ui, "Pre Request");
                ui::kv_table(ui, "vars-req", &mut req.vars_req, table);
                ui.add_space(10.0);
                ui::section_title(ui, "Post Response");
                ui::kv_table(ui, "vars-res", &mut req.vars_res, TableCfg { value_label: "Expr", ..table });
                ui.label(RichText::new("Expr is a JavaScript expression, e.g. res.body.token or res('data.items[0].id').").color(MUTED).size(12.0));
            }
            ReqSub::Script => {
                ui::section_title(ui, "Pre Request");
                ui::code_editor(ui, &mut req.script_req, 10);
                ui.add_space(8.0);
                ui::section_title(ui, "Post Response");
                ui::code_editor(ui, &mut req.script_res, 10);
            }
            ReqSub::Assert => {
                ui::kv_table(ui, "asserts", &mut req.asserts, TableCfg { value_label: "Operator & value", ..table });
                ui.label(RichText::new("Name is an expression like res.status or res.body.id; value is e.g. `eq 200`, `contains \"ok\"`, `isArray`.").color(MUTED).size(12.0));
            }
            ReqSub::Tests => {
                ui::code_editor(ui, &mut req.tests, 18);
                ui.label(RichText::new("test(\"status is 200\", () => expect(res.getStatus()).to.equal(200));").color(MUTED).monospace().size(11.5));
            }
            ReqSub::Docs => {
                ui::code_editor(ui, &mut req.docs, 18);
            }
            ReqSub::Settings => request_settings(ui, req),
            ReqSub::Messages => messages_editor(ui, t, &root),
        }
    });
}

fn file_body(ui: &mut egui::Ui, req: &mut Request, root: &Path) {
    let mut remove = None;
    let mut select = None;
    for (i, f) in req.files.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            if ui.radio(f.enabled, "").clicked() {
                select = Some(i);
            }
            ui.label(RichText::new(&f.value).monospace());
            ui.add(TextEdit::singleline(&mut f.content_type).hint_text("content-type").desired_width(180.0));
            if ui.small_button("🗑").clicked() {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = select {
        for (j, f) in req.files.iter_mut().enumerate() {
            f.enabled = i == j;
        }
    }
    if let Some(i) = remove {
        req.files.remove(i);
    }
    if ui.button("Add file…").clicked()
        && let Some(p) = rfd::FileDialog::new().pick_file()
    {
        // store collection-relative paths when possible, like Bruno
        let shown = p.strip_prefix(root).map(|r| r.to_string_lossy().replace('\\', "/")).unwrap_or_else(|_| p.to_string_lossy().to_string());
        let first = req.files.is_empty();
        req.files.push(Pair { key: "file".into(), value: shown, enabled: first, ..Default::default() });
    }
}

fn auth_editor(ui: &mut egui::Ui, auth: &mut crate::model::Auth, allow_inherit: bool) {
    let mut modes = vec![("none", "No Auth"), ("basic", "Basic Auth"), ("bearer", "Bearer Token")];
    if allow_inherit {
        modes.insert(0, ("inherit", "Inherit"));
    }
    let known = modes.iter().any(|(m, _)| *m == auth.mode);
    let label = modes.iter().find(|(m, _)| *m == auth.mode).map(|(_, l)| l.to_string()).unwrap_or(auth.mode.clone());
    egui::ComboBox::from_id_salt("auth-mode").selected_text(label).width(170.0).show_ui(ui, |ui| {
        for (m, l) in &modes {
            ui.selectable_value(&mut auth.mode, m.to_string(), *l);
        }
    });
    ui.add_space(6.0);
    egui::Grid::new("auth-grid").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| match auth.mode.as_str() {
        "basic" => {
            ui.label("Username");
            ui.add(TextEdit::singleline(&mut auth.username).desired_width(320.0).font(egui::TextStyle::Monospace));
            ui.end_row();
            ui.label("Password");
            ui.add(TextEdit::singleline(&mut auth.password).password(true).desired_width(320.0).font(egui::TextStyle::Monospace));
            ui.end_row();
        }
        "bearer" => {
            ui.label("Token");
            ui.add(TextEdit::singleline(&mut auth.token).desired_width(420.0).font(egui::TextStyle::Monospace));
            ui.end_row();
        }
        "inherit" => {
            ui.label(RichText::new("Auth is inherited from the nearest folder or the collection.").color(MUTED));
            ui.end_row();
        }
        "none" => {}
        _ if !known => {
            ui.label(RichText::new(format!("`{}` auth is kept in the file but not applied by ZenBruno.", auth.mode)).color(MUTED));
            ui.end_row();
        }
        _ => {}
    });
}

fn request_settings(ui: &mut egui::Ui, req: &mut Request) {
    let flag = |ui: &mut egui::Ui, req: &mut Request, key: &str, label: &str, default: bool| {
        let mut v = req.setting(key).map(|s| s == "true").unwrap_or(default);
        if ui.checkbox(&mut v, label).changed() {
            req.set_setting(key, v.to_string());
        }
    };
    let number = |ui: &mut egui::Ui, req: &mut Request, key: &str, label: &str| {
        ui.horizontal(|ui| {
            ui.label(label);
            let mut v = req.setting(key).unwrap_or("").to_string();
            if ui.add(TextEdit::singleline(&mut v).desired_width(100.0)).changed() {
                req.set_setting(key, v.trim().to_string());
            }
        });
    };
    if req.kind == "ws" {
        number(ui, req, "timeout", "Timeout (ms)");
        number(ui, req, "keepAliveInterval", "Keep-alive ping interval (ms, 0 = off)");
        return;
    }
    flag(ui, req, "encodeUrl", "URL encoding", true);
    flag(ui, req, "followRedirects", "Automatically follow redirects", true);
    number(ui, req, "maxRedirects", "Max redirects");
    number(ui, req, "timeout", "Timeout (ms, 0 = none)");
}

fn messages_editor(ui: &mut egui::Ui, t: &mut ReqTab, root: &Path) {
    let is_grpc = t.req.kind == "grpc";
    if is_grpc {
        ui.horizontal(|ui| {
            ui.label("Proto file");
            ui.add(TextEdit::singleline(&mut t.req.proto_path).hint_text("empty = use server reflection").desired_width(280.0));
            if ui.button("Browse…").clicked()
                && let Some(p) = rfd::FileDialog::new().add_filter("proto", &["proto"]).pick_file()
            {
                t.req.proto_path = p.strip_prefix(root).map(|r| r.to_string_lossy().replace('\\', "/")).unwrap_or_else(|_| p.to_string_lossy().to_string());
            }
            if t.grpc.loading.is_some() {
                ui.spinner();
            }
        });
        ui.horizontal(|ui| {
            if ui.button("Load from proto").clicked() {
                ui.data_mut(|d| d.insert_temp(egui::Id::unique("grpc-load"), (t.path.clone(), false)));
            }
            if ui.button("Use reflection").clicked() {
                ui.data_mut(|d| d.insert_temp(egui::Id::unique("grpc-load"), (t.path.clone(), true)));
            }
            if let (Some(pool), false) = (&t.grpc.pool, t.req.method.is_empty())
                && ui.button("Generate sample message").clicked()
                && let Ok(sample) = grpc::sample_message(pool, &t.req.method)
            {
                match t.req.messages.first_mut() {
                    Some(m) => m.content = sample,
                    None => t.req.messages.push(Message { name: "message 1".into(), content: sample, ..Default::default() }),
                }
            }
            if !t.grpc.methods.is_empty() {
                ui.label(RichText::new(format!("{} methods loaded", t.grpc.methods.len())).color(GREEN));
            }
        });
        if let Some(e) = &t.grpc.load_error {
            ui.colored_label(RED, e);
        }
        ui.add_space(6.0);
    }
    let mut remove = None;
    let mut send_idx = None;
    let n = t.req.messages.len();
    for (i, m) in t.req.messages.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.add(TextEdit::singleline(&mut m.name).desired_width(200.0));
            if !is_grpc {
                egui::ComboBox::from_id_salt(("ws-type", i)).width(70.0).selected_text(if m.kind.is_empty() { "text" } else { &m.kind }).show_ui(ui, |ui| {
                    for k in ["text", "json", "xml"] {
                        ui.selectable_value(&mut m.kind, k.to_string(), k);
                    }
                });
                if ui.add_enabled(t.ws.connected, egui::Button::new(RichText::new("Send").color(BRAND))).clicked() {
                    send_idx = Some(i);
                }
            }
            if n > 1 && ui.small_button("🗑").clicked() {
                remove = Some(i);
            }
        });
        ui::code_editor(ui, &mut m.content, if n > 1 { 6 } else { 12 });
        ui.add_space(6.0);
    }
    if let Some(i) = remove {
        t.req.messages.remove(i);
    }
    if is_grpc && !t.req.grpc_method_type.contains("streaming") && n > 0 {
        ui.label(RichText::new("Unary and server-streaming calls send the first message.").color(MUTED).size(12.0));
    }
    if ui::link_button(ui, "+ Add message").clicked() {
        let kind = if is_grpc { String::new() } else { "json".into() };
        t.req.messages.push(Message { name: format!("message {}", n + 1), kind, content: "{}".into(), selected: false });
    }
    if let Some(i) = send_idx {
        ui.data_mut(|d| d.insert_temp(egui::Id::unique("ws-send"), (t.path.clone(), i)));
    }
}

fn status_line(ui: &mut egui::Ui, status: Option<(u16, &str)>, time_ms: Option<u128>, size: Option<usize>) {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if let Some(s) = size {
            ui.label(RichText::new(ui::human_size(s)).color(MUTED));
        }
        if let Some(t) = time_ms {
            ui.label(RichText::new(format!("{t}ms")).color(MUTED));
        }
        if let Some((code, text)) = status {
            ui.label(RichText::new(format!("{code} {text}")).color(ui::status_color(code)).strong());
        }
    });
}

fn http_response(ui: &mut egui::Ui, t: &mut ReqTab) {
    if t.pending.is_some() {
        ui.add_space(40.0);
        ui.vertical_centered(|ui| {
            ui.spinner();
            let secs = t.sent_at.map(|s| s.elapsed().as_secs_f32()).unwrap_or(0.0);
            ui.label(RichText::new(format!("Sending request… {secs:.1}s")).color(MUTED));
        });
        return;
    }
    let Some(o) = &t.outcome else {
        ZenApp::empty_state(ui);
        return;
    };
    let tests_total = o.tests.len() + o.asserts.len();
    let tests_failed = o.tests.iter().filter(|x| !x.passed).count() + o.asserts.iter().filter(|a| a.1.is_err()).count();
    ui.horizontal(|ui| {
        let items = [(ResSub::Response, "Response", 0), (ResSub::Headers, "Headers", o.response.as_ref().map(|r| r.headers.len()).unwrap_or(0)), (ResSub::Timeline, "Timeline", 0), (ResSub::Tests, "Tests", tests_total)];
        ui::tab_strip(ui, &items, &mut t.res_sub);
        if let Some(r) = &o.response {
            status_line(ui, Some((r.status, &r.status_text)), Some(r.time_ms), Some(r.body.len()));
        }
    });
    egui::ScrollArea::both().id_salt(("res-scroll", &t.path)).auto_shrink([false, false]).show(ui, |ui| match t.res_sub {
        ResSub::Response => {
            if let Some(e) = &o.error {
                ui.colored_label(RED, e);
                ui.add_space(6.0);
            }
            if o.skipped {
                ui.label(RichText::new("Request skipped by script (bru.runner.skipRequest).").color(MUTED));
            }
            if o.response.is_some() {
                ui::code_view(ui, &t.pretty);
            }
        }
        ResSub::Headers => {
            if let Some(r) = &o.response {
                egui::Grid::new("res-headers").striped(true).num_columns(2).spacing([16.0, 4.0]).show(ui, |ui| {
                    for (k, v) in &r.headers {
                        ui.label(RichText::new(k).strong());
                        ui.label(RichText::new(v).monospace());
                        ui.end_row();
                    }
                });
            }
        }
        ResSub::Timeline => {
            if let Some(p) = &o.sent {
                ui.label(RichText::new(format!("{} {}", p.method, p.url)).monospace().strong());
                ui.add_space(4.0);
                for (k, v) in &p.headers {
                    ui.label(RichText::new(format!("{k}: {v}")).monospace().color(Color32::from_rgb(90, 90, 90)));
                }
                if p.basic_auth.is_some() {
                    ui.label(RichText::new("authorization: Basic ••••••").monospace().color(Color32::from_rgb(90, 90, 90)));
                }
                if let crate::http::BodyData::Raw(b) = &p.body
                    && !b.is_empty()
                {
                    ui.add_space(4.0);
                    ui::code_view(ui, b);
                }
            }
            if let Some(r) = &o.response {
                ui.add_space(8.0);
                ui.label(RichText::new(format!("← {} {} ({} ms)", r.status, r.status_text, r.time_ms)).monospace().color(ui::status_color(r.status)));
                for (k, v) in &r.headers {
                    ui.label(RichText::new(format!("{k}: {v}")).monospace().color(Color32::from_rgb(90, 90, 90)));
                }
            }
            if !o.logs.is_empty() {
                ui.add_space(10.0);
                ui::section_title(ui, "Console");
                for (level, msg) in &o.logs {
                    let color = match level.as_str() {
                        "error" => RED,
                        "warn" => Color32::from_rgb(198, 121, 16),
                        _ => TEXT,
                    };
                    ui.label(RichText::new(format!("[{level}] {msg}")).monospace().color(color));
                }
            }
        }
        ResSub::Tests => {
            if tests_total == 0 {
                ui.label(RichText::new("No tests or assertions ran.").color(MUTED));
            } else {
                ui.label(RichText::new(format!("Passed {} / {}", tests_total - tests_failed, tests_total)).color(if tests_failed == 0 { GREEN } else { RED }).strong());
                ui.add_space(4.0);
            }
            if !o.tests.is_empty() {
                ui::section_title(ui, "Tests");
                for x in &o.tests {
                    if x.passed {
                        ui.colored_label(GREEN, format!("✔ {}", x.name));
                    } else {
                        ui.colored_label(RED, format!("✖ {} — {}", x.name, x.error));
                    }
                }
            }
            if !o.asserts.is_empty() {
                ui.add_space(6.0);
                ui::section_title(ui, "Assertions");
                for (name, r) in &o.asserts {
                    match r {
                        Ok(()) => ui.colored_label(GREEN, format!("✔ {name}")),
                        Err(e) => ui.colored_label(RED, format!("✖ {name} — {e}")),
                    };
                }
            }
        }
    });
}

fn grpc_response(ui: &mut egui::Ui, t: &mut ReqTab) {
    if t.grpc.events.is_empty() && t.grpc.rx.is_none() {
        ZenApp::empty_state(ui);
        return;
    }
    let status = t.grpc.events.iter().rev().find_map(|e| match e {
        grpc::Event::Status(name, code, msg) => Some((name.clone(), *code, msg.clone())),
        _ => None,
    });
    ui.horizontal(|ui| {
        let n = t.grpc.events.iter().filter(|e| matches!(e, grpc::Event::Message(_))).count();
        ui::tab_strip(ui, &[(ResSub::Response, "Response", n), (ResSub::Headers, "Metadata", 0)], &mut t.res_sub);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if t.grpc.rx.is_some() {
                ui.spinner();
            } else {
                ui.label(RichText::new(format!("{}ms", t.grpc.elapsed_ms)).color(MUTED));
            }
            if let Some((name, code, _)) = &status {
                ui.label(RichText::new(format!("{code} {name}")).color(if *code == 0 { GREEN } else { RED }).strong());
            }
        });
    });
    egui::ScrollArea::both().id_salt(("grpc-res", &t.path)).auto_shrink([false, false]).show(ui, |ui| {
        for e in &t.grpc.events {
            match (t.res_sub, e) {
                (ResSub::Headers, grpc::Event::Headers(h) | grpc::Event::Trailers(h)) => {
                    for (k, v) in h {
                        ui.label(RichText::new(format!("{k}: {v}")).monospace());
                    }
                }
                (ResSub::Headers, _) => {}
                (_, grpc::Event::Message(m)) => {
                    egui::Frame::new().stroke(Stroke::new(1.0, BORDER)).corner_radius(CornerRadius::same(4)).inner_margin(egui::Margin::same(6)).show(ui, |ui| ui::code_view(ui, m));
                    ui.add_space(4.0);
                }
                (_, grpc::Event::Status(_, code, msg)) if *code != 0 => {
                    ui.colored_label(RED, msg);
                }
                (_, grpc::Event::Error(e)) => {
                    ui.colored_label(RED, e);
                }
                _ => {}
            }
        }
    });
}

fn ws_response(ui: &mut egui::Ui, t: &mut ReqTab) {
    if t.ws.log.is_empty() {
        ZenApp::empty_state(ui);
        return;
    }
    ui.horizontal(|ui| {
        ui::tab_strip(ui, &[(ResSub::Response, "Messages", t.ws.log.len())], &mut t.res_sub);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui::link_button(ui, "Clear").clicked() {
                t.ws.log.clear();
            }
            ui.label(if t.ws.connected { RichText::new("● Connected").color(GREEN) } else { RichText::new("● Disconnected").color(MUTED) });
        });
    });
    egui::ScrollArea::vertical().id_salt(("ws-log", &t.path)).stick_to_bottom(true).auto_shrink([false, false]).show(ui, |ui| {
        for (time, e) in &t.ws.log {
            let (arrow, color, text) = match e {
                ws::Event::Opened { status, .. } => ("✔", GREEN, format!("Connected ({status})")),
                ws::Event::Sent(s) => ("↑", BRAND, s.clone()),
                ws::Event::Received(s) => ("↓", GREEN, s.clone()),
                ws::Event::Closed(s) => ("■", MUTED, format!("Closed: {s}")),
                ws::Event::Error(s) => ("✖", RED, s.clone()),
            };
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(arrow).color(color).strong());
                ui.label(RichText::new(time).color(MUTED).size(11.0));
                ui.label(RichText::new(text).monospace());
            });
        }
    });
}

// ---------- folder / collection settings ----------

fn settings_view(ui: &mut egui::Ui, t: &mut SettingsTab, c: &Collection, save: &mut bool, acts: &mut Vec<Act>) {
    ui.horizontal(|ui| {
        let on = |p: &[Pair]| p.iter().filter(|x| x.enabled).count();
        let d = &t.data;
        let mut items = vec![
            (SetSub::Headers, "Headers", on(&d.headers)),
            (SetSub::Vars, "Vars", on(&d.vars_req) + on(&d.vars_res)),
            (SetSub::Auth, "Auth", usize::from(!matches!(d.auth.mode.as_str(), "none" | "inherit" | ""))),
            (SetSub::Script, "Script", usize::from(!d.script_req.trim().is_empty()) + usize::from(!d.script_res.trim().is_empty())),
            (SetSub::Tests, "Tests", usize::from(!d.tests.trim().is_empty())),
            (SetSub::Docs, "Docs", usize::from(!d.docs.trim().is_empty())),
        ];
        items.insert(0, (SetSub::Overview, if t.is_collection { "Overview" } else { "Info" }, 0));
        ui::tab_strip(ui, &items, &mut t.sub);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui::primary_button(ui, "Save").clicked() {
                *save = true;
            }
        });
    });
    egui::ScrollArea::vertical().id_salt(("settings", &t.dir)).auto_shrink([false, false]).show(ui, |ui| {
        let d = &mut t.data;
        let table = TableCfg { description: true, toggle: true, ..Default::default() };
        match t.sub {
            SetSub::Overview => {
                let title = if t.is_collection { c.name.clone() } else { d.name.clone() };
                ui.label(RichText::new(format!("📦 {title}")).strong().size(17.0));
                ui.add_space(10.0);
                let row = |ui: &mut egui::Ui, icon: &str, head: &str, body: String| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(icon).size(20.0));
                        ui.vertical(|ui| {
                            ui.label(RichText::new(head).strong());
                            ui.label(RichText::new(body).color(MUTED));
                        });
                    });
                    ui.add_space(8.0);
                };
                row(ui, "📁", "Location", t.dir.display().to_string());
                if t.is_collection {
                    row(ui, "🗎", "Format", match c.format {
                        Format::Yaml => "OpenCollection YAML (.yml)".into(),
                        Format::Bru => "Bru (.bru)".into(),
                    });
                    row(ui, "🌐", "Environments", format!("{} collection environments", c.envs.len()));
                    row(ui, "⇄", "Requests", format!("{} requests in collection", c.request_count()));
                } else {
                    ui.horizontal(|ui| {
                        ui.label("Folder name");
                        ui.add(TextEdit::singleline(&mut d.name).desired_width(260.0));
                    });
                    ui.label(RichText::new("Changing the name here keeps the directory name; use Rename in the sidebar to rename both.").color(MUTED).size(12.0));
                }
                ui.add_space(6.0);
                if ui::primary_button(ui, &format!("Run {}", if t.is_collection { "collection" } else { "folder" })).clicked() {
                    acts.push(Act::OpenRunner(c.root.clone(), t.dir.clone()));
                }
            }
            SetSub::Headers => {
                ui.label(RichText::new("Headers here apply to every request inside.").color(MUTED).size(12.0));
                ui::kv_table(ui, "folder-headers", &mut d.headers, table);
            }
            SetSub::Vars => {
                ui::section_title(ui, "Pre Request");
                ui::kv_table(ui, "folder-vars-req", &mut d.vars_req, table);
                ui.add_space(10.0);
                ui::section_title(ui, "Post Response");
                ui::kv_table(ui, "folder-vars-res", &mut d.vars_res, TableCfg { value_label: "Expr", ..table });
            }
            SetSub::Auth => auth_editor(ui, &mut d.auth, !t.is_collection),
            SetSub::Script => {
                ui::section_title(ui, "Pre Request");
                ui::code_editor(ui, &mut d.script_req, 10);
                ui.add_space(8.0);
                ui::section_title(ui, "Post Response");
                ui::code_editor(ui, &mut d.script_res, 10);
            }
            SetSub::Tests => {
                ui::code_editor(ui, &mut d.tests, 18);
            }
            SetSub::Docs => {
                ui::code_editor(ui, &mut d.docs, 18);
            }
        }
    });
}

// ---------- runner ----------

/// Returns true when "Run" was clicked.
fn runner_view(ui: &mut egui::Ui, r: &mut RunnerTab, col: Option<&Collection>) -> bool {
    let mut run = false;
    let running = r.rx.is_some();
    egui::Panel::right("runner-list").resizable(true).default_size(ui.available_width() * 0.5).show(ui, |ui| {
        if r.results.is_empty() {
            let selected = r.items.iter().filter(|i| i.4).count();
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{selected} of {} selected", r.items.len())).color(MUTED));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui::link_button(ui, "Select All").clicked() {
                        r.items.iter_mut().for_each(|i| i.4 = true);
                    }
                    if ui::link_button(ui, "Deselect All").clicked() {
                        r.items.iter_mut().for_each(|i| i.4 = false);
                    }
                });
            });
            egui::ScrollArea::vertical().id_salt("runner-items").show(ui, |ui| {
                for (path, name, method, tags, on) in &mut r.items {
                    ui.horizontal(|ui| {
                        ui.checkbox(on, "");
                        ui.label(ui::method_tag(method, "http"));
                        ui.label(name.as_str());
                        let rel = col.and_then(|c| path.parent()?.strip_prefix(&c.root).ok()).map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
                        ui.label(RichText::new(rel).color(MUTED));
                        if !tags.is_empty() {
                            ui.label(RichText::new(tags.join(", ")).color(BRAND).size(11.0));
                        }
                    });
                }
            });
            return;
        }
        let (mut passed, mut failed, mut skipped) = (0, 0, 0);
        for row in &r.results {
            match row_state(row) {
                RowState::Passed => passed += 1,
                RowState::Failed => failed += 1,
                RowState::Skipped => skipped += 1,
            }
        }
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("Total Requests: {}", r.results.len())).strong());
            ui.label(RichText::new(format!("Passed: {passed}")).color(GREEN));
            ui.label(RichText::new(format!("Failed: {failed}")).color(RED));
            ui.label(RichText::new(format!("Skipped: {skipped}")).color(MUTED));
        });
        ui.separator();
        egui::ScrollArea::vertical().id_salt("runner-results").auto_shrink([false, false]).show(ui, |ui| {
            for (i, row) in r.results.iter().enumerate() {
                let o = row.outcome.as_ref();
                let (icon, color) = match row_state(row) {
                    RowState::Passed => ("✔", GREEN),
                    RowState::Failed => ("✖", RED),
                    RowState::Skipped => ("⤼", MUTED),
                };
                let resp = ui
                    .horizontal(|ui| {
                        ui.label(RichText::new(icon).color(color).strong());
                        ui.label(ui::method_tag(&row.method, "http"));
                        ui.label(&row.name);
                        if let Some(res) = o.and_then(|o| o.response.as_ref()) {
                            ui.label(RichText::new(format!("{} {}", res.status, res.status_text)).color(ui::status_color(res.status)));
                            ui.label(RichText::new(format!("{}ms", res.time_ms)).color(MUTED));
                        }
                        if row.iteration > 1 {
                            ui.label(RichText::new(format!("#{}", row.iteration)).color(MUTED));
                        }
                    })
                    .response
                    .interact(egui::Sense::click());
                if resp.clicked() {
                    r.picked = Some(i);
                }
                if let Some(o) = o.filter(|_| r.picked == Some(i)) {
                    egui::Frame::new().fill(Color32::from_rgb(250, 250, 250)).inner_margin(egui::Margin::same(8)).show(ui, |ui| {
                        if let Some(e) = &o.error {
                            ui.colored_label(RED, e);
                        }
                        for t in &o.tests {
                            if t.passed { ui.colored_label(GREEN, format!("✔ {}", t.name)) } else { ui.colored_label(RED, format!("✖ {} — {}", t.name, t.error)) };
                        }
                        for (n, a) in &o.asserts {
                            match a {
                                Ok(()) => ui.colored_label(GREEN, format!("✔ {n}")),
                                Err(e) => ui.colored_label(RED, format!("✖ {n} — {e}")),
                            };
                        }
                        if let Some(res) = &o.response {
                            ui::code_view(ui, &pretty_body(&res.body[..res.body.len().min(64 * 1024)]));
                        }
                    });
                }
            }
        });
    });
    egui::CentralPanel::no_frame().show(ui, |ui| {
        ui.label(RichText::new("🏃 Runner").strong().size(15.0));
        ui.label(format!("You have {} requests in this {}.", r.items.len(), if col.is_some_and(|c| c.root == r.dir) { "collection" } else { "folder" }));
        ui.add_space(10.0);
        ui.label(RichText::new("Timings").strong());
        ui.label("Delay between requests (ms)");
        ui.add(TextEdit::singleline(&mut r.delay).hint_text("e.g. 5").desired_width(f32::INFINITY));
        ui.add_space(8.0);
        ui.label(RichText::new("Filters").strong());
        ui.columns(2, |cols| {
            cols[0].label("Include tags");
            cols[0].add(TextEdit::singleline(&mut r.include_tags).hint_text("e.g., smoke, regression"));
            cols[1].label("Exclude tags");
            cols[1].add(TextEdit::singleline(&mut r.exclude_tags).hint_text("e.g., smoke, regression"));
        });
        ui.add_space(8.0);
        ui.label("Iterations");
        ui.add(TextEdit::singleline(&mut r.iterations).desired_width(200.0));
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            let n = r.items.iter().filter(|i| i.4).count();
            if running {
                ui.spinner();
                if ui.button("Stop").clicked() {
                    r.stop.store(true, Ordering::Relaxed);
                }
            } else if ui.add_enabled(n > 0, egui::Button::new(RichText::new(format!("Run {n} Requests")).color(Color32::WHITE).strong()).fill(BRAND)).clicked() {
                run = true;
            }
            if !running && !r.results.is_empty() && ui::link_button(ui, "Reset").clicked() {
                r.results.clear();
                r.picked = None;
            }
        });
    });
    run
}

enum RowState {
    Passed,
    Failed,
    Skipped,
}

fn row_state(row: &RunRow) -> RowState {
    match &row.outcome {
        Some(o) if o.skipped => RowState::Skipped,
        Some(o) if o.error.is_none() && o.tests.iter().all(|t| t.passed) && o.asserts.iter().all(|a| a.1.is_ok()) && o.response.as_ref().is_some_and(|r| r.status < 400) => RowState::Passed,
        _ => RowState::Failed,
    }
}

impl eframe::App for ZenApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll(&ctx);

        // shortcuts
        let (send, save, close, envs) = ui.input_mut(|i| {
            (
                i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Enter)),
                i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::S)),
                i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::W)),
                i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::E)),
            )
        });
        if send {
            self.send_active(&ctx);
        }
        if save {
            self.save_tab(self.active);
        }
        if close && !self.tabs.is_empty() {
            self.request_close_tab(self.active);
        }
        if envs && let Some(root) = self.current_root.clone() {
            let sel = self.env_sel.get(&root).copied().unwrap_or(0);
            self.env_window = Some((root, sel));
        }
        if ctx.input(|i| i.viewport().close_requested()) && !self.quitting && self.tabs.iter().any(Tab::dirty) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.dialog = Some(Dialog::Quit);
        }

        egui::Panel::bottom("statusbar").frame(egui::Frame::new().fill(SIDEBAR).inner_margin(egui::Margin::symmetric(10, 3))).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&self.status).size(11.5).color(Color32::from_rgb(110, 110, 110)));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(concat!("ZenBruno v", env!("CARGO_PKG_VERSION"))).size(11.0).color(MUTED));
                });
            });
        });
        egui::Panel::left("sidebar").resizable(true).default_size(270.0).min_size(180.0).frame(egui::Frame::new().fill(SIDEBAR).inner_margin(egui::Margin::symmetric(8, 4))).show(ui, |ui| self.sidebar(ui));
        egui::CentralPanel::default_margins().show(ui, |ui| self.main_area(ui));

        // deferred requests from nested widgets
        let grpc_load: Option<(PathBuf, bool)> = ctx.data_mut(|d| d.remove_temp(egui::Id::unique("grpc-load")));
        if let Some((_, reflection)) = grpc_load {
            self.grpc_load(&ctx, reflection);
        }
        let ws_send: Option<(PathBuf, usize)> = ctx.data_mut(|d| d.remove_temp(egui::Id::unique("ws-send")));
        if let Some((_, idx)) = ws_send {
            self.ws_send(idx);
        }
        let gen_code: Option<PathBuf> = ctx.data_mut(|d| d.remove_temp(egui::Id::unique("gen-code")));
        if let Some(path) = gen_code {
            self.generate_code(&path);
        }
        let load: Option<PathBuf> = ctx.data_mut(|d| d.remove_temp(egui::Id::unique("load-test")));
        if let (Some(path), Some(root)) = (load, self.tabs.get(self.active).map(|t| t.root.clone())) {
            self.open_load(&root, &path);
        }
        self.apply_acts();
        self.dialogs(&ctx);
        self.env_window(&ctx);
        self.code_window(&ctx);
    }
}

impl ZenApp {
    fn generate_code(&mut self, path: &Path) {
        let Some(root) = self.tabs.get(self.active).map(|t| t.root.clone()) else { return };
        let r = self.current_request(path).and_then(|req| {
            let job = self.job_for(&root, path, &req).ok_or("collection not open".to_string())?;
            exec::prepare(&job).map(|p| (req.name, crate::curl::to_curl(&p)))
        });
        match r {
            Ok((name, code)) => self.code_window = Some((format!("Generate code · {name}"), code)),
            Err(e) => self.status = format!("Cannot generate code: {e}"),
        }
    }

    fn code_window(&mut self, ctx: &egui::Context) {
        let Some((title, code)) = self.code_window.clone() else { return };
        let mut open = true;
        egui::Window::new("Generate Code")
            .id(egui::Id::unique("code-window"))
            .collapsible(false)
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .min_width(640.0)
            .open(&mut open)
            .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&title).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui::primary_button(ui, "Copy").clicked() {
                        ui.ctx().copy_text(code.clone());
                        self.status = "cURL command copied to clipboard".into();
                    }
                    ui.label(RichText::new("Shell: bash").color(MUTED));
                });
            });
            ui.label(RichText::new("Variables are resolved with the selected environment; scripts are not run.").color(MUTED).size(12.0));
            egui::ScrollArea::vertical().show(ui, |ui| ui::code_view(ui, &code));
        });
        if !open {
            self.code_window = None;
        }
    }
}

// ---------- load test ----------

const SERIES_1: Color32 = Color32::from_rgb(0x2a, 0x78, 0xd6);
const SERIES_2: Color32 = Color32::from_rgb(0xeb, 0x68, 0x34);
const GRID: Color32 = Color32::from_rgb(236, 236, 236);

fn nice_max(v: f64) -> f64 {
    if v <= 0.0 {
        return 1.0;
    }
    let mag = 10f64.powf(v.log10().floor());
    let n = v / mag;
    mag * if n <= 1.0 { 1.0 } else if n <= 2.0 { 2.0 } else if n <= 5.0 { 5.0 } else { 10.0 }
}

fn fmt_num(v: f64) -> String {
    if v >= 1000.0 { format!("{:.1}k", v / 1000.0) } else if v >= 100.0 || v.fract() == 0.0 { format!("{v:.0}") } else if v < 1.0 { format!("{v:.2}").trim_end_matches('0').to_string() } else { format!("{v:.1}") }
}

/// Plot frame with recessive gridlines + y labels; returns the plot rect and y scale max.
fn plot_frame(ui: &mut egui::Ui, width: f32, height: f32, max: f64, unit: &str) -> (egui::Rect, f64, egui::Response) {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    let plot = egui::Rect::from_min_max(rect.min + egui::vec2(46.0, 6.0), rect.max - egui::vec2(8.0, 20.0));
    let ymax = nice_max(max);
    let painter = ui.painter();
    for i in 0..=4 {
        let v = ymax * i as f64 / 4.0;
        let y = plot.bottom() - plot.height() * (i as f32 / 4.0);
        painter.hline(plot.x_range(), y, Stroke::new(1.0, GRID));
        painter.text(egui::pos2(plot.left() - 6.0, y), egui::Align2::RIGHT_CENTER, format!("{}{unit}", fmt_num(v)), egui::FontId::proportional(10.5), MUTED);
    }
    (plot, ymax, resp)
}

fn x_labels(ui: &egui::Ui, plot: egui::Rect, n: usize) {
    let step = ((n as f32 / 8.0).ceil() as usize).max(1);
    for s in (0..n).step_by(step) {
        let x = plot.left() + plot.width() * (s as f32 + 0.5) / n as f32;
        ui.painter().text(egui::pos2(x, plot.bottom() + 4.0), egui::Align2::CENTER_TOP, format!("{s}s"), egui::FontId::proportional(10.5), MUTED);
    }
}

/// Requests per second: stacked bars, OK (series blue) under failed (status red), 2px gaps, rounded tops.
fn throughput_chart(ui: &mut egui::Ui, width: f32, seconds: &[loadtest::Second]) {
    let n = seconds.len().max(1);
    let max = seconds.iter().map(|s| (s.ok + s.failed) as f64).fold(0.0, f64::max);
    let (plot, ymax, resp) = plot_frame(ui, width, 170.0, max, "");
    let slot = plot.width() / n as f32;
    let bar_w = (slot - 2.0).clamp(1.0, 28.0);
    let y = |v: f64| plot.bottom() - (v / ymax) as f32 * plot.height();
    let hovered = resp.hover_pos().filter(|p| plot.contains(*p)).map(|p| (((p.x - plot.left()) / slot) as usize).min(n - 1));
    for (i, s) in seconds.iter().enumerate() {
        let cx = plot.left() + slot * (i as f32 + 0.5);
        let x0 = cx - bar_w / 2.0;
        let ok_top = y(s.ok as f64);
        let all_top = y((s.ok + s.failed) as f64);
        let r = (bar_w / 2.0).min(4.0) as u8;
        let dim = |c: Color32| if hovered.is_some_and(|h| h != i) { c.gamma_multiply(0.55) } else { c };
        if s.failed > 0 {
            ui.painter().rect_filled(egui::Rect::from_min_max(egui::pos2(x0, all_top), egui::pos2(x0 + bar_w, ok_top - if s.ok > 0 { 2.0 } else { 0.0 })), CornerRadius { nw: r, ne: r, sw: 0, se: 0 }, dim(RED));
        }
        if s.ok > 0 {
            let top_r = if s.failed > 0 { 0 } else { r };
            ui.painter().rect_filled(egui::Rect::from_min_max(egui::pos2(x0, ok_top), egui::pos2(x0 + bar_w, plot.bottom())), CornerRadius { nw: top_r, ne: top_r, sw: 0, se: 0 }, dim(SERIES_1));
        }
    }
    x_labels(ui, plot, n);
    if let Some(i) = hovered
        && let Some(s) = seconds.get(i)
    {
        resp.on_hover_ui_at_pointer(|ui| {
            ui.label(RichText::new(format!("{i}s – {}s", i + 1)).strong());
            ui.label(format!("{} requests", s.ok + s.failed));
            ui.label(RichText::new(format!("■ OK  {}", s.ok)).color(SERIES_1));
            if s.failed > 0 {
                ui.label(RichText::new(format!("× Failed  {}", s.failed)).color(RED));
            }
        });
    }
}

/// Latency per second: avg and p95 lines (2px), direct end labels, crosshair tooltip.
fn latency_chart(ui: &mut egui::Ui, width: f32, seconds: &[loadtest::Second]) {
    let pts: Vec<Option<(f64, f64)>> = seconds.iter().map(loadtest::second_latency).collect();
    let n = pts.len().max(1);
    let max = pts.iter().flatten().map(|p| p.0.max(p.1)).fold(0.0, f64::max);
    let (plot, ymax, resp) = plot_frame(ui, width, 170.0, max, "ms");
    let x = |i: usize| plot.left() + plot.width() * (i as f32 + 0.5) / n as f32;
    let y = |v: f64| plot.bottom() - (v / ymax) as f32 * plot.height();
    for (series, color, pick) in [("avg", SERIES_1, 0usize), ("p95", SERIES_2, 1)] {
        let line: Vec<egui::Pos2> = pts.iter().enumerate().filter_map(|(i, p)| p.map(|p| egui::pos2(x(i), y(if pick == 0 { p.0 } else { p.1 })))).collect();
        if line.len() > 1 {
            ui.painter().add(egui::Shape::line(line.clone(), Stroke::new(2.0, color)));
        } else if let Some(p) = line.first() {
            ui.painter().circle_filled(*p, 4.0, color);
        }
        if let Some(last) = line.last() {
            // direct label at the line end, in text ink with a color key
            ui.painter().circle_filled(*last, 3.0, color);
            // p95 label sits above its line end, avg below, so close lines never collide
            let above = pick == 1;
            let right = last.x > plot.right() - 40.0;
            let anchor = match (right, above) { (true, true) => egui::Align2::RIGHT_BOTTOM, (true, false) => egui::Align2::RIGHT_TOP, (false, true) => egui::Align2::LEFT_BOTTOM, (false, false) => egui::Align2::LEFT_TOP };
            ui.painter().text(*last + egui::vec2(if right { -6.0 } else { 6.0 }, if above { -4.0 } else { 4.0 }), anchor, series, egui::FontId::proportional(11.0), TEXT);
        }
    }
    x_labels(ui, plot, n);
    if let Some(p) = resp.hover_pos().filter(|p| plot.contains(*p)) {
        let i = (((p.x - plot.left()) / plot.width() * n as f32) as usize).min(n - 1);
        ui.painter().vline(x(i), plot.y_range(), Stroke::new(1.0, Color32::from_rgb(190, 190, 190)));
        if let Some(Some((avg, p95))) = pts.get(i) {
            for (v, c) in [(*avg, SERIES_1), (*p95, SERIES_2)] {
                ui.painter().circle(egui::pos2(x(i), y(v)), 4.0, c, Stroke::new(2.0, Color32::WHITE));
            }
            let (avg, p95) = (*avg, *p95);
            resp.on_hover_ui_at_pointer(|ui| {
                ui.label(RichText::new(format!("{i}s – {}s", i + 1)).strong());
                ui.label(RichText::new(format!("━ avg  {avg:.1} ms")).color(SERIES_1));
                ui.label(RichText::new(format!("━ p95  {p95:.1} ms")).color(SERIES_2));
            });
        }
    }
}

fn stat_tile(ui: &mut egui::Ui, width: f32, label: &str, value: String, note: Option<RichText>) {
    egui::Frame::new().stroke(Stroke::new(1.0, BORDER)).corner_radius(CornerRadius::same(6)).inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
        ui.set_width(width);
        ui.vertical(|ui| {
            ui.label(RichText::new(label).color(MUTED).size(11.5));
            ui.label(RichText::new(value).strong().size(19.0));
            if let Some(n) = note {
                ui.label(n);
            }
        });
    });
}

fn legend_item(ui: &mut egui::Ui, color: Color32, text: &str) {
    let (r, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
    ui.painter().rect_filled(r, 2.0, color);
    ui.label(RichText::new(text).size(12.0));
}

/// Returns true when "Start" was clicked.
fn load_view(ui: &mut egui::Ui, l: &mut LoadTab) -> bool {
    let mut start = false;
    let running = l.rx.is_some();
    let height = ui.available_height();
    let total_width = ui.available_width();
    ui.horizontal_top(|ui| {
    ui.allocate_ui_with_layout(egui::vec2(270.0, height), egui::Layout::top_down(egui::Align::Min), |ui| {
        ui.set_width(270.0);
        ui.label(RichText::new("⚡ Load Test").strong().size(15.0));
        ui.horizontal(|ui| {
            ui.label(ui::method_tag(&l.method, "http"));
            ui.label(&l.name);
        });
        ui.add_space(10.0);
        egui::Grid::new("load-cfg").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
            for (label, value, hint) in [
                ("Virtual users", &mut l.users, "concurrent users"),
                ("Duration (s)", &mut l.duration, "seconds"),
                ("Max requests", &mut l.max_requests, "0 = no limit"),
                ("Ramp-up (s)", &mut l.ramp_up, "0 = all at once"),
                ("Think time (ms)", &mut l.think, "pause per user"),
            ] {
                ui.label(label);
                ui.add_enabled(!running, TextEdit::singleline(value).hint_text(hint).desired_width(110.0));
                ui.end_row();
            }
        });
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            if running {
                ui.spinner();
                if ui.button("Stop").clicked() {
                    l.stop.store(true, Ordering::Relaxed);
                }
            } else if ui::primary_button(ui, "Start load test").clicked() {
                start = true;
            }
        });
        ui.add_space(10.0);
        ui.label(RichText::new("The request is prepared once with the selected environment and current runtime variables, then replayed by every user over shared keep-alive connections. Scripts, vars and assertions are not re-run per request. A request fails on a network error or an HTTP status ≥ 400.").color(MUTED).size(11.5));
        if let Some(e) = &l.error {
            ui.add_space(6.0);
            ui.colored_label(RED, e);
        }
    });
    ui.separator();
    let width = (total_width - 270.0 - 24.0).max(300.0);
    ui.allocate_ui_with_layout(egui::vec2(width, height), egui::Layout::top_down(egui::Align::Min), |ui| {
        ui.set_max_width(width);
        if l.stats.total == 0 && !running {
            ui.add_space(40.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("Configure the test on the left and press Start.").color(MUTED));
            });
            return;
        }
        egui::ScrollArea::vertical().id_salt("load-scroll").auto_shrink([false, false]).show(ui, |ui| {
            // the scroll area otherwise inherits the full horizontal layout width
            ui.set_width(width - 14.0);
            if !l.target.is_empty() {
                ui.add(egui::Label::new(RichText::new(&l.target).monospace().color(Color32::from_rgb(90, 90, 90))).selectable(false));
            }
            let st = &mut l.stats;
            let elapsed = l.elapsed;
            let tile_w = ((width - 20.0) / 4.0 - 34.0).max(90.0);
            let rate = st.error_rate();
            let note = if st.failed == 0 { RichText::new("no failures").color(GREEN).size(11.0) } else { RichText::new(format!("× {} failed", st.failed)).color(RED).size(11.0) };
            let tiles = [
                ("Requests", fmt_num(st.total as f64), Some(RichText::new(format!("{:.1}s elapsed", elapsed.as_secs_f64())).color(MUTED).size(11.0))),
                ("Throughput", format!("{}/s", fmt_num(st.rps(elapsed))), None),
                ("Avg latency", format!("{:.1} ms", st.avg_ms()), None),
                ("Error rate", format!("{rate:.1}%"), Some(note)),
                ("p50", format!("{:.1} ms", st.percentile_ms(50.0)), None),
                ("p95", format!("{:.1} ms", st.percentile_ms(95.0)), None),
                ("p99", format!("{:.1} ms", st.percentile_ms(99.0)), Some(RichText::new(format!("max {:.1} ms", st.max_ms())).color(MUTED).size(11.0))),
                ("Received", ui::human_size(st.bytes as usize), None),
            ];
            egui::Grid::new("load-tiles").num_columns(4).spacing([8.0, 8.0]).show(ui, |ui| {
                for (i, (label, value, note)) in tiles.into_iter().enumerate() {
                    stat_tile(ui, tile_w, label, value, note);
                    if i % 4 == 3 {
                        ui.end_row();
                    }
                }
            });
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Requests per second").strong());
                ui.add_space(12.0);
                legend_item(ui, SERIES_1, "OK");
                legend_item(ui, RED, "× Failed");
            });
            throughput_chart(ui, width - 20.0, &st.seconds);
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Response time").strong());
                ui.add_space(12.0);
                legend_item(ui, SERIES_1, "avg");
                legend_item(ui, SERIES_2, "p95");
            });
            latency_chart(ui, width - 20.0, &st.seconds);
            ui.add_space(10.0);
            ui.columns(2, |cols| {
                cols[0].label(RichText::new("Status codes").strong());
                egui::Grid::new("load-status").striped(true).num_columns(3).show(&mut cols[0], |ui| {
                    for (code, count) in &st.statuses {
                        ui.label(RichText::new(code.to_string()).color(ui::status_color(*code)).strong());
                        ui.label(fmt_num(*count as f64));
                        ui.label(RichText::new(format!("{:.1}%", *count as f64 * 100.0 / st.total.max(1) as f64)).color(MUTED));
                        ui.end_row();
                    }
                });
                cols[1].label(RichText::new("Errors").strong());
                if st.errors.is_empty() {
                    cols[1].label(RichText::new("None").color(MUTED));
                }
                for (e, count) in &st.errors {
                    cols[1].label(RichText::new(format!("✖ {count} × {e}")).color(RED).size(12.0));
                }
            });
            ui.add_space(10.0);
            if !running && ui::link_button(ui, "Copy summary").clicked() {
                let (p50, p95, p99) = (st.percentile_ms(50.0), st.percentile_ms(95.0), st.percentile_ms(99.0));
                let summary = format!(
                    "Load test: {}\nUsers/duration: {} / {:.1}s\nRequests: {} ({:.1}/s), failed {} ({:.1}%)\nLatency ms: avg {:.1}, p50 {:.1}, p95 {:.1}, p99 {:.1}, max {:.1}\nStatus codes: {}\n",
                    l.target,
                    l.users,
                    elapsed.as_secs_f64(),
                    st.total,
                    st.rps(elapsed),
                    st.failed,
                    st.error_rate(),
                    st.avg_ms(),
                    p50,
                    p95,
                    p99,
                    st.max_ms(),
                    st.statuses.iter().map(|(c, n)| format!("{c}×{n}")).collect::<Vec<_>>().join(", ")
                );
                ui.ctx().copy_text(summary);
            }
        });
    });
    });
    start
}

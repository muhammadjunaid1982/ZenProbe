//! Format-independent request / folder / environment model. `bru.rs` and `yml.rs` map files onto it.

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pair {
    pub key: String,
    pub value: String,
    pub enabled: bool,
    pub description: String,
    /// Raw non-description annotation lines from .bru files (e.g. `@number`), kept for round-trip.
    pub annotations: Vec<String>,
    /// `key: [ ... ]` list values in .bru dictionaries (meta tags, settings omitHeaders).
    pub list: Option<Vec<String>>,
    /// Multipart parts: file part whose `value` holds `|`-separated paths.
    pub is_file: bool,
    /// Multipart parts and body:file entries.
    pub content_type: String,
}

impl Pair {
    pub fn new(key: &str, value: &str) -> Self {
        Pair { key: key.into(), value: value.into(), enabled: true, ..Default::default() }
    }
}

/// gRPC or WebSocket message.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Message {
    pub name: String,
    /// WebSocket payload type: text | json | xml | binary
    pub kind: String,
    pub content: String,
    pub selected: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Format {
    #[default]
    Bru,
    Yaml,
}

impl Format {
    pub fn ext(self) -> &'static str {
        match self {
            Format::Bru => "bru",
            Format::Yaml => "yml",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Auth {
    /// inherit | none | basic | bearer | (other modes are preserved but not applied)
    pub mode: String,
    pub username: String,
    pub password: String,
    pub token: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Request {
    pub name: String,
    /// http | graphql | grpc | ws
    pub kind: String,
    pub seq: String,
    pub tags: Vec<String>,
    pub method: String,
    pub url: String,
    pub body_mode: String,
    pub auth: Auth,
    pub query: Vec<Pair>,
    pub path_params: Vec<Pair>,
    /// HTTP/WS headers, or gRPC metadata.
    pub headers: Vec<Pair>,
    pub body_json: String,
    pub body_text: String,
    pub body_xml: String,
    pub body_sparql: String,
    pub form: Vec<Pair>,
    pub multipart: Vec<Pair>,
    /// body:file entries: value = path, enabled = selected
    pub files: Vec<Pair>,
    pub gql_query: String,
    pub gql_vars: String,
    pub grpc_method_type: String,
    pub proto_path: String,
    pub messages: Vec<Message>,
    pub vars_req: Vec<Pair>,
    pub vars_res: Vec<Pair>,
    pub asserts: Vec<Pair>,
    pub script_req: String,
    pub script_res: String,
    pub tests: String,
    pub docs: String,
    /// timeout, followRedirects, maxRedirects, encodeUrl, keepAliveInterval, ...
    pub settings: Vec<Pair>,
    /// Format-specific data ZenProbe does not model, kept so saving loses nothing.
    pub keep: Keep,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Keep {
    pub bru_meta: Vec<Pair>,
    pub bru_blocks: Vec<crate::bru::Block>,
    pub yaml: Option<serde_yaml::Value>,
}

impl Request {
    pub fn new(name: &str, seq: usize, kind: &str) -> Self {
        let mut r = Request {
            name: name.into(),
            kind: kind.into(),
            seq: seq.to_string(),
            method: match kind {
                "graphql" => "post",
                "http" => "get",
                _ => "",
            }
            .into(),
            body_mode: match kind {
                "graphql" => "graphql",
                "grpc" => "grpc",
                "ws" => "ws",
                _ => "none",
            }
            .into(),
            auth: Auth { mode: "inherit".into(), ..Default::default() },
            ..Default::default()
        };
        if kind == "grpc" || kind == "ws" {
            let kind = if kind == "ws" { "json" } else { "" };
            r.messages.push(Message { name: "message 1".into(), kind: kind.into(), content: "{}".into(), selected: kind == "json" });
        }
        r
    }

    pub fn setting(&self, key: &str) -> Option<&str> {
        self.settings.iter().find(|p| p.key == key).map(|p| p.value.as_str())
    }

    pub fn set_setting(&mut self, key: &str, value: String) {
        match self.settings.iter_mut().find(|p| p.key == key) {
            Some(p) => p.value = value,
            None => self.settings.push(Pair::new(key, &value)),
        }
    }
}

/// collection.bru / folder.bru / opencollection.yml `request` / folder.yml
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FolderSettings {
    pub name: String,
    pub seq: String,
    pub headers: Vec<Pair>,
    pub auth: Auth,
    pub vars_req: Vec<Pair>,
    pub vars_res: Vec<Pair>,
    pub script_req: String,
    pub script_res: String,
    pub tests: String,
    pub docs: String,
    pub keep: Keep,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Environment {
    pub name: String,
    pub path: std::path::PathBuf,
    pub vars: Vec<Pair>,
    /// Names of secret variables; their values live encrypted in Bruno's own app storage.
    pub secrets: Vec<String>,
    pub keep: Keep,
}

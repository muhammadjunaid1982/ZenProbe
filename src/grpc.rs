//! gRPC client: methods from a .proto file (compiled with protox) or server reflection,
//! JSON messages encoded through prost-reflect, unary and all streaming call types.

use prost::Message as _;
use prost_reflect::{DescriptorPool, DynamicMessage, MessageDescriptor, SerializeOptions};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use tonic::codec::{Codec, DecodeBuf, Decoder, EncodeBuf, Encoder};
use tonic::metadata::{MetadataKey, MetadataMap, MetadataValue};
use tonic::transport::{Channel, ClientTlsConfig, Endpoint};
use tonic::{Request, Status};

#[derive(Clone, Debug, PartialEq)]
pub struct MethodInfo {
    /// `/package.Service/Method`
    pub path: String,
    /// unary | client-streaming | server-streaming | bidi-streaming
    pub kind: &'static str,
}

#[derive(Debug, Clone)]
pub enum Event {
    Headers(Vec<(String, String)>),
    Message(String),
    Trailers(Vec<(String, String)>),
    /// Final status: code name, numeric code, message.
    Status(String, i32, String),
    Error(String),
}

pub fn from_proto(proto: &Path, include_dirs: &[PathBuf]) -> Result<DescriptorPool, String> {
    let mut includes: Vec<PathBuf> = proto.parent().map(Path::to_path_buf).into_iter().collect();
    includes.extend(include_dirs.iter().cloned());
    let file = proto.file_name().ok_or("invalid proto path")?;
    let fds = protox::compile([file], includes).map_err(|e| e.to_string())?;
    DescriptorPool::from_file_descriptor_set(fds).map_err(|e| e.to_string())
}

pub fn methods(pool: &DescriptorPool) -> Vec<MethodInfo> {
    let mut out: Vec<MethodInfo> = pool
        .services()
        .flat_map(|s| s.methods().collect::<Vec<_>>())
        .filter(|m| !m.parent_service().full_name().starts_with("grpc.reflection."))
        .map(|m| MethodInfo {
            path: format!("/{}/{}", m.parent_service().full_name(), m.name()),
            kind: match (m.is_client_streaming(), m.is_server_streaming()) {
                (true, true) => "bidi-streaming",
                (true, false) => "client-streaming",
                (false, true) => "server-streaming",
                _ => "unary",
            },
        })
        .collect();
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

fn find_method(pool: &DescriptorPool, path: &str) -> Result<prost_reflect::MethodDescriptor, String> {
    let (svc, name) = path.trim_start_matches('/').split_once('/').ok_or(format!("invalid method `{path}`"))?;
    pool.get_service_by_name(svc).and_then(|s| s.methods().find(|m| m.name() == name)).ok_or(format!("method {path} not found; reload methods"))
}

/// JSON template with every field at its default, like Bruno's "generate sample message".
pub fn sample_message(pool: &DescriptorPool, path: &str) -> Result<String, String> {
    let m = find_method(pool, path)?;
    let msg = DynamicMessage::new(m.input());
    let mut ser = serde_json::Serializer::pretty(Vec::new());
    msg.serialize_with_options(&mut ser, &SerializeOptions::new().skip_default_fields(false)).map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&ser.into_inner()).into_owned())
}

/// grpc:// → http, grpcs:// → https; without a scheme, localhost is plaintext (Bruno's rule).
pub fn normalize_url(url: &str) -> (String, bool) {
    let url = url.trim().trim_end_matches('/');
    let (tls, rest) = if let Some(r) = url.strip_prefix("grpcs://").or_else(|| url.strip_prefix("https://")) {
        (true, r)
    } else if let Some(r) = url.strip_prefix("grpc://").or_else(|| url.strip_prefix("http://")) {
        (false, r)
    } else {
        (!(url.contains("localhost") || url.contains("127.0.0.1")), url)
    };
    // gRPC uses only the authority; any path is ignored
    let host = rest.split('/').next().unwrap_or(rest);
    (format!("{}://{host}", if tls { "https" } else { "http" }), tls)
}

async fn connect(url: &str) -> Result<Channel, String> {
    let (endpoint, tls) = normalize_url(url);
    let mut ep = Endpoint::from_shared(endpoint).map_err(|e| e.to_string())?.connect_timeout(std::time::Duration::from_secs(10));
    if tls {
        ep = ep.tls_config(ClientTlsConfig::new().with_native_roots()).map_err(|e| e.to_string())?;
    }
    ep.connect().await.map_err(|e| {
        let mut msg = e.to_string();
        let mut src = std::error::Error::source(&e);
        while let Some(s) = src {
            msg = format!("{msg}: {s}");
            src = s.source();
        }
        msg
    })
}

fn apply_metadata(map: &mut MetadataMap, metadata: &[(String, String)]) -> Result<(), String> {
    for (k, v) in metadata {
        let key = MetadataKey::from_bytes(k.to_ascii_lowercase().as_bytes()).map_err(|e| format!("metadata `{k}`: {e}"))?;
        let value = MetadataValue::try_from(v.as_str()).map_err(|e| format!("metadata `{k}`: {e}"))?;
        map.insert(key, value);
    }
    Ok(())
}

fn metadata_pairs(map: &MetadataMap) -> Vec<(String, String)> {
    map.clone().into_headers().iter().map(|(k, v)| (k.to_string(), String::from_utf8_lossy(v.as_bytes()).into_owned())).collect()
}

fn runtime() -> Result<tokio::runtime::Runtime, String> {
    tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| e.to_string())
}

// ---------- dynamic codec ----------

#[derive(Clone)]
struct DynCodec(MessageDescriptor);
struct DynEncoder;
struct DynDecoder(MessageDescriptor);

impl Codec for DynCodec {
    type Encode = DynamicMessage;
    type Decode = DynamicMessage;
    type Encoder = DynEncoder;
    type Decoder = DynDecoder;
    fn encoder(&mut self) -> DynEncoder {
        DynEncoder
    }
    fn decoder(&mut self) -> DynDecoder {
        DynDecoder(self.0.clone())
    }
}

impl Encoder for DynEncoder {
    type Item = DynamicMessage;
    type Error = Status;
    fn encode(&mut self, item: DynamicMessage, dst: &mut EncodeBuf<'_>) -> Result<(), Status> {
        item.encode(dst).map_err(|e| Status::internal(e.to_string()))
    }
}

impl Decoder for DynDecoder {
    type Item = DynamicMessage;
    type Error = Status;
    fn decode(&mut self, src: &mut DecodeBuf<'_>) -> Result<Option<DynamicMessage>, Status> {
        DynamicMessage::decode(self.0.clone(), src).map(Some).map_err(|e| Status::internal(e.to_string()))
    }
}

fn to_json(msg: &DynamicMessage) -> String {
    serde_json::to_value(msg).and_then(|v| serde_json::to_string_pretty(&v)).unwrap_or_else(|e| e.to_string())
}

fn parse_message(desc: &MessageDescriptor, text: &str) -> Result<DynamicMessage, String> {
    let text = if text.trim().is_empty() { "{}" } else { text };
    let mut de = serde_json::Deserializer::from_str(text);
    let msg = DynamicMessage::deserialize(desc.clone(), &mut de).map_err(|e| format!("message does not match {}: {e}", desc.full_name()))?;
    de.end().map_err(|e| e.to_string())?;
    Ok(msg)
}

fn status_event(s: &Status) -> Event {
    Event::Status(format!("{:?}", s.code()), s.code() as i32, s.message().to_string())
}

/// Invoke a method, sending all `messages` (JSON) and streaming results to `tx`. Blocking.
pub fn call(url: &str, pool: &DescriptorPool, path: &str, messages: &[String], metadata: &[(String, String)], tx: &Sender<Event>) {
    let send = |e: Event| {
        let _ = tx.send(e);
    };
    let run = async {
        let method = find_method(pool, path)?;
        let msgs = messages.iter().map(|m| parse_message(&method.input(), m)).collect::<Result<Vec<_>, _>>()?;
        let channel = connect(url).await?;
        let mut client = tonic::client::Grpc::new(channel);
        client.ready().await.map_err(|e| e.to_string())?;
        let grpc_path = tonic::codegen::http::uri::PathAndQuery::try_from(path).map_err(|e| e.to_string())?;
        let codec = DynCodec(method.output());
        let client_stream = method.is_client_streaming();
        let server_stream = method.is_server_streaming();

        let outcome: Result<(), Status> = async {
            if !client_stream {
                let first = msgs.first().cloned().unwrap_or_else(|| DynamicMessage::new(method.input()));
                let mut req = Request::new(first);
                apply_metadata(req.metadata_mut(), metadata).map_err(Status::invalid_argument)?;
                if server_stream {
                    let resp = client.server_streaming(req, grpc_path, codec).await?;
                    send(Event::Headers(metadata_pairs(resp.metadata())));
                    let mut stream = resp.into_inner();
                    while let Some(m) = stream.message().await? {
                        send(Event::Message(to_json(&m)));
                    }
                    if let Some(t) = stream.trailers().await? {
                        send(Event::Trailers(metadata_pairs(&t)));
                    }
                } else {
                    let resp = client.unary(req, grpc_path, codec).await?;
                    send(Event::Headers(metadata_pairs(resp.metadata())));
                    send(Event::Message(to_json(resp.get_ref())));
                }
            } else {
                let mut req = Request::new(tokio_stream::iter(msgs.clone()));
                apply_metadata(req.metadata_mut(), metadata).map_err(Status::invalid_argument)?;
                if server_stream {
                    let resp = client.streaming(req, grpc_path, codec).await?;
                    send(Event::Headers(metadata_pairs(resp.metadata())));
                    let mut stream = resp.into_inner();
                    while let Some(m) = stream.message().await? {
                        send(Event::Message(to_json(&m)));
                    }
                    if let Some(t) = stream.trailers().await? {
                        send(Event::Trailers(metadata_pairs(&t)));
                    }
                } else {
                    let resp = client.client_streaming(req, grpc_path, codec).await?;
                    send(Event::Headers(metadata_pairs(resp.metadata())));
                    send(Event::Message(to_json(resp.get_ref())));
                }
            }
            Ok(())
        }
        .await;
        match outcome {
            Ok(()) => send(Event::Status("Ok".into(), 0, String::new())),
            Err(s) => send(status_event(&s)),
        }
        Ok::<(), String>(())
    };
    let result = runtime().and_then(|rt| rt.block_on(run));
    if let Err(e) = result {
        send(Event::Error(e));
    }
}

// ---------- server reflection ----------

macro_rules! reflect {
    ($ver:ident, $channel:expr) => {{
        use tonic_reflection::pb::$ver::{
            ServerReflectionRequest, server_reflection_client::ServerReflectionClient, server_reflection_request::MessageRequest,
            server_reflection_response::MessageResponse,
        };
        let mut client = ServerReflectionClient::new($channel.clone());
        let ask = |m: MessageRequest| ServerReflectionRequest { host: String::new(), message_request: Some(m) };
        let mut stream = client.server_reflection_info(tokio_stream::iter(vec![ask(MessageRequest::ListServices(String::new()))])).await.map_err(|e| e.to_string())?.into_inner();
        let services: Vec<String> = match stream.message().await.map_err(|e| e.to_string())?.and_then(|r| r.message_response) {
            Some(MessageResponse::ListServicesResponse(l)) => l.service.into_iter().map(|s| s.name).collect(),
            Some(MessageResponse::ErrorResponse(e)) => return Err(e.error_message),
            _ => return Err("unexpected reflection response".to_string()),
        };
        let requests: Vec<_> = services.iter().map(|s| ask(MessageRequest::FileContainingSymbol(s.clone()))).collect();
        let mut stream = client.server_reflection_info(tokio_stream::iter(requests)).await.map_err(|e| e.to_string())?.into_inner();
        let mut files = std::collections::BTreeMap::new();
        while let Some(r) = stream.message().await.map_err(|e| e.to_string())? {
            if let Some(MessageResponse::FileDescriptorResponse(f)) = r.message_response {
                for bytes in f.file_descriptor_proto {
                    let fd = prost_types::FileDescriptorProto::decode(bytes.as_slice()).map_err(|e| e.to_string())?;
                    files.insert(fd.name.clone().unwrap_or_default(), fd);
                }
            }
        }
        let mut pool = DescriptorPool::new();
        pool.add_file_descriptor_protos(files.into_values()).map_err(|e| e.to_string())?;
        Ok::<DescriptorPool, String>(pool)
    }};
}

pub fn from_reflection(url: &str) -> Result<DescriptorPool, String> {
    runtime()?.block_on(async {
        let channel = connect(url).await?;
        match reflect!(v1, channel) {
            Ok(pool) => Ok(pool),
            Err(_) => reflect!(v1alpha, channel),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls() {
        assert_eq!(normalize_url("localhost:50051"), ("http://localhost:50051".into(), false));
        assert_eq!(normalize_url("api.example.com:443/x"), ("https://api.example.com:443".into(), true));
        assert_eq!(normalize_url("grpc://h:1"), ("http://h:1".into(), false));
        assert_eq!(normalize_url("grpcs://h:1"), ("https://h:1".into(), true));
    }

    #[test]
    fn proto_file_methods_and_sample() {
        let dir = std::env::temp_dir().join(format!("zenbruno-proto-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let proto = dir.join("hello.proto");
        std::fs::write(&proto, "syntax = \"proto3\";\npackage demo;\nimport \"google/protobuf/timestamp.proto\";\nmessage Req { string name = 1; int32 n = 2; google.protobuf.Timestamp at = 3; }\nmessage Res { string msg = 1; }\nservice Greeter {\n  rpc Hello (Req) returns (Res);\n  rpc Chat (stream Req) returns (stream Res);\n}\n").unwrap();
        let pool = from_proto(&proto, &[]).unwrap();
        let m = methods(&pool);
        assert_eq!(m, vec![MethodInfo { path: "/demo.Greeter/Chat".into(), kind: "bidi-streaming" }, MethodInfo { path: "/demo.Greeter/Hello".into(), kind: "unary" }]);
        let sample = sample_message(&pool, "/demo.Greeter/Hello").unwrap();
        assert!(sample.contains("\"name\": \"\"") && sample.contains("\"n\": 0"), "{sample}");
        let desc = find_method(&pool, "/demo.Greeter/Hello").unwrap().input();
        assert!(parse_message(&desc, "{\"name\": \"x\", \"n\": 3}").is_ok());
        assert!(parse_message(&desc, "{\"nope\": 1}").is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// End-to-end against a real tonic server exposing the reflection service:
    /// loads methods by reflection, then calls the bidi reflection method dynamically.
    #[test]
    fn reflection_and_bidi_call_against_live_server() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
            rt.block_on(async {
                let svc = tonic_reflection::server::Builder::configure()
                    .register_encoded_file_descriptor_set(tonic_reflection::pb::v1::FILE_DESCRIPTOR_SET)
                    .build_v1()
                    .unwrap();
                tonic::transport::Server::builder().add_service(svc).serve(addr).await.unwrap();
            });
        });
        let url = format!("localhost:{}", addr.port());
        let mut pool = Err(String::new());
        for _ in 0..50 {
            pool = from_reflection(&url);
            if pool.is_ok() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        let pool = pool.unwrap();
        let path = "/grpc.reflection.v1.ServerReflection/ServerReflectionInfo";
        assert!(find_method(&pool, path).is_ok());
        let (tx, rx) = std::sync::mpsc::channel();
        call(&url, &pool, path, &["{\"listServices\": \"\"}".into()], &[("x-test".into(), "1".into())], &tx);
        drop(tx);
        let events: Vec<Event> = rx.iter().collect();
        assert!(events.iter().any(|e| matches!(e, Event::Message(m) if m.contains("grpc.reflection.v1.ServerReflection"))), "{events:?}");
        assert!(matches!(events.last(), Some(Event::Status(_, 0, _))), "{events:?}");
    }
}

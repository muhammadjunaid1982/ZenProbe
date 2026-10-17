//! WebSocket client on a background thread: connect with headers, send/receive, close.

use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::time::{Duration, Instant};
use tungstenite::client::IntoClientRequest;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

#[derive(Debug, Clone)]
pub enum Event {
    Opened { status: u16 },
    Received(String),
    Sent(String),
    Closed(String),
    Error(String),
}

pub enum Command {
    Send(String),
    Close,
}

/// Bruno's rule: no scheme → ws:// for localhost, wss:// otherwise; http(s) map to ws(s).
pub fn normalize_url(url: &str) -> String {
    let url = url.trim();
    if let Some(r) = url.strip_prefix("http://") {
        return format!("ws://{r}");
    }
    if let Some(r) = url.strip_prefix("https://") {
        return format!("wss://{r}");
    }
    if url.contains("://") {
        return url.to_string();
    }
    if url.contains("localhost") || url.contains("127.0.0.1") { format!("ws://{url}") } else { format!("wss://{url}") }
}

fn set_read_timeout(ws: &WebSocket<MaybeTlsStream<std::net::TcpStream>>) {
    let t = Some(Duration::from_millis(50));
    match ws.get_ref() {
        MaybeTlsStream::Plain(s) => {
            let _ = s.set_read_timeout(t);
        }
        MaybeTlsStream::NativeTls(s) => {
            let _ = s.get_ref().set_read_timeout(t);
        }
        _ => {}
    }
}

/// Connects on a new thread; returns the command sender. Events (including errors) go to `events`.
/// `notify` is called after every event so the UI can repaint.
pub fn connect(url: &str, headers: Vec<(String, String)>, keep_alive_ms: u64, events: Sender<Event>, notify: impl Fn() + Send + 'static) -> Sender<Command> {
    let (cmd_tx, cmd_rx): (Sender<Command>, Receiver<Command>) = channel();
    let url = normalize_url(url);
    std::thread::spawn(move || {
        let emit = |e: Event| {
            let _ = events.send(e);
            notify();
        };
        let mut request = match url.as_str().into_client_request() {
            Ok(r) => r,
            Err(e) => return emit(Event::Error(e.to_string())),
        };
        for (k, v) in &headers {
            match (tungstenite::http::HeaderName::from_bytes(k.as_bytes()), tungstenite::http::HeaderValue::from_str(v)) {
                (Ok(k), Ok(v)) => {
                    request.headers_mut().insert(k, v);
                }
                _ => return emit(Event::Error(format!("invalid header `{k}`"))),
            }
        }
        let (mut ws, resp) = match tungstenite::connect(request) {
            Ok(x) => x,
            Err(e) => return emit(Event::Error(e.to_string())),
        };
        emit(Event::Opened { status: resp.status().as_u16() });
        set_read_timeout(&ws);
        let mut last_ping = Instant::now();
        loop {
            match cmd_rx.try_recv() {
                Ok(Command::Send(text)) => match ws.send(Message::text(text.clone())) {
                    Ok(()) => emit(Event::Sent(text)),
                    Err(e) => emit(Event::Error(e.to_string())),
                },
                Ok(Command::Close) | Err(TryRecvError::Disconnected) => {
                    let _ = ws.close(None);
                    let _ = ws.flush();
                    return emit(Event::Closed("closed by client".into()));
                }
                Err(TryRecvError::Empty) => {}
            }
            if keep_alive_ms > 0 && last_ping.elapsed() >= Duration::from_millis(keep_alive_ms) {
                let _ = ws.send(Message::Ping(Vec::new().into()));
                last_ping = Instant::now();
            }
            match ws.read() {
                Ok(Message::Text(t)) => emit(Event::Received(t.to_string())),
                Ok(Message::Binary(b)) => emit(Event::Received(format!("<binary {} bytes> {}", b.len(), String::from_utf8_lossy(&b)))),
                Ok(Message::Close(frame)) => {
                    return emit(Event::Closed(frame.map(|f| format!("{} {}", u16::from(f.code), f.reason)).unwrap_or_else(|| "closed by server".into())));
                }
                Ok(_) => {}
                Err(tungstenite::Error::Io(e)) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
                Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => return emit(Event::Closed("connection closed".into())),
                Err(e) => return emit(Event::Error(e.to_string())),
            }
        }
    });
    cmd_tx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_rules() {
        assert_eq!(normalize_url("localhost:8080/ws"), "ws://localhost:8080/ws");
        assert_eq!(normalize_url("echo.example.com"), "wss://echo.example.com");
        assert_eq!(normalize_url("https://x/y"), "wss://x/y");
    }

    #[test]
    fn echo_round_trip() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut ws = tungstenite::accept_hdr(stream, |req: &tungstenite::handshake::server::Request, resp| {
                assert_eq!(req.headers()["x-token"], "abc");
                Ok(resp)
            })
            .unwrap();
            while let Ok(m) = ws.read() {
                if m.is_text() {
                    ws.send(Message::text(format!("echo: {}", m.to_text().unwrap()))).unwrap();
                }
            }
        });
        let (tx, rx) = channel();
        let cmd = connect(&format!("localhost:{port}"), vec![("x-token".into(), "abc".into())], 0, tx, || {});
        assert!(matches!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), Event::Opened { status: 101, .. }));
        cmd.send(Command::Send("hi".into())).unwrap();
        assert!(matches!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), Event::Sent(s) if s == "hi"));
        assert!(matches!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), Event::Received(s) if s == "echo: hi"));
        cmd.send(Command::Close).unwrap();
        assert!(matches!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), Event::Closed(_)));
    }
}

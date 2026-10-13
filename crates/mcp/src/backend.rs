//! Where MCP tool calls end up: a control-channel method call.

use std::io::{BufReader, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::headless::Headless;

/// Something that answers control-channel methods (`engine.execute`, `document.inspect`,
/// `ui.pointer`, `ui.render`, `app.export`, …). See `vectorcraft_ui_egui::control` for the list.
pub trait Backend {
    /// Call one method. `Ok` carries the `result`, `Err` the error message.
    fn call(&mut self, method: &str, params: Value) -> Result<Value, String>;
    /// True when a real UI is attached (`ui.*` methods like `ui.key` / `ui.inspect` work).
    fn has_ui(&self) -> bool;
    /// Short human description ("headless", "remote 127.0.0.1:7979").
    fn describe(&self) -> String;
    /// Why UI-only tools fail right now, when [`has_ui`](Self::has_ui) is false for a reason
    /// beyond "no app is running". `None` means the generic "start the app" advice applies.
    fn ui_holdup(&self) -> Option<String> {
        None
    }
    /// Reconsider where calls go before a UI-only tool reads [`has_ui`](Self::has_ui). Most
    /// backends already know; one that follows a running app looks again here, so a tool gated on
    /// the UI answers from a live routing rather than from the last call's.
    fn refresh(&mut self) {}
}

/// A running VectorCraft app, reached through its loopback control port.
pub struct Remote {
    addr: String,
    conn: Option<(BufReader<TcpStream>, TcpStream)>,
    next_id: u64,
}

/// Longest accepted control-channel reply line (bytes, without the newline). The app answers
/// small JSON objects except for images, which reach tens of megabytes; anything longer is refused
/// and the connection dropped, so a faulty endpoint can't make the server buffer without bound
/// (#1727).
const MAX_REPLY_BYTES: usize = 256 * 1024 * 1024;

impl Remote {
    /// Connect to `addr` (e.g. `127.0.0.1:7979`), failing fast when nothing is listening.
    pub fn connect(addr: &str) -> std::io::Result<Self> {
        let mut r = Self::lazy(addr);
        r.reconnect()?;
        Ok(r)
    }

    /// Reach `addr` without contacting it: the first call connects, and a connection that has
    /// failed is attempted again on the next one. [`connect`](Self::connect) is the form that
    /// fails at once, for a caller that asked for an app explicitly.
    pub fn lazy(addr: impl Into<String>) -> Self {
        Self { addr: addr.into(), conn: None, next_id: 1 }
    }

    pub fn addr(&self) -> &str {
        &self.addr
    }

    #[cfg(test)]
    pub(crate) fn connection_probe(&self) -> TcpStream {
        self.conn.as_ref().unwrap().1.try_clone().unwrap()
    }

    /// Whether a connection to the app is open right now.
    pub fn is_connected(&self) -> bool {
        self.conn.is_some()
    }

    /// Connect unless already connected, without failing when nothing is listening: the result
    /// says whether the app can be reached. A failed attempt is retried by the next call.
    pub fn ensure_connected(&mut self) -> bool {
        if self.conn.is_some() {
            return true;
        }
        self.reconnect().is_ok()
    }

    /// Drop the connection without contacting the app: the next call starts fresh.
    fn disconnect(&mut self) {
        self.conn = None;
    }

    fn reconnect(&mut self) -> std::io::Result<()> {
        self.conn = None;
        let mut last = std::io::Error::new(std::io::ErrorKind::NotFound, format!("cannot resolve {}", self.addr));
        for sa in self.addr.to_socket_addrs()? {
            match TcpStream::connect_timeout(&sa, Duration::from_millis(800)) {
                Ok(s) => {
                    s.set_nodelay(true).ok();
                    // The app answers within 60 s (its own timeout); leave headroom.
                    s.set_read_timeout(Some(Duration::from_secs(90))).ok();
                    let read = s.try_clone()?;
                    self.conn = Some((BufReader::new(read), s));
                    return Ok(());
                }
                Err(e) => last = e,
            }
        }
        Err(last)
    }

    fn prepare_connection(&mut self) -> std::io::Result<()> {
        if let Some((_, stream)) = self.conn.as_ref() {
            stream.set_nonblocking(true)?;
            let probe = stream.peek(&mut [0]);
            stream.set_nonblocking(false)?;
            match probe {
                // Closed by the app, or holding bytes nobody asked for (an app that explains why it
                // closes): either way no reply to the next request can be read from it.
                Ok(_) => self.conn = None,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(_) => self.conn = None,
            }
        }
        if self.conn.is_none() {
            self.reconnect()?;
        }
        Ok(())
    }

    fn roundtrip(&mut self, line: &str) -> std::io::Result<String> {
        let Some((reader, writer)) = self.conn.as_mut() else {
            return Err(std::io::Error::new(std::io::ErrorKind::NotConnected, "not connected to the app"));
        };
        writer.write_all(line.as_bytes())?;
        writer.write_all(b"\n")?;
        writer.flush()?;
        let mut buf = Vec::new();
        if !crate::server::read_line_capped(reader, &mut buf, MAX_REPLY_BYTES)? {
            return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "control channel closed"));
        }
        if buf.len() > MAX_REPLY_BYTES {
            // A faulty endpoint's endless line is refused rather than buffered: the connection is
            // dropped and the call reports uncertainty, as for any reply that never arrived whole.
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, format!("control reply longer than {MAX_REPLY_BYTES} bytes")));
        }
        String::from_utf8(buf).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }
}

impl Backend for Remote {
    fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        self.prepare_connection().map_err(|e| {
            self.conn = None;
            format!("VectorCraft app at {} is not reachable: {e}; `{method}`: nothing was sent", self.addr)
        })?;
        let id = self.next_id;
        self.next_id += 1;
        let line = json!({"id": id, "method": method, "params": params}).to_string();
        let reply = self.roundtrip(&line).map_err(|e| {
            self.conn = None;
            format!("VectorCraft app at {}: `{method}` failed: {e}; operation may have completed; inspect state before retrying", self.addr)
        })?;
        let v: Value = serde_json::from_str(reply.trim()).map_err(|e| format!("bad reply from app: {e}"))?;
        if v.get("id").is_none() && v.get("ok").and_then(Value::as_bool) == Some(false) {
            // The app's refusals carry no id (too many connections, a line it would not read):
            // it ran nothing and closes the connection, so the next call starts a fresh one.
            self.conn = None;
            let why = v.get("error").and_then(Value::as_str).unwrap_or("unknown error");
            return Err(format!("VectorCraft app at {} refused `{method}`: {why}; nothing was run", self.addr));
        }
        if v.get("id") != Some(&json!(id)) {
            // Not the answer to this call: the stream is desynchronised (or the endpoint faulty),
            // so the connection is dropped and the next call starts a fresh one. The operation may
            // still have completed, as with any reply that never arrived whole (#1728).
            self.conn = None;
            return Err(format!(
                "VectorCraft app at {}: reply id mismatch (expected {id}); connection dropped; operation may have completed; inspect state before retrying",
                self.addr
            ));
        }
        if v.get("ok").and_then(Value::as_bool) == Some(true) {
            Ok(v.get("result").cloned().unwrap_or(Value::Null))
        } else {
            Err(v.get("error").and_then(Value::as_str).unwrap_or("unknown error").to_string())
        }
    }

    fn has_ui(&self) -> bool {
        true
    }

    fn describe(&self) -> String {
        format!("remote {}", self.addr)
    }
}

/// How long [`Auto`] waits before looking for the app again after an attempt that found nothing.
/// A session with no app open must not pay a connect timeout on every tool call, and an app the
/// user opens during the session should be picked up while they wait for the next reply.
const PROBE_INTERVAL: Duration = Duration::from_secs(1);

/// Where [`Auto`] sends the next call.
enum Route {
    /// An app answers: the call goes there.
    App,
    /// No app answers: the call goes to the in-process headless session.
    Headless,
    /// The app that was in use went away: the call fails loudly rather than succeeding against a
    /// headless document nobody sees. The app is retried, throttled, on later calls.
    Gone,
}

/// The server without a mode flag: the running app when one answers and the session may leave
/// where it is, an in-process headless session otherwise.
///
/// An MCP client starts its server when a session begins, which is normally before the user opens
/// VectorCraft. Deciding the backend once — the way `--connect` does, by choice — would leave that
/// app unreachable for the server's whole lifetime (#1470). But a working session is never swapped
/// mid-way: once the headless session holds unsaved edits it stays put however long an app runs,
/// and once an app was in use its going away errors instead of falling back to a fresh headless
/// document. Each switch that does happen is a log record, which a client that sent
/// `logging/setLevel` receives as a `notifications/message`.
pub struct Auto {
    remote: Remote,
    /// Built on first use: a session served by an app never needs the document.
    headless: Option<Headless>,
    /// Whether calls go to the app. Stays true through an outage, so the app — not a fresh
    /// headless document — is retried.
    on_app: bool,
    /// The earliest time to look for the app again, after the last attempt found nothing.
    probe_after: Option<Instant>,
    /// How long that wait is; shorter in tests.
    pub(crate) re_probe: Duration,
}

impl Auto {
    pub fn new(addr: impl Into<String>) -> Self {
        Self { remote: Remote::lazy(addr), headless: None, on_app: false, probe_after: None, re_probe: PROBE_INTERVAL }
    }

    /// The address an app is looked for at.
    pub fn addr(&self) -> &str {
        self.remote.addr()
    }

    /// Whether tool calls currently reach the app.
    pub fn on_app(&self) -> bool {
        self.on_app
    }

    /// Whether the headless session may be left for an app that starts answering: it holds no
    /// unsaved edits, so nothing built so far drops out of view.
    fn headless_untouched(&self) -> bool {
        self.headless.as_ref().is_none_or(|h| h.session.doc().is_ok_and(|d| !d.is_dirty()))
    }

    /// Record where calls go, saying so when it changed (a log record, which a client that
    /// sent `logging/setLevel` receives as a `notifications/message`).
    fn set_routing(&mut self, app: bool) {
        if app != self.on_app {
            let addr = self.remote.addr().to_string();
            log::info!(
                "backend: now using the {}",
                if app { format!("VectorCraft app at {addr}") } else { format!("in-process headless session (no app answered at {addr})") }
            );
            self.on_app = app;
        }
    }

    /// Decide where the next call goes, looking for the app again when it wasn't there recently.
    fn route(&mut self) -> Route {
        if self.remote.is_connected() {
            self.set_routing(true);
            return Route::App;
        }
        if let Some(after) = self.probe_after
            && Instant::now() < after
        {
            // A recent attempt found nothing: don't pay another connect timeout yet.
            return if self.on_app { Route::Gone } else { Route::Headless };
        }
        if self.remote.ensure_connected() {
            self.probe_after = None;
            if self.on_app || self.headless_untouched() {
                self.set_routing(true);
                return Route::App;
            }
            // The headless session holds unsaved edits: it stays put, however long the app runs.
            // The just-opened connection is dropped, or the next call would take it for a live
            // routing; UI-only tools say so through `ui_holdup`, everything else keeps working
            // headless. Retried, throttled: saving the session lets a later probe switch.
            self.remote.disconnect();
            self.probe_after = Some(Instant::now() + self.re_probe);
            return Route::Headless;
        }
        self.probe_after = Some(Instant::now() + self.re_probe);
        if self.on_app { Route::Gone } else { Route::Headless }
    }

    fn session(&mut self) -> &mut Headless {
        self.headless.get_or_insert_with(Headless::with_document)
    }
}

impl Backend for Auto {
    fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        match self.route() {
            Route::App => self.remote.call(method, params),
            Route::Headless => self.session().call(method, params),
            Route::Gone => Err(format!(
                "the VectorCraft app at {} went away: restart it to keep working in it, or restart this server with `--headless` for an in-process session",
                self.remote.addr()
            )),
        }
    }

    fn has_ui(&self) -> bool {
        self.on_app
    }

    fn refresh(&mut self) {
        self.route();
    }

    /// Why UI-only tools fail right now, when `has_ui` is false for a reason beyond "no app is
    /// running": the session is pinned to a headless document with unsaved edits.
    fn ui_holdup(&self) -> Option<String> {
        (!self.on_app && !self.headless_untouched()).then(|| {
            format!(
                "this server keeps its headless session, which holds unsaved edits; restart it with `--connect {}` to work in the app",
                self.remote.addr()
            )
        })
    }

    fn describe(&self) -> String {
        if self.on_app {
            self.remote.describe()
        } else if self.headless_untouched() {
            format!("headless (an app at {} takes over while this session holds no edits)", self.remote.addr())
        } else {
            format!("headless (holding edits; restart with `--connect {}` to work in the app)", self.remote.addr())
        }
    }
}

//! Small HTTP/1.1 loopback listener shared by all registry adapters.

use crate::adapters::{
    CargoAdapter, CondaAdapter, NpmAdapter, PypiAdapter, RegistryAdapter, RegistryContext,
    RegistryRouteAction,
};
use crate::{compress_brotli, ProxyConfig, CONTENT_ENCODING_BROTLI};
use isolock_cas::InMemoryCas;
use isolock_core::AirlockMode;
use isolock_plan::AirlockPlan;
use std::borrow::Cow;
use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

const MAX_HEADER_BYTES: usize = 64 * 1024;
const READ_TIMEOUT: Duration = Duration::from_secs(5);
const ACCEPT_POLL_INTERVAL: Duration = Duration::from_millis(10);

/// A bound loopback server that can be run synchronously or moved to a thread.
pub struct ProxyServer {
    listener: TcpListener,
    state: Arc<ServerState>,
}

struct ServerState {
    mode: AirlockMode,
    base_url: String,
    plan: Arc<AirlockPlan>,
    cas: Arc<InMemoryCas>,
    adapters: Vec<Box<dyn RegistryAdapter>>,
}

/// A running proxy thread. Dropping it requests a clean listener shutdown.
pub struct ProxyHandle {
    address: SocketAddr,
    shutdown: Arc<AtomicBool>,
    thread: Option<JoinHandle<io::Result<()>>>,
}

struct HttpRequest {
    method: String,
    path: String,
    headers: HashMap<String, String>,
}

#[derive(Clone)]
struct HttpResponse<'a> {
    status: u16,
    reason: &'static str,
    content_type: &'static str,
    body: Cow<'a, [u8]>,
    is_metadata: bool,
    is_artifact: bool,
    etag: Option<String>,
    content_range: Option<String>,
    content_encoding: Option<&'static str>,
}

impl ProxyServer {
    /// Binds to a loopback IP address. Set `config.port` to zero for an OS-assigned port.
    pub fn bind<P, C>(config: ProxyConfig, plan: P, cas: C) -> io::Result<Self>
    where
        P: Into<Arc<AirlockPlan>>,
        C: Into<Arc<InMemoryCas>>,
    {
        let ip = config.bind_host.parse::<IpAddr>().map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "proxy bind_host must be a loopback IP address",
            )
        })?;
        if !ip.is_loopback() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "the hermetic proxy may only bind to a loopback address",
            ));
        }
        let listener = TcpListener::bind(SocketAddr::new(ip, config.port))?;
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?;
        let base_url = format!("http://{address}");
        let state = Arc::new(ServerState {
            mode: config.mode,
            base_url,
            plan: plan.into(),
            cas: cas.into(),
            adapters: vec![
                Box::new(CondaAdapter),
                Box::new(PypiAdapter),
                Box::new(NpmAdapter),
                Box::new(CargoAdapter),
            ],
        });
        Ok(Self { listener, state })
    }

    /// Returns the actual listener address, including the assigned port.
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    /// Returns the local HTTP origin used in synthesized registry URLs.
    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.state.base_url
    }

    /// Runs the listener until the process is interrupted or the listener fails.
    pub fn serve(self) -> io::Result<()> {
        self.serve_until(None)
    }

    /// Starts serving on a background thread and returns a stoppable handle.
    pub fn spawn(self) -> io::Result<ProxyHandle> {
        let address = self.local_addr()?;
        let shutdown = Arc::new(AtomicBool::new(false));
        let thread_shutdown = Arc::clone(&shutdown);
        let thread = thread::Builder::new()
            .name("isolock-proxy".into())
            .spawn(move || self.serve_until(Some(thread_shutdown)))?;
        Ok(ProxyHandle {
            address,
            shutdown,
            thread: Some(thread),
        })
    }

    fn serve_until(self, shutdown: Option<Arc<AtomicBool>>) -> io::Result<()> {
        loop {
            if shutdown
                .as_ref()
                .is_some_and(|flag| flag.load(Ordering::Acquire))
            {
                return Ok(());
            }
            match self.listener.accept() {
                Ok((stream, _peer)) => {
                    let state = Arc::clone(&self.state);
                    let _ = thread::Builder::new()
                        .name("isolock-proxy-request".into())
                        .spawn(move || handle_connection(stream, &state));
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    thread::sleep(ACCEPT_POLL_INTERVAL);
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            }
        }
    }
}

impl ProxyHandle {
    /// Returns the address of the live listener.
    #[must_use]
    pub const fn local_addr(&self) -> SocketAddr {
        self.address
    }

    /// Returns the HTTP origin of the live listener.
    #[must_use]
    pub fn base_url(&self) -> String {
        format!("http://{}", self.address)
    }

    /// Stops the listener and joins its accept loop.
    pub fn shutdown(mut self) -> io::Result<()> {
        self.shutdown.store(true, Ordering::Release);
        self.join()
    }

    fn join(&mut self) -> io::Result<()> {
        let Some(thread) = self.thread.take() else {
            return Ok(());
        };
        thread
            .join()
            .map_err(|_| io::Error::other("proxy listener thread panicked"))?
    }
}

impl Drop for ProxyHandle {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        let _ = self.join();
    }
}

fn handle_connection(mut stream: TcpStream, state: &ServerState) {
    let _ = stream.set_read_timeout(Some(READ_TIMEOUT));
    let request = match read_request(&mut stream) {
        Ok(Some(request)) => request,
        Ok(None) => return,
        Err(_) => {
            let response = text_response(400, "Bad Request", "malformed HTTP request\n");
            let _ = write_response(&mut stream, &response, false, None, None, None);
            return;
        }
    };
    let head_only = request.method == "HEAD";
    let response = route_request(&request, state);
    let accept_encoding = request.headers.get("accept-encoding").map(String::as_str);
    let range = request.headers.get("range").map(String::as_str);
    let if_range = request.headers.get("if-range").map(String::as_str);
    let _ = write_response(
        &mut stream,
        &response,
        head_only,
        accept_encoding,
        range,
        if_range,
    );
}

fn route_request<'a>(request: &HttpRequest, state: &'a ServerState) -> HttpResponse<'a> {
    if request.method != "GET" && request.method != "HEAD" {
        return text_response(
            405,
            "Method Not Allowed",
            "only GET and HEAD are supported\n",
        );
    }
    let context = RegistryContext {
        plan: &state.plan,
        cas: &state.cas,
        base_url: &state.base_url,
    };
    for adapter in &state.adapters {
        match adapter.handle_route(&request.method, &request.path, &context) {
            RegistryRouteAction::NotFound => continue,
            RegistryRouteAction::ServeIndex { content_type, body } => {
                return HttpResponse {
                    status: 200,
                    reason: "OK",
                    content_type,
                    body: Cow::Owned(body),
                    is_metadata: true,
                    is_artifact: false,
                    etag: None,
                    content_range: None,
                    content_encoding: None,
                };
            }
            RegistryRouteAction::DownloadArtifact {
                digest,
                filename: _,
            } => {
                if let Some(bytes) = state.cas.get(&digest) {
                    return artifact_response(bytes, digest.as_str());
                }
                return cache_miss(state.mode);
            }
            RegistryRouteAction::CacheMiss { filename } => {
                let message = if filename.is_empty() {
                    "planned artifact is absent from CAS\n".to_owned()
                } else {
                    format!("planned artifact {filename:?} is absent from CAS\n")
                };
                return fail_closed_response(state.mode, &message);
            }
            RegistryRouteAction::BadRequest(message) => {
                return text_response(400, "Bad Request", &format!("{message}\n"));
            }
            RegistryRouteAction::InvalidMetadata(message) => {
                return text_response(500, "Internal Server Error", &format!("{message}\n"));
            }
        }
    }
    text_response(404, "Not Found", "registry resource not found\n")
}

fn cache_miss(mode: AirlockMode) -> HttpResponse<'static> {
    fail_closed_response(mode, "planned artifact is absent from CAS\n")
}

fn fail_closed_response(mode: AirlockMode, message: &str) -> HttpResponse<'static> {
    match mode {
        AirlockMode::Connected => text_response(502, "Bad Gateway", message),
        AirlockMode::Ci | AirlockMode::LocalAirlock => {
            text_response(403, "Forbidden", &format!("airlock violation: {message}"))
        }
    }
}

fn artifact_response<'a>(bytes: &'a [u8], digest: &str) -> HttpResponse<'a> {
    HttpResponse {
        status: 200,
        reason: "OK",
        content_type: "application/octet-stream",
        body: Cow::Borrowed(bytes),
        is_metadata: false,
        is_artifact: true,
        etag: Some(format!("\"{digest}\"")),
        content_range: None,
        content_encoding: None,
    }
}

fn text_response(status: u16, reason: &'static str, text: &str) -> HttpResponse<'static> {
    HttpResponse {
        status,
        reason,
        content_type: "text/plain; charset=utf-8",
        body: Cow::Owned(text.as_bytes().to_vec()),
        is_metadata: false,
        is_artifact: false,
        etag: None,
        content_range: None,
        content_encoding: None,
    }
}

fn read_request(stream: &mut TcpStream) -> io::Result<Option<HttpRequest>> {
    let mut bytes = Vec::with_capacity(2048);
    let mut chunk = [0_u8; 1024];
    loop {
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            return if bytes.is_empty() {
                Ok(None)
            } else {
                Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "incomplete request",
                ))
            };
        }
        bytes.extend_from_slice(&chunk[..read]);
        if let Some(position) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            if position + 4 > MAX_HEADER_BYTES {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "HTTP headers exceed the configured limit",
                ));
            }
            bytes.truncate(position + 4);
            break;
        }
        if bytes.len() > MAX_HEADER_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "HTTP headers exceed the configured limit",
            ));
        }
    }
    parse_request(&bytes)
}

fn parse_request(bytes: &[u8]) -> io::Result<Option<HttpRequest>> {
    let request = std::str::from_utf8(bytes)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "HTTP headers are not UTF-8"))?;
    let mut lines = request.split("\r\n");
    let Some(request_line) = lines.next() else {
        return Ok(None);
    };
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts.next().unwrap_or_default();
    let path = request_parts.next().unwrap_or_default();
    let version = request_parts.next().unwrap_or_default();
    if method.is_empty()
        || path.is_empty()
        || !matches!(version, "HTTP/1.0" | "HTTP/1.1")
        || request_parts.next().is_some()
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid request line",
        ));
    }
    let mut headers = HashMap::new();
    for line in lines.take_while(|line| !line.is_empty()) {
        let Some((name, value)) = line.split_once(':') else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid HTTP header",
            ));
        };
        if name.is_empty() || !name.bytes().all(is_header_name_byte) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid HTTP header name",
            ));
        }
        let key = name.to_ascii_lowercase();
        let value = value.trim();
        headers
            .entry(key)
            .and_modify(|existing: &mut String| {
                existing.push_str(", ");
                existing.push_str(value);
            })
            .or_insert_with(|| value.to_owned());
    }
    Ok(Some(HttpRequest {
        method: method.to_owned(),
        path: path.to_owned(),
        headers,
    }))
}

fn is_header_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'!' | b'#'
                | b'$'
                | b'%'
                | b'&'
                | b'\''
                | b'*'
                | b'+'
                | b'-'
                | b'.'
                | b'^'
                | b'_'
                | b'`'
                | b'|'
                | b'~'
        )
}

fn write_response(
    stream: &mut TcpStream,
    response: &HttpResponse<'_>,
    head_only: bool,
    accept_encoding: Option<&str>,
    range: Option<&str>,
    if_range: Option<&str>,
) -> io::Result<()> {
    let mut response = response.clone();
    if response.is_metadata && accepts_brotli(accept_encoding) {
        match compress_brotli(&response.body) {
            Ok(compressed) => {
                response.body = Cow::Owned(compressed);
                response.content_encoding = Some(CONTENT_ENCODING_BROTLI);
            }
            Err(error) => {
                response = text_response(
                    500,
                    "Internal Server Error",
                    &format!("metadata compression failed: {error}\n"),
                );
            }
        }
    }

    let range = range.filter(|_| {
        if_range
            .is_none_or(|validator| response.etag.as_deref() == Some(validator) || validator == "*")
    });
    if response.is_artifact {
        if let Some(range) = range {
            if let Some((start, end)) = byte_range(range, response.body.len()) {
                response.content_range =
                    Some(format!("bytes {start}-{end}/{}", response.body.len()));
                response.status = 206;
                response.reason = "Partial Content";
                response.body = Cow::Owned(response.body[start..=end].to_vec());
            } else {
                let full_length = response.body.len();
                response = text_response(416, "Range Not Satisfiable", "invalid byte range\n");
                response.content_range = Some(format!("bytes */{full_length}"));
            }
        }
    }

    let content_length = response.body.len();
    let mut header = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n",
        response.status, response.reason, response.content_type, content_length
    );
    if response.is_metadata {
        header.push_str("Vary: Accept-Encoding\r\n");
    }
    if response.is_artifact {
        header.push_str("Accept-Ranges: bytes\r\n");
    }
    if let Some(etag) = response.etag {
        header.push_str(&format!("ETag: {etag}\r\n"));
    }
    if let Some(content_range) = response.content_range {
        header.push_str(&format!("Content-Range: {content_range}\r\n"));
    }
    if let Some(encoding) = response.content_encoding {
        header.push_str(&format!("Content-Encoding: {encoding}\r\n"));
    }
    header.push_str("\r\n");
    stream.write_all(header.as_bytes())?;
    if !head_only {
        stream.write_all(response.body.as_ref())?;
    }
    stream.flush()
}

fn accepts_brotli(header: Option<&str>) -> bool {
    let Some(header) = header else {
        return false;
    };
    let mut wildcard_quality = None;
    for encoding in header.split(',') {
        let mut parts = encoding.trim().split(';');
        let name = parts.next().unwrap_or_default().trim();
        let quality = parts
            .filter_map(|parameter| parameter.trim().strip_prefix("q="))
            .next()
            .and_then(|value| value.parse::<f32>().ok())
            .unwrap_or(1.0);
        if name.eq_ignore_ascii_case("br") {
            return quality > 0.0;
        }
        if name == "*" {
            wildcard_quality = Some(quality);
        }
    }
    wildcard_quality.is_some_and(|quality| quality > 0.0)
}

fn byte_range(range: &str, length: usize) -> Option<(usize, usize)> {
    let (unit, range) = range.split_once('=')?;
    if !unit.eq_ignore_ascii_case("bytes") || range.contains(',') || length == 0 {
        return None;
    }
    let (start, end) = range.split_once('-')?;
    let length = u64::try_from(length).ok()?;
    let (start, end) = if start.is_empty() {
        let suffix = end.parse::<u64>().ok()?;
        if suffix == 0 {
            return None;
        }
        (length.saturating_sub(suffix), length - 1)
    } else {
        let start = start.parse::<u64>().ok()?;
        let end = if end.is_empty() {
            length - 1
        } else {
            end.parse::<u64>().ok()?.min(length - 1)
        };
        (start, end)
    };
    if start > end || start >= length {
        return None;
    }
    Some((usize::try_from(start).ok()?, usize::try_from(end).ok()?))
}

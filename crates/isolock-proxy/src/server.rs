//! Loopback binding and bounded listener orchestration, independent of registry composition.

use crate::adapters::{default_adapters, AdapterRegistry, RegistryContext, RegistryRouteAction};
use crate::request::{read_request, HttpRequest};
use crate::resources::{Counters, DeadlineWriter, Workers};
use crate::response::HttpResponse;
use crate::{ProxyConfig, ProxyLimits, ProxyStats};
use isolock_cas::InMemoryCas;
use isolock_core::AirlockMode;
use isolock_plan::AirlockPlan;
use std::io;
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

const ACCEPT_POLL_INTERVAL: Duration = Duration::from_millis(10);
const OVERLOAD_WRITE_TIMEOUT: Duration = Duration::from_millis(25);

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
    adapters: AdapterRegistry,
    limits: ProxyLimits,
}

/// A running proxy. Drop stops acceptance, interrupts sockets, and applies the shutdown grace.
pub struct ProxyHandle {
    address: SocketAddr,
    shutdown: Arc<AtomicBool>,
    monitor: ProxyMonitor,
    thread: Option<JoinHandle<io::Result<()>>>,
}

/// Request accounting that remains observable after a handle is shut down or dropped.
#[derive(Clone)]
pub struct ProxyMonitor(Arc<Counters>);

impl ProxyMonitor {
    /// Returns a point-in-time snapshot; concurrent completion can change counters immediately.
    #[must_use]
    pub fn stats(&self) -> ProxyStats {
        self.0.snapshot()
    }
}

impl ProxyServer {
    /// Binds with the default Conda, PyPI, npm, and Cargo registry adapters.
    ///
    /// # Errors
    /// Returns an error for invalid resource limits, non-loopback binding, or listener I/O.
    pub fn bind<P, C>(config: ProxyConfig, plan: P, cas: C) -> io::Result<Self>
    where
        P: Into<Arc<AirlockPlan>>,
        C: Into<Arc<InMemoryCas>>,
    {
        Self::bind_with_adapters(config, plan, cas, default_adapters())
    }

    /// Binds using an ordered injected registry; the first matching adapter owns a route.
    ///
    /// # Errors
    /// Returns an error for invalid resource limits, non-loopback binding, or listener I/O.
    pub fn bind_with_adapters<P, C>(
        config: ProxyConfig,
        plan: P,
        cas: C,
        adapters: AdapterRegistry,
    ) -> io::Result<Self>
    where
        P: Into<Arc<AirlockPlan>>,
        C: Into<Arc<InMemoryCas>>,
    {
        config.limits.validate()?;
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
        let base_url = format!("http://{}", listener.local_addr()?);
        let state = Arc::new(ServerState {
            mode: config.mode,
            base_url,
            plan: plan.into(),
            cas: cas.into(),
            adapters,
            limits: config.limits,
        });
        Ok(Self { listener, state })
    }

    /// Returns the actual listener address, including the assigned port.
    ///
    /// # Errors
    /// Returns the operating system's socket-address query error.
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    /// Returns the local HTTP origin used in synthesized registry URLs.
    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.state.base_url
    }

    /// Runs until listener failure; in-process cancellation is available through `spawn`.
    ///
    /// # Errors
    /// Returns listener/worker-spawn I/O errors or a shutdown grace timeout.
    pub fn serve(self) -> io::Result<()> {
        self.serve_until(
            Arc::new(AtomicBool::new(false)),
            Arc::new(Counters::default()),
        )
    }

    /// Starts serving on a background thread and returns a stoppable handle.
    ///
    /// # Errors
    /// Returns socket-address or thread-spawn errors before the handle can be created.
    pub fn spawn(self) -> io::Result<ProxyHandle> {
        let address = self.local_addr()?;
        let shutdown = Arc::new(AtomicBool::new(false));
        let counters = Arc::new(Counters::default());
        let thread_shutdown = Arc::clone(&shutdown);
        let thread_counters = Arc::clone(&counters);
        let thread = thread::Builder::new()
            .name("isolock-proxy".into())
            .spawn(move || self.serve_until(thread_shutdown, thread_counters))?;
        Ok(ProxyHandle {
            address,
            shutdown,
            monitor: ProxyMonitor(counters),
            thread: Some(thread),
        })
    }

    fn serve_until(self, shutdown: Arc<AtomicBool>, counters: Arc<Counters>) -> io::Result<()> {
        let mut workers = Workers::new(
            self.state.limits,
            Arc::clone(&shutdown),
            Arc::clone(&counters),
        );
        let result = self.accept_connections(&shutdown, &counters, &mut workers);
        // Close the listener before waiting for pending workers, including on listener failure.
        drop(self.listener);
        let completion = workers.finish();
        result.and(completion)
    }

    fn accept_connections(
        &self,
        shutdown: &Arc<AtomicBool>,
        counters: &Counters,
        workers: &mut Workers,
    ) -> io::Result<()> {
        while !shutdown.load(Ordering::Acquire) {
            workers.reap();
            match self.listener.accept() {
                Ok((mut stream, _peer)) => {
                    if shutdown.load(Ordering::Acquire) {
                        break;
                    }
                    if workers.at_capacity() {
                        counters.reject();
                        // No queue, no worker, no registry evaluation; never contact upstream.
                        let mut writer = DeadlineWriter::new(&mut stream, OVERLOAD_WRITE_TIMEOUT);
                        let _ = HttpResponse::text(
                            503,
                            "Service Unavailable",
                            "proxy request capacity exhausted\n",
                        )
                        .write_to(&mut writer, false);
                        continue;
                    }
                    let state = Arc::clone(&self.state);
                    let cancelled = Arc::clone(shutdown);
                    workers.spawn(stream, move |stream| {
                        handle_connection(stream, &state, &cancelled)
                    })?;
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    thread::sleep(ACCEPT_POLL_INTERVAL)
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            }
        }
        Ok(())
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

    /// Retains accounting even when shutdown reports an uncooperative adapter timeout.
    #[must_use]
    pub fn monitor(&self) -> ProxyMonitor {
        self.monitor.clone()
    }

    /// Closes the listener, cancels requests, interrupts sockets, and joins bounded work.
    ///
    /// # Errors
    /// Returns listener/worker errors, a listener panic, or `TimedOut` if an adapter ignores
    /// cancellation past the grace period. Such workers are detached and counted by `monitor`.
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
        if let Err(error) = self.join() {
            eprintln!("isolock proxy shutdown: {error}");
        }
    }
}

fn handle_connection(mut stream: TcpStream, state: &ServerState, cancelled: &AtomicBool) {
    let request = match read_request(
        &mut stream,
        Duration::from_millis(state.limits.header_timeout_ms),
    ) {
        Ok(Some(request)) => request,
        Ok(None) => return,
        Err(_) => {
            let mut writer = DeadlineWriter::new(
                &mut stream,
                Duration::from_millis(state.limits.write_timeout_ms),
            );
            let _ = HttpResponse::text(400, "Bad Request", "malformed HTTP request\n")
                .write_to(&mut writer, false);
            return;
        }
    };
    if cancelled.load(Ordering::Acquire) {
        return;
    }
    let response = route_request(&request, state, cancelled)
        .select_range(
            request.headers.get("range").map(String::as_str),
            request.headers.get("if-range").map(String::as_str),
        )
        .compress(request.headers.get("accept-encoding").map(String::as_str));
    if let Ok(response) = response {
        let mut writer = DeadlineWriter::new(
            &mut stream,
            Duration::from_millis(state.limits.write_timeout_ms),
        );
        let _ = response.write_to(&mut writer, request.method == "HEAD");
    }
}

fn route_request<'a>(
    request: &HttpRequest,
    state: &'a ServerState,
    cancelled: &AtomicBool,
) -> HttpResponse<'a> {
    if request.method != "GET" && request.method != "HEAD" {
        return HttpResponse::text(
            405,
            "Method Not Allowed",
            "only GET and HEAD are supported\n",
        );
    }
    let context = RegistryContext {
        plan: &state.plan,
        cas: &state.cas,
        base_url: &state.base_url,
        cancelled,
    };
    match state
        .adapters
        .route(&request.method, &request.path, &context)
    {
        RegistryRouteAction::NotFound => {
            HttpResponse::text(404, "Not Found", "registry resource not found\n")
        }
        RegistryRouteAction::ServeIndex { content_type, body } => {
            HttpResponse::metadata(content_type, body)
        }
        RegistryRouteAction::DownloadArtifact { digest, .. } => match state.cas.get(&digest) {
            Some(bytes) => HttpResponse::artifact(bytes, digest.as_str()),
            None => fail_closed_response(state.mode, "planned artifact is absent from CAS\n"),
        },
        RegistryRouteAction::CacheMiss { filename } => fail_closed_response(
            state.mode,
            &format!("planned artifact {filename:?} is absent from CAS\n"),
        ),
        RegistryRouteAction::BadRequest(message) => {
            HttpResponse::text(400, "Bad Request", &format!("{message}\n"))
        }
        RegistryRouteAction::InvalidMetadata(message) => {
            HttpResponse::text(500, "Internal Server Error", &format!("{message}\n"))
        }
    }
}

fn fail_closed_response(mode: AirlockMode, message: &str) -> HttpResponse<'static> {
    match mode {
        AirlockMode::Connected => HttpResponse::text(502, "Bad Gateway", message),
        AirlockMode::Ci | AirlockMode::LocalAirlock => {
            HttpResponse::text(403, "Forbidden", &format!("airlock violation: {message}"))
        }
    }
}

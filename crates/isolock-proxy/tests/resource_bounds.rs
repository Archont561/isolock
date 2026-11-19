//! Executable M6 resource contract, including real sockets and injected adapters.
#![allow(clippy::unwrap_used, clippy::expect_used)] // Fixture failures must stop the test immediately.

use isolock_cas::{InMemoryCas, Sha256Digest};
use isolock_core::{AirlockMode, Registry};
use isolock_plan::{AirlockPlan, PlannedArtifact, PlannedArtifactMetadata};
use isolock_proxy::{
    default_adapters, ProxyConfig, ProxyHandle, ProxyLimits, ProxyMonitor, ProxyServer,
    RegistryAdapter, RegistryContext, RegistryRouteAction,
};
use rstest::fixture;
use rstest_bdd_macros::{given, scenario, then, when};
use std::cell::RefCell;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

const WAIT: Duration = Duration::from_secs(3);

struct Extension {
    entered: SyncSender<()>,
    released: Arc<AtomicBool>,
    cooperative: bool,
}
impl RegistryAdapter for Extension {
    fn ecosystem(&self) -> Registry {
        Registry::Npm
    }
    fn handle_route(
        &self,
        _: &str,
        path: &str,
        context: &RegistryContext<'_>,
    ) -> RegistryRouteAction {
        match path {
            "/extension" => RegistryRouteAction::ServeIndex {
                content_type: "application/json",
                body: b"{\"extension\":true}".to_vec(),
            },
            "/panic" => panic!("intentional adapter failure"),
            "/hold" => {
                self.entered.try_send(()).unwrap();
                let deadline = Instant::now() + WAIT;
                while !self.released.load(Ordering::Acquire)
                    && !(self.cooperative && context.is_cancelled())
                    && Instant::now() < deadline
                {
                    thread::sleep(Duration::from_millis(1));
                }
                RegistryRouteAction::ServeIndex {
                    content_type: "application/json",
                    body: b"{}".to_vec(),
                }
            }
            _ => RegistryRouteAction::NotFound,
        }
    }
}

struct Harness {
    proxy: Option<ProxyHandle>,
    address: SocketAddr,
    monitor: ProxyMonitor,
    entered: Receiver<()>,
    released: Arc<AtomicBool>,
    clients: Vec<TcpStream>,
}
impl Harness {
    fn start(limits: ProxyLimits, cooperative: bool, bytes: Vec<u8>) -> Self {
        let (entered, receiver) = mpsc::sync_channel(256);
        let released = Arc::new(AtomicBool::new(false));
        let mut adapters = default_adapters();
        adapters.register(Extension {
            entered,
            released: Arc::clone(&released),
            cooperative,
        });
        let (plan, cas) = snapshot(bytes);
        let proxy = ProxyServer::bind_with_adapters(
            ProxyConfig::default().with_limits(limits),
            plan,
            cas,
            adapters,
        )
        .unwrap()
        .spawn()
        .unwrap();
        Self {
            address: proxy.local_addr(),
            monitor: proxy.monitor(),
            proxy: Some(proxy),
            entered: receiver,
            released,
            clients: Vec::new(),
        }
    }
    fn hold(&mut self) {
        self.clients.push(send(
            self.address,
            b"GET /hold HTTP/1.1\r\nHost: local\r\n\r\n",
        ));
        self.entered.recv_timeout(WAIT).unwrap();
    }
    fn shutdown(&mut self) -> std::io::Result<()> {
        self.proxy.take().unwrap().shutdown()
    }
}
impl Drop for Harness {
    fn drop(&mut self) {
        self.released.store(true, Ordering::Release);
        drop(self.proxy.take());
    }
}

fn limits(max_concurrency: usize) -> ProxyLimits {
    ProxyLimits {
        max_concurrency,
        shutdown_grace_ms: 250,
        ..ProxyLimits::default()
    }
}

fn snapshot(bytes: Vec<u8>) -> (AirlockPlan, InMemoryCas) {
    let mut plan = AirlockPlan::new();
    let mut cas = InMemoryCas::new();
    let digest = Sha256Digest::from_bytes(&bytes);
    cas.insert(digest.clone(), bytes).unwrap();
    for (name, digest) in [
        ("fixture", digest),
        ("missing", Sha256Digest::from_bytes(b"absent")),
    ] {
        plan.add_with_metadata(
            PlannedArtifact {
                name: name.into(),
                version: "1.0.0".into(),
                registry: Registry::Npm,
                digest,
            },
            PlannedArtifactMetadata {
                filename: Some(format!("{name}-1.0.0.tgz")),
                registry_metadata: Some(
                    serde_json::json!({"package_json": {"name": name, "version": "1.0.0"}}),
                ),
                ..PlannedArtifactMetadata::default()
            },
        );
    }
    (plan, cas)
}

fn send(address: SocketAddr, request: &[u8]) -> TcpStream {
    let mut stream = TcpStream::connect_timeout(&address, WAIT).unwrap();
    stream.set_read_timeout(Some(WAIT)).unwrap();
    stream.set_write_timeout(Some(WAIT)).unwrap();
    stream.write_all(request).unwrap();
    stream
}

fn request(address: SocketAddr, path: &str) -> Vec<u8> {
    let stream = send(
        address,
        format!("GET {path} HTTP/1.1\r\nHost: local\r\nConnection: close\r\n\r\n").as_bytes(),
    );
    read_reply(stream)
}

fn read_reply(stream: TcpStream) -> Vec<u8> {
    // Read the framed response, not EOF: overload may close while client input remains unread.
    let mut reader = BufReader::new(stream);
    let mut reply = Vec::new();
    loop {
        let mut line = Vec::new();
        assert!(reader.read_until(b'\n', &mut line).unwrap() > 0);
        reply.extend_from_slice(&line);
        if line == b"\r\n" {
            break;
        }
    }
    let head = std::str::from_utf8(&reply).unwrap();
    let length: usize = head
        .lines()
        .find_map(|line| line.strip_prefix("Content-Length: "))
        .unwrap()
        .parse()
        .unwrap();
    let offset = reply.len();
    reply.resize(offset + length, 0);
    reader.read_exact(&mut reply[offset..]).unwrap();
    reply
}

fn wait_until(condition: impl Fn() -> bool) {
    let deadline = Instant::now() + WAIT;
    while !condition() {
        assert!(Instant::now() < deadline, "bounded test wait expired");
        thread::sleep(Duration::from_millis(1));
    }
}

#[derive(Default)]
struct World {
    harness: RefCell<Option<Harness>>,
    reply: RefCell<Vec<u8>>,
    shutdown: RefCell<Option<(std::io::Result<()>, Duration)>>,
}
#[fixture]
fn world() -> World {
    World::default()
}

#[given("the local registry proxy has reached its configured request-concurrency limit")]
fn saturated(world: &World) {
    let mut harness = Harness::start(limits(2), true, b"artifact".to_vec());
    harness.hold();
    harness.hold();
    assert_eq!(harness.monitor.stats().active, 2);
    *world.harness.borrow_mut() = Some(harness);
}
#[when("another client requests a registry resource")]
fn overload(world: &World) {
    let harness = world.harness.borrow();
    let harness = harness.as_ref().unwrap();
    *world.reply.borrow_mut() = request(harness.address, "/cargo/config.json");
}
#[then("the proxy returns a deterministic fail-closed overload response or uses a bounded wait")]
fn closed_overload(world: &World) {
    assert!(world
        .reply
        .borrow()
        .starts_with(b"HTTP/1.1 503 Service Unavailable\r\n"));
}
#[then("it does not create an additional unbounded request thread")]
fn no_worker(world: &World) {
    let harness = world.harness.borrow();
    let harness = harness.as_ref().unwrap();
    for _ in 0..12 {
        assert!(request(harness.address, "/cargo/config.json").starts_with(b"HTTP/1.1 503 "));
    }
    let stats = harness.monitor.stats();
    assert_eq!(stats.spawned, 2);
    assert_eq!(stats.peak_active, 2);
    assert_eq!(stats.rejected, 13);
}

#[given("one or more proxy requests are active within the configured limit")]
fn active_requests(world: &World) {
    let mut harness = Harness::start(limits(2), true, b"artifact".to_vec());
    harness.hold();
    harness
        .clients
        .push(send(harness.address, b"GET /cargo/config"));
    wait_until(|| harness.monitor.stats().active == 2);
    *world.harness.borrow_mut() = Some(harness);
}
#[when("the execution session requests proxy shutdown")]
fn shutdown(world: &World) {
    let started = Instant::now();
    let result = world.harness.borrow_mut().as_mut().unwrap().shutdown();
    *world.shutdown.borrow_mut() = Some((result, started.elapsed()));
}
#[then("the listener stops accepting new connections")]
fn listener_closed(world: &World) {
    let address = world.harness.borrow().as_ref().unwrap().address;
    assert!(TcpStream::connect_timeout(&address, Duration::from_millis(100)).is_err());
}
#[then("the execution session accounts for worker completion within its bounded shutdown policy")]
fn workers_accounted(world: &World) {
    let shutdown = world.shutdown.borrow();
    let (result, elapsed) = shutdown.as_ref().unwrap();
    assert!(result.is_ok(), "{result:?}");
    assert!(*elapsed < Duration::from_secs(2));
    let stats = world.harness.borrow().as_ref().unwrap().monitor.stats();
    assert_eq!(stats.active, 0);
    assert_eq!(stats.completed, 2);
    assert_eq!(stats.detached, 0);
}

#[given("the local registry proxy has available request capacity")]
fn capacity(world: &World) {
    *world.harness.borrow_mut() = Some(Harness::start(limits(2), true, b"artifact".to_vec()));
}
#[when("a package manager requests planned metadata or an artifact")]
fn planned_request(world: &World) {
    let harness = world.harness.borrow();
    let harness = harness.as_ref().unwrap();
    assert!(request(harness.address, "/cargo/config.json").starts_with(b"HTTP/1.1 200 OK\r\n"));
    *world.reply.borrow_mut() = request(harness.address, "/npm/fixture/-/fixture-1.0.0.tgz");
}
#[then("the existing registry adapter response and cache-miss policy are preserved")]
fn registry_preserved(world: &World) {
    let reply = world.reply.borrow();
    assert!(reply.starts_with(b"HTTP/1.1 200 OK\r\n"));
    assert!(reply.ends_with(b"artifact"));
    let harness = world.harness.borrow();
    let harness = harness.as_ref().unwrap();
    assert!(request(harness.address, "/npm/missing/-/missing-1.0.0.tgz")
        .starts_with(b"HTTP/1.1 403 Forbidden\r\n"));
    let extension = request(harness.address, "/extension");
    assert!(extension.starts_with(b"HTTP/1.1 200 OK\r\n"));
    assert!(extension.ends_with(b"{\"extension\":true}"));
}

#[scenario(
    path = "../../backlog/features/proxy-resource-bounds.feature",
    index = 0
)]
fn bdd_bounded_overload(world: World) {
    let _ = world;
}
#[scenario(
    path = "../../backlog/features/proxy-resource-bounds.feature",
    index = 1
)]
fn bdd_pending_shutdown(world: World) {
    let _ = world;
}
#[scenario(
    path = "../../backlog/features/proxy-resource-bounds.feature",
    index = 2
)]
fn bdd_available_capacity(world: World) {
    let _ = world;
}

#[test]
fn uncooperative_adapter_is_accounted_for_without_unbounded_shutdown() {
    let mut harness = Harness::start(
        ProxyLimits {
            shutdown_grace_ms: 30,
            ..limits(1)
        },
        false,
        vec![],
    );
    harness.hold();
    let started = Instant::now();
    assert_eq!(
        harness.shutdown().unwrap_err().kind(),
        std::io::ErrorKind::TimedOut
    );
    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(TcpStream::connect_timeout(&harness.address, Duration::from_millis(100)).is_err());
    let stats = harness.monitor.stats();
    assert_eq!(stats.detached, 1);
    assert_eq!(stats.active, 1);
    harness.released.store(true, Ordering::Release);
    wait_until(|| harness.monitor.stats().active == 0);
    assert_eq!(harness.monitor.stats().completed, 1);
}

#[test]
fn panicked_worker_releases_capacity_and_does_not_kill_listener() {
    let mut harness = Harness::start(limits(1), true, vec![]);
    let _stream = send(
        harness.address,
        b"GET /panic HTTP/1.1\r\nHost: local\r\n\r\n",
    );
    wait_until(|| harness.monitor.stats().panicked == 1);
    // The accept loop reaps finished workers before accepting the next connection.
    assert!(request(harness.address, "/extension").starts_with(b"HTTP/1.1 200 OK\r\n"));
    harness.shutdown().unwrap();
    assert_eq!(harness.monitor.stats().completed, 2);
}

#[test]
fn header_deadline_is_absolute_even_when_a_client_trickles_bytes() {
    let mut harness = Harness::start(
        ProxyLimits {
            header_timeout_ms: 100,
            ..limits(1)
        },
        true,
        vec![],
    );
    let mut client = send(harness.address, b"GET /");
    wait_until(|| harness.monitor.stats().active == 1);
    for _ in 0..10 {
        let _ = client.write_all(b"x");
        thread::sleep(Duration::from_millis(25));
    }
    assert_eq!(harness.monitor.stats().completed, 1);
    assert!(read_reply(client).starts_with(b"HTTP/1.1 400 Bad Request\r\n"));
    harness.shutdown().unwrap();
}

#[test]
fn slow_reader_cannot_keep_a_worker_past_the_wire_write_deadline() {
    let mut harness = Harness::start(
        ProxyLimits {
            write_timeout_ms: 50,
            ..limits(1)
        },
        true,
        vec![0xCD; 8 * 1024 * 1024],
    );
    let _unread = send(
        harness.address,
        b"GET /npm/fixture/-/fixture-1.0.0.tgz HTTP/1.1\r\nHost: local\r\n\r\n",
    );
    wait_until(|| harness.monitor.stats().completed == 1);
    harness.shutdown().unwrap();
    assert_eq!(harness.monitor.stats().detached, 0);
}

#[test]
fn invalid_limits_are_rejected_before_listener_startup() {
    for invalid in [
        ProxyLimits {
            max_concurrency: 0,
            ..limits(1)
        },
        ProxyLimits {
            max_concurrency: 257,
            ..limits(1)
        },
        ProxyLimits {
            header_timeout_ms: 0,
            ..limits(1)
        },
        ProxyLimits {
            write_timeout_ms: 60001,
            ..limits(1)
        },
        ProxyLimits {
            shutdown_grace_ms: 0,
            ..limits(1)
        },
    ] {
        assert!(
            matches!(ProxyServer::bind(ProxyConfig::default().with_limits(invalid), AirlockPlan::new(), InMemoryCas::new()), Err(error) if error.kind() == std::io::ErrorKind::InvalidInput)
        );
    }
    let legacy: ProxyConfig =
        serde_json::from_str(r#"{"bind_host":"127.0.0.1","port":0,"mode":"LocalAirlock"}"#)
            .unwrap();
    assert_eq!(legacy.limits, ProxyLimits::default());
    assert_eq!(legacy.mode, AirlockMode::LocalAirlock);
}

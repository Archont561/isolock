//! Bounded request ownership, cancellation, socket deadlines, and shutdown accounting.

use serde::{Deserialize, Serialize};
use std::io::{self, Write};
use std::net::{Shutdown, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// Operational resource bounds; none of these values affects artifact/plan identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct ProxyLimits {
    /// Concurrent request workers (1–256); overload has no queue or extra worker.
    pub max_concurrency: usize,
    /// Absolute HTTP header deadline, including trickled bytes (1–60000 ms).
    pub header_timeout_ms: u64,
    /// Absolute wire-write deadline, including partial progress (1–60000 ms).
    pub write_timeout_ms: u64,
    /// Grace period after interrupting worker sockets (1–60000 ms).
    pub shutdown_grace_ms: u64,
}

impl Default for ProxyLimits {
    fn default() -> Self {
        Self {
            max_concurrency: 32,
            header_timeout_ms: 5000,
            write_timeout_ms: 5000,
            shutdown_grace_ms: 2000,
        }
    }
}

impl ProxyLimits {
    /// Validate resource bounds before binding a listener or starting workers.
    ///
    /// # Errors
    /// Returns `InvalidInput` for zero/excessive concurrency or deadlines outside 1–60000 ms.
    pub fn validate(&self) -> io::Result<()> {
        if !(1..=256).contains(&self.max_concurrency)
            || [
                self.header_timeout_ms,
                self.write_timeout_ms,
                self.shutdown_grace_ms,
            ]
            .iter()
            .any(|value| !(1..=60000).contains(value))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid proxy concurrency or deadline limits",
            ));
        }
        Ok(())
    }
}

/// Observable request accounting. Completed includes panics; detached is cumulative.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProxyStats {
    pub active: usize,
    pub peak_active: usize,
    pub spawned: usize,
    pub rejected: usize,
    pub completed: usize,
    pub panicked: usize,
    /// Uncooperative adapter workers explicitly accounted for at grace expiry.
    pub detached: usize,
}

#[derive(Default)]
pub(super) struct Counters {
    active: AtomicUsize,
    peak_active: AtomicUsize,
    spawned: AtomicUsize,
    rejected: AtomicUsize,
    completed: AtomicUsize,
    panicked: AtomicUsize,
    detached: AtomicUsize,
}

impl Counters {
    pub fn snapshot(&self) -> ProxyStats {
        ProxyStats {
            active: self.active.load(Ordering::Acquire),
            peak_active: self.peak_active.load(Ordering::Acquire),
            spawned: self.spawned.load(Ordering::Acquire),
            rejected: self.rejected.load(Ordering::Acquire),
            completed: self.completed.load(Ordering::Acquire),
            panicked: self.panicked.load(Ordering::Acquire),
            detached: self.detached.load(Ordering::Acquire),
        }
    }
    pub fn reject(&self) {
        self.rejected.fetch_add(1, Ordering::Relaxed);
    }
}

struct Completion(Arc<Counters>);
impl Drop for Completion {
    fn drop(&mut self) {
        if thread::panicking() {
            self.0.panicked.fetch_add(1, Ordering::Relaxed);
        }
        self.0.completed.fetch_add(1, Ordering::Release);
        self.0.active.fetch_sub(1, Ordering::Release);
    }
}

struct Worker {
    control: TcpStream,
    thread: JoinHandle<()>,
}

pub(super) struct Workers {
    running: Vec<Worker>,
    limits: ProxyLimits,
    stop: Arc<AtomicBool>,
    counters: Arc<Counters>,
}

impl Workers {
    pub fn new(limits: ProxyLimits, stop: Arc<AtomicBool>, counters: Arc<Counters>) -> Self {
        Self {
            running: Vec::new(),
            limits,
            stop,
            counters,
        }
    }
    pub fn at_capacity(&self) -> bool {
        self.running.len() >= self.limits.max_concurrency
    }

    pub fn spawn(
        &mut self,
        stream: TcpStream,
        work: impl FnOnce(TcpStream) + Send + 'static,
    ) -> io::Result<()> {
        if self.at_capacity() {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "proxy request worker limit reached",
            ));
        }
        let control = stream.try_clone()?;
        let counters = Arc::clone(&self.counters);
        let active = counters.active.fetch_add(1, Ordering::AcqRel) + 1;
        counters.peak_active.fetch_max(active, Ordering::Relaxed);
        counters.spawned.fetch_add(1, Ordering::Relaxed);
        match thread::Builder::new()
            .name("isolock-proxy-request".into())
            .spawn(move || {
                let _completion = Completion(counters);
                work(stream);
            }) {
            Ok(thread) => self.running.push(Worker { control, thread }),
            Err(error) => {
                self.counters.active.fetch_sub(1, Ordering::Release);
                self.counters.spawned.fetch_sub(1, Ordering::Relaxed);
                return Err(error);
            }
        }
        Ok(())
    }

    pub fn reap(&mut self) {
        let mut index = 0;
        while index < self.running.len() {
            if self.running[index].thread.is_finished() {
                // Completion records panics too; a panicked adapter does not kill the listener.
                let _ = self.running.swap_remove(index).thread.join();
            } else {
                index += 1;
            }
        }
    }

    pub fn finish(&mut self) -> io::Result<()> {
        self.stop.store(true, Ordering::Release);
        for worker in &self.running {
            let _ = worker.control.shutdown(Shutdown::Both);
        }
        let deadline = Instant::now() + Duration::from_millis(self.limits.shutdown_grace_ms);
        loop {
            self.reap();
            if self.running.is_empty() {
                return Ok(());
            }
            if Instant::now() >= deadline {
                let count = self.running.len();
                self.counters.detached.fetch_add(count, Ordering::Release);
                self.running.clear(); // Explicitly detached, still cancellation-signalled and counted.
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!("{count} proxy worker(s) exceeded the shutdown grace period"),
                ));
            }
            thread::sleep(Duration::from_millis(1));
        }
    }
}

impl Drop for Workers {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

/// Reapply the remaining absolute deadline to every partial socket write.
pub(super) struct DeadlineWriter<'a> {
    stream: &'a mut TcpStream,
    deadline: Instant,
}
impl<'a> DeadlineWriter<'a> {
    pub fn new(stream: &'a mut TcpStream, timeout: Duration) -> Self {
        Self {
            stream,
            deadline: Instant::now() + timeout,
        }
    }
}
impl Write for DeadlineWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "HTTP write deadline exceeded",
            ));
        }
        self.stream.set_write_timeout(Some(remaining))?;
        self.stream.write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
}

//! Bounded caller proxy for platform drivers that must stay on one owner thread.
//!
//! In particular, Objective-C Bluetooth objects must be created, used, and
//! dropped on the worker that owns them. A caller timeout cannot cancel a
//! native operation already in progress, so the worker retains the per-device
//! lease until its driver has actually been dropped.

use super::Connection;
use std::collections::HashSet;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(12);
const CALLER_GRACE: Duration = Duration::from_millis(50);
const CONNECT_POLL: Duration = Duration::from_millis(10);
const MAX_REQUEST_BYTES: usize = 64 * 1024;
const MAX_WORKERS: usize = 8;

static LEASES: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

fn leases() -> &'static Mutex<HashSet<String>> {
    LEASES.get_or_init(|| Mutex::new(HashSet::new()))
}

struct Lease(String);

impl Lease {
    fn acquire(identity: &str) -> io::Result<Self> {
        let key = normalize_identity(identity);
        let mut active = leases().lock().unwrap_or_else(|e| e.into_inner());
        if active.contains(&key) {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "A previous Bluetooth operation for this device is still stopping.",
            ));
        }
        if active.len() >= MAX_WORKERS {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "The Bluetooth worker limit has been reached.",
            ));
        }
        active.insert(key.clone());
        Ok(Self(key))
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        leases()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.0);
    }
}

fn normalize_identity(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Cancellation/poison state shared with a driver without moving the driver
/// itself off its owner thread.
pub struct Signals {
    cancelled: Arc<AtomicBool>,
    poisoned: AtomicBool,
    dropped: AtomicBool,
}

impl Signals {
    fn new(cancelled: Arc<AtomicBool>) -> Self {
        Self {
            cancelled,
            poisoned: AtomicBool::new(false),
            dropped: AtomicBool::new(false),
        }
    }

    pub fn stopped(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
            || self.poisoned.load(Ordering::Acquire)
            || self.dropped.load(Ordering::Acquire)
    }

    fn poison(&self) {
        self.poisoned.store(true, Ordering::Release);
    }

    fn drop_proxy(&self) {
        self.dropped.store(true, Ordering::Release);
    }
}

/// A driver is constructed and exclusively used on its worker thread. It need
/// not implement `Send`, which permits thread-affine native handles.
pub(super) trait Driver {
    fn read(
        &mut self,
        buffer: &mut [u8],
        deadline: Instant,
        signals: &Signals,
    ) -> io::Result<usize>;

    fn write(&mut self, bytes: &[u8], deadline: Instant, signals: &Signals) -> io::Result<()>;
}

enum Request {
    Read {
        size: usize,
        deadline: Instant,
        reply: mpsc::SyncSender<io::Result<Vec<u8>>>,
    },
    Write {
        bytes: Vec<u8>,
        deadline: Instant,
        reply: mpsc::SyncSender<io::Result<()>>,
    },
}

struct Connected {
    service_uuid: &'static str,
    requests: mpsc::SyncSender<Request>,
    signals: Arc<Signals>,
}

/// Connect a platform driver on its dedicated owner thread.
pub(super) fn connect(
    identity: String,
    cancelled: Arc<AtomicBool>,
    connector: impl FnOnce(Arc<Signals>, Instant) -> Result<(Box<dyn Driver>, &'static str), String>
        + Send
        + 'static,
) -> Result<Box<dyn Connection>, String> {
    let lease = Lease::acquire(&identity).map_err(|e| e.to_string())?;
    let signals = Arc::new(Signals::new(cancelled));
    let worker_signals = Arc::clone(&signals);
    let (connected_tx, connected_rx) = mpsc::sync_channel(1);

    thread::Builder::new()
        .name("bluetooth-owner".into())
        .spawn(move || {
            let mut lease = Some(lease);
            let connect_deadline = Instant::now() + CONNECT_TIMEOUT;
            let result = if worker_signals.stopped() {
                Err("Bluetooth connection was cancelled.".to_string())
            } else {
                connector(Arc::clone(&worker_signals), connect_deadline)
            };
            let (mut driver, service_uuid) = match result {
                Ok(value) => value,
                Err(error) => {
                    // No driver exists, so the device is free: release it before the caller can
                    // observe the failure and immediately try again.
                    drop(lease.take());
                    let _ = connected_tx.send(Err(error));
                    return;
                }
            };
            if worker_signals.stopped() || Instant::now() >= connect_deadline {
                worker_signals.poison();
                let _ = connected_tx.send(Err(
                    "Bluetooth connection timed out or was cancelled.".into()
                ));
                drop(driver);
                return;
            }

            let (requests_tx, requests_rx) = mpsc::sync_channel(1);
            if connected_tx.send(Ok((service_uuid, requests_tx))).is_err() {
                worker_signals.drop_proxy();
                drop(driver);
                return;
            }

            while !worker_signals.stopped() {
                let request = match requests_rx.recv_timeout(CONNECT_POLL) {
                    Ok(request) => request,
                    Err(mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                };
                match request {
                    Request::Read {
                        size,
                        deadline,
                        reply,
                    } => {
                        if worker_signals.stopped() || Instant::now() >= deadline {
                            let _ = reply.send(Err(io::Error::new(
                                io::ErrorKind::TimedOut,
                                "Bluetooth read deadline expired before I/O.",
                            )));
                            continue;
                        }
                        let mut buffer = vec![0; size];
                        let result = driver.read(&mut buffer, deadline, &worker_signals);
                        let result = if worker_signals.stopped() {
                            Err(io::Error::new(
                                io::ErrorKind::Interrupted,
                                "Bluetooth worker stopped.",
                            ))
                        } else if result.is_ok() && Instant::now() >= deadline {
                            Err(io::Error::new(
                                io::ErrorKind::TimedOut,
                                "Bluetooth read deadline expired.",
                            ))
                        } else {
                            result.map(|count| {
                                buffer.truncate(count.min(buffer.len()));
                                buffer
                            })
                        };
                        let _ = reply.send(result);
                    }
                    Request::Write {
                        bytes,
                        deadline,
                        reply,
                    } => {
                        if worker_signals.stopped() || Instant::now() >= deadline {
                            let _ = reply.send(Err(io::Error::new(
                                io::ErrorKind::TimedOut,
                                "Bluetooth write deadline expired before I/O.",
                            )));
                            continue;
                        }
                        let result = driver.write(&bytes, deadline, &worker_signals);
                        // A cancellation racing with an in-flight write makes
                        // delivery unknowable. Never report success in that case.
                        let result = if worker_signals.stopped()
                            || Instant::now() >= deadline
                            || result.is_err()
                        {
                            worker_signals.poison();
                            Err(io::Error::new(
                                io::ErrorKind::TimedOut,
                                "Bluetooth write outcome is unknown.",
                            ))
                        } else {
                            result
                        };
                        let _ = reply.send(result);
                    }
                }
            }
            drop(driver);
        })
        .map_err(|e| format!("Could not start Bluetooth owner thread: {e}"))?;

    let caller_deadline = Instant::now() + CONNECT_TIMEOUT;
    loop {
        if signals.stopped() {
            signals.poison();
            return Err("Bluetooth connection was cancelled.".into());
        }
        let remaining = caller_deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            signals.poison();
            return Err("Bluetooth connection timed out.".into());
        }
        match connected_rx.recv_timeout(remaining.min(CONNECT_POLL)) {
            Ok(Ok((service_uuid, requests))) => {
                return Ok(Box::new(WorkerConnection {
                    connected: Connected {
                        service_uuid,
                        requests,
                        signals,
                    },
                }));
            }
            Ok(Err(error)) => return Err(error),
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err("Bluetooth owner thread stopped during connection.".into());
            }
        }
    }
}

struct WorkerConnection {
    connected: Connected,
}

impl WorkerConnection {
    fn wait_reply<T>(
        &self,
        receiver: mpsc::Receiver<io::Result<T>>,
        deadline: Instant,
        write: bool,
    ) -> io::Result<T> {
        loop {
            if self.connected.signals.cancelled.load(Ordering::Acquire) {
                self.connected.signals.poison();
                return Err(io::Error::new(
                    if write {
                        io::ErrorKind::TimedOut
                    } else {
                        io::ErrorKind::Interrupted
                    },
                    if write {
                        "Bluetooth write outcome is unknown."
                    } else {
                        "Bluetooth operation cancelled."
                    },
                ));
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                self.connected.signals.poison();
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    if write {
                        "Bluetooth write outcome is unknown."
                    } else {
                        "Bluetooth operation timed out."
                    },
                ));
            }
            match receiver.recv_timeout(remaining.min(CONNECT_POLL)) {
                Ok(result) => return result,
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(io::Error::new(
                        io::ErrorKind::BrokenPipe,
                        "Bluetooth worker stopped.",
                    ));
                }
            }
        }
    }
}

impl Connection for WorkerConnection {
    fn service_uuid(&self) -> &'static str {
        self.connected.service_uuid
    }

    fn read(&mut self, buffer: &mut [u8], timeout: Duration) -> io::Result<usize> {
        if self.connected.signals.stopped() {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "Bluetooth connection is stopped.",
            ));
        }
        if buffer.len() > MAX_REQUEST_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Bluetooth read exceeds 64 KiB.",
            ));
        }
        let deadline = Instant::now() + timeout;
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        self.connected
            .requests
            .send(Request::Read {
                size: buffer.len(),
                deadline,
                reply: reply_tx,
            })
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "Bluetooth worker stopped."))?;
        let bytes = self.wait_reply(reply_rx, deadline + CALLER_GRACE, false)?;
        let count = bytes.len().min(buffer.len());
        buffer[..count].copy_from_slice(&bytes[..count]);
        Ok(count)
    }

    fn write_all(&mut self, bytes: &[u8], timeout: Duration) -> io::Result<()> {
        if bytes.len() > MAX_REQUEST_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Bluetooth write exceeds 64 KiB.",
            ));
        }
        if self.connected.signals.stopped() {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "Bluetooth connection is stopped.",
            ));
        }
        let deadline = Instant::now() + timeout;
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        self.connected
            .requests
            .send(Request::Write {
                bytes: bytes.to_vec(),
                deadline,
                reply: reply_tx,
            })
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "Bluetooth worker stopped."))?;
        self.wait_reply(reply_rx, deadline + CALLER_GRACE, true)
    }
}

impl Drop for WorkerConnection {
    fn drop(&mut self) {
        self.connected.signals.drop_proxy();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    use std::sync::{Condvar, Mutex};

    struct FakeDriver {
        writes: Arc<AtomicBool>,
        block: Option<Arc<(Mutex<bool>, Condvar)>>,
        eof: bool,
        read_timeout: bool,
        fail_write: bool,
    }

    impl Driver for FakeDriver {
        fn read(
            &mut self,
            buffer: &mut [u8],
            _deadline: Instant,
            _signals: &Signals,
        ) -> io::Result<usize> {
            if self.read_timeout {
                return Err(io::Error::new(io::ErrorKind::WouldBlock, "read timed out"));
            }
            if self.eof {
                return Ok(0);
            }
            let data = b"ok";
            let count = data.len().min(buffer.len());
            buffer[..count].copy_from_slice(&data[..count]);
            Ok(count)
        }

        fn write(
            &mut self,
            _bytes: &[u8],
            _deadline: Instant,
            _signals: &Signals,
        ) -> io::Result<()> {
            self.writes.store(true, Ordering::Release);
            if self.fail_write {
                return Err(io::Error::new(io::ErrorKind::BrokenPipe, "write failed"));
            }
            if let Some(gate) = &self.block {
                let (lock, cv) = &**gate;
                let mut open = lock.lock().unwrap();
                while !*open {
                    open = cv.wait(open).unwrap();
                }
            }
            Ok(())
        }
    }

    fn connector(
        writes: Arc<AtomicBool>,
        block: Option<Arc<(Mutex<bool>, Condvar)>>,
        eof: bool,
    ) -> impl FnOnce(Arc<Signals>, Instant) -> Result<(Box<dyn Driver>, &'static str), String>
           + Send
           + 'static {
        move |_, _| {
            Ok((
                Box::new(FakeDriver {
                    writes,
                    block,
                    eof,
                    read_timeout: false,
                    fail_write: false,
                }),
                "service",
            ))
        }
    }

    fn connect_fake(
        identity: &str,
        cancelled: Arc<AtomicBool>,
        writes: Arc<AtomicBool>,
        block: Option<Arc<(Mutex<bool>, Condvar)>>,
        eof: bool,
    ) -> Result<Box<dyn Connection>, String> {
        connect(identity.into(), cancelled, connector(writes, block, eof))
    }

    #[test]
    fn normal_read_and_eof() {
        let wrote = Arc::new(AtomicBool::new(false));
        let mut c = connect_fake(
            "read-normal",
            Arc::new(AtomicBool::new(false)),
            wrote.clone(),
            None,
            false,
        )
        .unwrap();
        let mut b = [0; 8];
        assert_eq!(c.read(&mut b, Duration::from_secs(1)).unwrap(), 2);
        assert_eq!(&b[..2], b"ok");
        drop(c);
        let mut c = connect_fake(
            "read-eof",
            Arc::new(AtomicBool::new(false)),
            wrote,
            None,
            true,
        )
        .unwrap();
        assert_eq!(c.read(&mut b, Duration::from_secs(1)).unwrap(), 0);
    }

    #[test]
    fn ordinary_would_block_read_does_not_poison_connection() {
        let writes = Arc::new(AtomicBool::new(false));
        let mut c = connect(
            "read-would-block".into(),
            Arc::new(AtomicBool::new(false)),
            move |_, _| {
                Ok((
                    Box::new(FakeDriver {
                        writes,
                        block: None,
                        eof: false,
                        read_timeout: true,
                        fail_write: false,
                    }),
                    "service",
                ))
            },
        )
        .unwrap();
        let mut buffer = [0; 8];
        assert_eq!(
            c.read(&mut buffer, Duration::from_secs(1))
                .unwrap_err()
                .kind(),
            io::ErrorKind::WouldBlock
        );
        assert!(c.write_all(b"still usable", Duration::from_secs(1)).is_ok());
    }

    #[test]
    fn driver_write_error_never_becomes_success() {
        let writes = Arc::new(AtomicBool::new(false));
        let mut c = connect(
            "write-error".into(),
            Arc::new(AtomicBool::new(false)),
            move |_, _| {
                Ok((
                    Box::new(FakeDriver {
                        writes,
                        block: None,
                        eof: false,
                        read_timeout: false,
                        fail_write: true,
                    }),
                    "service",
                ))
            },
        )
        .unwrap();
        assert!(c.write_all(b"fail", Duration::from_secs(1)).is_err());
    }

    #[test]
    fn stalled_write_times_out_unknown_and_retains_device_lease_until_worker_exits() {
        let gate = Arc::new((Mutex::new(false), Condvar::new()));
        let wrote = Arc::new(AtomicBool::new(false));
        let mut c = connect_fake(
            "blocked-write",
            Arc::new(AtomicBool::new(false)),
            wrote.clone(),
            Some(gate.clone()),
            false,
        )
        .unwrap();
        let started = Instant::now();
        let err = c
            .write_all(b"payload", Duration::from_millis(100))
            .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::TimedOut);
        assert!(err.to_string().contains("unknown"));
        assert!(started.elapsed() < Duration::from_millis(500));
        assert!(wrote.load(Ordering::Acquire));
        drop(c);

        let error = match connect_fake(
            "BLOCKED-WRITE",
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            None,
            false,
        ) {
            Ok(_) => panic!(
                "same-device connection unexpectedly succeeded while the prior worker was blocked"
            ),
            Err(error) => error,
        };
        assert!(error.contains("still stopping"));

        let (lock, cv) = &*gate;
        *lock.lock().unwrap() = true;
        cv.notify_all();
        let until = Instant::now() + Duration::from_secs(1);
        let mut reconnected = false;
        while Instant::now() < until {
            if let Ok(c) = connect_fake(
                "blocked-write",
                Arc::new(AtomicBool::new(false)),
                Arc::new(AtomicBool::new(false)),
                None,
                false,
            ) {
                drop(c);
                reconnected = true;
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert!(
            reconnected,
            "lease should release when the stuck worker returns and drops its driver"
        );
    }

    #[test]
    fn cancelled_connect_fails_and_oversize_requests_are_rejected() {
        let cancelled = Arc::new(AtomicBool::new(true));
        let error = match connect_fake(
            "already-cancelled",
            cancelled,
            Arc::new(AtomicBool::new(false)),
            None,
            false,
        ) {
            Ok(_) => panic!("cancelled connection unexpectedly succeeded"),
            Err(error) => error,
        };
        assert!(error.contains("cancelled"));

        let mut c = connect_fake(
            "oversize",
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            None,
            false,
        )
        .unwrap();
        assert_eq!(
            c.write_all(&vec![0; MAX_REQUEST_BYTES + 1], Duration::from_secs(1))
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(
            c.read(&mut vec![0; MAX_REQUEST_BYTES + 1], Duration::from_secs(1))
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
    }

    #[test]
    fn connector_failure_releases_lease() {
        let result = connect(
            "connect-failure".into(),
            Arc::new(AtomicBool::new(false)),
            |_, _| Err("expected".into()),
        );
        let error = match result {
            Ok(_) => panic!("connector failure unexpectedly succeeded"),
            Err(error) => error,
        };
        assert!(error.contains("expected"));
        let wrote = Arc::new(AtomicBool::new(false));
        assert!(connect_fake(
            "connect-failure",
            Arc::new(AtomicBool::new(false)),
            wrote,
            None,
            false
        )
        .is_ok());
    }
}

//! Start our own `rigctld` and keep it running (restart with backoff when it exits).
//!
//! It always listens on 127.0.0.1 only, so nobody on the network can talk to the rig.
//! The supervisor keeps rigctld's last log lines and process state for the settings page.

use std::collections::VecDeque;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use atlas_core::setup::{DeviceProblem, RigConfig, RigctldStatus};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

const BACKOFF_MIN: Duration = Duration::from_secs(1);
const BACKOFF_MAX: Duration = Duration::from_secs(30);
/// A run this long counts as healthy and resets the backoff.
const HEALTHY_RUN: Duration = Duration::from_secs(60);
const LOG_LINES: usize = 50;
/// How often to look again for a missing or inaccessible serial device (hot-plug).
const DEVICE_RECHECK: Duration = Duration::from_secs(2);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spec {
    pub program: String,
    pub model: u32,
    pub device: String,
    pub baud: u32,
    pub port: u16,
}

impl From<&RigConfig> for Spec {
    fn from(c: &RigConfig) -> Self {
        Self { program: c.rigctld.clone(), model: c.model, device: c.device.clone(), baud: c.baud, port: c.port }
    }
}

impl Spec {
    pub fn args(&self) -> Vec<String> {
        let mut a = vec!["-m".into(), self.model.to_string()];
        if !self.device.is_empty() {
            a.extend(["-r".into(), self.device.clone()]);
        }
        if self.baud > 0 {
            a.extend(["-s".into(), self.baud.to_string()]);
        }
        a.extend(["-T".into(), "127.0.0.1".into(), "-t".into(), self.port.to_string()]);
        a
    }
}

#[derive(Default)]
struct Shared {
    status: RigctldStatus,
    started: Option<Instant>,
    log: VecDeque<String>,
}

impl Shared {
    fn push_log(&mut self, line: String) {
        if self.log.len() == LOG_LINES {
            self.log.pop_front();
        }
        self.log.push_back(line);
    }
}

/// A running supervisor. `stop()` ends it cleanly; dropping it kills rigctld too.
pub struct Supervisor {
    shared: Arc<Mutex<Shared>>,
    stop: Option<oneshot::Sender<()>>,
    task: Option<JoinHandle<()>>,
}

impl Supervisor {
    pub fn status(&self) -> RigctldStatus {
        let s = self.shared.lock().expect("rigctld status lock");
        RigctldStatus {
            uptime_s: s.started.filter(|_| s.status.running).map(|t| t.elapsed().as_secs() as u32),
            log: s.log.iter().cloned().collect(),
            ..s.status.clone()
        }
    }

    /// Stop rigctld and wait until it has exited, so its serial and TCP ports are free.
    pub async fn stop(mut self) {
        if let Some(tx) = self.stop.take() {
            let _ = tx.send(());
        }
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        // not stopped cleanly: aborting drops the child, and kill_on_drop kills it
        if let Some(t) = &self.task {
            t.abort();
        }
    }
}

/// Can the serial device be used? Checked up front because rigctld often reports
/// nothing at all when it cannot open the port.
fn device_problem(device: &str) -> Option<DeviceProblem> {
    if device.is_empty() {
        return None;
    }
    if !crate::discover::port_exists(device) {
        Some(DeviceProblem::Missing)
    } else if !crate::discover::accessible(std::path::Path::new(device)) {
        Some(DeviceProblem::PermissionDenied)
    } else {
        None
    }
}

/// Is something already listening on the port rigctld would use?
fn port_taken(port: u16) -> bool {
    std::net::TcpListener::bind(("127.0.0.1", port)).is_err()
}

/// Run `rigctld` until the supervisor is stopped or dropped.
pub fn supervise(spec: Spec) -> Supervisor {
    let shared = Arc::new(Mutex::new(Shared::default()));
    let (stop_tx, mut stop_rx) = oneshot::channel::<()>();
    let st = shared.clone();
    let task = tokio::spawn(async move {
        let set = |f: &dyn Fn(&mut Shared)| f(&mut st.lock().expect("rigctld status lock"));
        let mut backoff = BACKOFF_MIN;
        loop {
            let started = Instant::now();
            let problem = device_problem(&spec.device);
            set(&|s| s.status.device_problem = problem);
            if problem.is_some() {
                // not worth starting rigctld; look again soon so plugging the rig in just works
                tokio::select! {
                    _ = tokio::time::sleep(DEVICE_RECHECK) => continue,
                    _ = &mut stop_rx => return,
                }
            }
            // rigctld exits silently with status 1 when its port is taken, so check first
            if port_taken(spec.port) {
                set(&|s| s.status.port_in_use = true);
            } else {
                set(&|s| s.status.port_in_use = false);
                let child = crate::process::command(&spec.program)
                    .args(spec.args())
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::piped())
                    .kill_on_drop(true)
                    .spawn();
                match child {
                    Err(e) => {
                        tracing::error!("cannot start {}: {e}", spec.program);
                        let msg = format!("{}: {e}", spec.program);
                        set(&|s| s.status.spawn_error = Some(msg.clone()));
                    }
                    Ok(mut child) => {
                        crate::process::tie_to_app(&child);
                        tracing::info!("rigctld started: {} {}", spec.program, spec.args().join(" "));
                        set(&|s| {
                            s.status.spawn_error = None;
                            s.status.running = true;
                            s.started = Some(Instant::now());
                            s.push_log(format!("--- {} {}", spec.program, spec.args().join(" ")));
                        });
                        let err = child.stderr.take().expect("piped stderr");
                        let log_to = st.clone();
                        let reader = tokio::spawn(async move {
                            let mut lines = BufReader::new(err).lines();
                            while let Ok(Some(l)) = lines.next_line().await {
                                log_to.lock().expect("rigctld status lock").push_log(l);
                            }
                        });
                        let exited = tokio::select! {
                            r = child.wait() => Some(r),
                            _ = &mut stop_rx => None,
                        };
                        let Some(r) = exited else {
                            let _ = child.kill().await; // kill + wait
                            reader.abort();
                            set(&|s| s.status.running = false);
                            return;
                        };
                        let _ = reader.await; // the last lines often explain the exit
                        let why = match r {
                            Ok(st) => st.to_string(),
                            Err(e) => e.to_string(),
                        };
                        tracing::warn!("rigctld exited: {why}");
                        set(&|s| {
                            s.status.running = false;
                            s.status.restarts += 1;
                            s.status.last_exit = Some(why.clone());
                        });
                    }
                }
            }
            if started.elapsed() >= HEALTHY_RUN {
                backoff = BACKOFF_MIN;
            }
            tokio::select! {
                _ = tokio::time::sleep(backoff) => {}
                _ = &mut stop_rx => return,
            }
            backoff = (backoff * 2).min(BACKOFF_MAX);
        }
    });
    Supervisor { shared, stop: Some(stop_tx), task: Some(task) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ftdx10_args() {
        let s = Spec { program: "rigctld".into(), model: 1042, device: "/dev/ttyUSB0".into(), baud: 9600, port: 4532 };
        assert_eq!(s.args().join(" "), "-m 1042 -r /dev/ttyUSB0 -s 9600 -T 127.0.0.1 -t 4532");
    }

    #[test]
    fn dummy_rig_args() {
        let s = Spec { program: "rigctld".into(), model: 1, device: String::new(), baud: 0, port: 4533 };
        assert_eq!(s.args().join(" "), "-m 1 -T 127.0.0.1 -t 4533");
    }

    async fn until(sup: &Supervisor, f: impl Fn(&RigctldStatus) -> bool) -> RigctldStatus {
        for _ in 0..100 {
            let s = sup.status();
            if f(&s) {
                return s;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("timed out; last status {:?}", sup.status());
    }

    #[tokio::test]
    async fn missing_executable_is_reported() {
        let sup = supervise(Spec {
            program: "/nonexistent/rigctld".into(),
            model: 1,
            device: String::new(),
            baud: 0,
            port: 45_401,
        });
        let s = until(&sup, |s| s.spawn_error.is_some()).await;
        assert!(!s.running);
        sup.stop().await;
    }

    #[tokio::test]
    async fn taken_port_is_reported_without_starting() {
        let _hold = std::net::TcpListener::bind("127.0.0.1:45402").unwrap();
        let sup = supervise(Spec { program: "rigctld".into(), model: 1, device: String::new(), baud: 0, port: 45_402 });
        let s = until(&sup, |s| s.port_in_use).await;
        assert!(!s.running && s.spawn_error.is_none());
        sup.stop().await;
    }

    #[tokio::test]
    async fn missing_device_is_reported_without_starting() {
        let sup = supervise(Spec {
            program: "rigctld".into(),
            model: 1042,
            device: "/dev/swatlas-gone".into(),
            baud: 9600,
            port: 45_404,
        });
        let s = until(&sup, |s| s.device_problem.is_some()).await;
        assert_eq!(s.device_problem, Some(DeviceProblem::Missing));
        assert!(!s.running);
        sup.stop().await;
    }

    /// Needs Hamlib: start the dummy rig, stop it, and the port is free right away.
    #[tokio::test]
    async fn stop_waits_for_exit() {
        if std::process::Command::new("rigctld").arg("--version").output().is_err() {
            eprintln!("rigctld not installed: skipping");
            return;
        }
        let port = 45_403;
        let sup = supervise(Spec { program: "rigctld".into(), model: 1, device: String::new(), baud: 0, port });
        until(&sup, |s| s.running).await;
        for _ in 0..50 {
            if tokio::net::TcpStream::connect(("127.0.0.1", port)).await.is_ok() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(port_taken(port), "rigctld should be listening");
        sup.stop().await;
        assert!(!port_taken(port), "port still taken after stop()");
    }
}

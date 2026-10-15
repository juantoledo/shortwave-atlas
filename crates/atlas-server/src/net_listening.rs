//! The NET band's listening history, kept in `net-listening.jsonl` (one session per line,
//! appended as each ends) and recorded by `atlas_core::net_listening::NetTracker` from what the
//! audio hub plays. It never touches `listening.jsonl`: the rig's history and its Stats don't
//! see internet listening.

use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use atlas_core::net::NetPlaying;
use atlas_core::net_listening::{clear_before, parse_lines, to_line, trim, NetSession, NetTracker};

use crate::files::write_atomic;

pub struct NetListeningService {
    /// `net-listening.jsonl`; `None` keeps the history in memory only (tests).
    file: Option<PathBuf>,
    inner: Mutex<Inner>,
}

struct Inner {
    /// Ended sessions, oldest first.
    list: Vec<NetSession>,
    tracker: NetTracker,
}

impl NetListeningService {
    /// Load the history; lines that do not read are skipped (with a warning).
    pub fn new(file: Option<PathBuf>) -> Arc<Self> {
        let mut list = vec![];
        if let Some(p) = &file {
            if let Ok(text) = std::fs::read_to_string(p) {
                let (l, bad) = parse_lines(&text);
                if bad > 0 {
                    tracing::warn!("skipped {bad} unreadable lines in {}", p.display());
                }
                list = l;
            }
        }
        list.sort_by_key(|s| s.start);
        trim(&mut list);
        Arc::new(Self { file, inner: Mutex::new(Inner { list, tracker: NetTracker::default() }) })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().expect("net listening lock")
    }

    /// Seconds of play before it counts (tests shorten it).
    pub fn set_dwell(&self, secs: i64) {
        self.lock().tracker.dwell_s = secs;
    }

    /// What is playing at `now`; a session that ends is saved.
    pub fn observe(&self, playing: Option<&NetPlaying>, now: i64) {
        let mut inner = self.lock();
        if let Some(s) = inner.tracker.observe(playing, now) {
            self.keep(&mut inner, s);
        }
    }

    /// End and save the session under way (stopping, shutting down).
    pub fn close(&self) {
        let mut inner = self.lock();
        if let Some(s) = inner.tracker.close() {
            self.keep(&mut inner, s);
        }
    }

    fn keep(&self, inner: &mut Inner, s: NetSession) {
        let line = to_line(&s);
        inner.list.push(s);
        if trim(&mut inner.list) {
            if let Some(p) = &self.file {
                let text: String = inner.list.iter().map(to_line).collect();
                if let Err(e) = write_atomic(p, &text) {
                    tracing::warn!("could not save the internet listening history to {}: {e}", p.display());
                }
            }
        } else if let Some(p) = &self.file {
            let append = || -> std::io::Result<()> {
                if let Some(dir) = p.parent() {
                    std::fs::create_dir_all(dir)?;
                }
                std::fs::OpenOptions::new().create(true).append(true).open(p)?.write_all(line.as_bytes())
            };
            if let Err(e) = append() {
                tracing::warn!("could not save an internet listening session to {}: {e}", p.display());
            }
        }
    }

    /// Run `f` on the ended sessions (oldest first) and the one under way.
    pub fn with<T>(&self, f: impl FnOnce(&[NetSession], Option<&NetSession>) -> T) -> T {
        let inner = self.lock();
        let open = inner.tracker.open();
        f(&inner.list, open.as_ref())
    }

    /// Forget the sessions that ended before `before`, or all of them; how many went.
    pub fn clear(&self, before: Option<i64>) -> Result<usize, String> {
        let mut inner = self.lock();
        let mut next = inner.list.clone();
        let n = clear_before(&mut next, before);
        if let Some(p) = &self.file {
            if p.exists() || !next.is_empty() {
                let text: String = next.iter().map(to_line).collect();
                write_atomic(p, &text).map_err(|e| e.to_string())?;
            }
        }
        inner.list = next;
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recorded_saved_and_cleared() {
        let dir = std::env::temp_dir().join(format!("swatlas-net-listening-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let file = dir.join("net-listening.jsonl");
        let st = NetPlaying {
            uuid: "u1".into(),
            name: "La 100".into(),
            itu: Some("ARG".into()),
            flag: Some("ar".into()),
            codec: Some("MP3".into()),
            bitrate: Some(128),
        };
        let svc = NetListeningService::new(Some(file.clone()));
        svc.set_dwell(10);
        let t0 = 1_781_049_600;
        for i in 0..5 {
            svc.observe(Some(&st), t0 + i * 5);
        }
        assert_eq!(svc.with(|l, open| (l.len(), open.map(|s| s.secs()))), (0, Some(20)));
        svc.close();
        assert_eq!(svc.with(|l, open| (l.len(), open.is_some())), (1, false));

        let again = NetListeningService::new(Some(file.clone()));
        assert_eq!(again.with(|l, _| l.iter().map(|s| s.uuid.clone()).collect::<Vec<_>>()), ["u1"]);
        assert_eq!(again.clear(None), Ok(1));
        assert_eq!(NetListeningService::new(Some(file)).with(|l, _| l.len()), 0);
        let _ = std::fs::remove_dir_all(dir);
    }
}

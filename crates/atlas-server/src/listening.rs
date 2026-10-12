//! The listening history, kept in `listening.jsonl` (one session per line, appended as each
//! ends) and recorded by `atlas_core::listening::Tracker` from the rig's states.

use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use atlas_core::listening::{clear_before, parse_lines, to_line, trim, Ctx, Event, Session, Tracker};
use atlas_core::rig::RigState;
use atlas_core::source::StationKey;

use crate::files::write_atomic;

pub struct ListeningService {
    /// `listening.jsonl`; `None` keeps the history in memory only (tests).
    file: Option<PathBuf>,
    inner: Mutex<Inner>,
}

struct Inner {
    /// Ended sessions, oldest first.
    list: Vec<Session>,
    tracker: Tracker,
}

impl ListeningService {
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
        Arc::new(Self { file, inner: Mutex::new(Inner { list, tracker: Tracker::default() }) })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().expect("listening lock")
    }

    /// Seconds on a frequency before it counts (tests shorten it).
    pub fn set_dwell(&self, secs: i64) {
        self.lock().tracker.dwell_s = secs;
    }

    /// Take the rig's state at `now`; a session that ends is saved.
    pub fn observe(&self, st: &RigState, scanning: bool, now: i64, ctx: &Ctx) {
        let mut inner = self.lock();
        for e in inner.tracker.observe(st, scanning, now, ctx) {
            if let Event::Closed(s) = e {
                self.keep(&mut inner, s);
            }
        }
    }

    pub fn hint(&self, key: StationKey, ctx: &Ctx, now: i64) {
        self.lock().tracker.hint(key, ctx, now);
    }

    /// End and save the session under way (shutting down).
    pub fn close(&self) {
        let mut inner = self.lock();
        if let Some(s) = inner.tracker.close() {
            self.keep(&mut inner, s);
        }
    }

    fn keep(&self, inner: &mut Inner, s: Session) {
        let line = to_line(&s);
        inner.list.push(s);
        if trim(&mut inner.list) {
            self.save(&inner.list);
        } else if let Some(p) = &self.file {
            let append = || -> std::io::Result<()> {
                if let Some(dir) = p.parent() {
                    std::fs::create_dir_all(dir)?;
                }
                std::fs::OpenOptions::new().create(true).append(true).open(p)?.write_all(line.as_bytes())
            };
            if let Err(e) = append() {
                tracing::warn!("could not save a listening session to {}: {e}", p.display());
            }
        }
    }

    /// Write the whole history again.
    fn save(&self, list: &[Session]) {
        if let Some(p) = &self.file {
            let text: String = list.iter().map(to_line).collect();
            if let Err(e) = write_atomic(p, &text) {
                tracing::warn!("could not save the listening history to {}: {e}", p.display());
            }
        }
    }

    /// Run `f` on the ended sessions (oldest first) and the one under way up to `now`.
    pub fn with<T>(&self, now: i64, f: impl FnOnce(&[Session], Option<&Session>) -> T) -> T {
        let inner = self.lock();
        let open = inner.tracker.open(now);
        f(&inner.list, open.as_ref())
    }

    /// Forget the sessions that ended before `before`, or all of them; how many went.
    pub fn clear(&self, before: Option<i64>) -> Result<usize, String> {
        let mut inner = self.lock();
        let mut next = inner.list.clone();
        let n = clear_before(&mut next, before);
        if let Some(p) = &self.file {
            let text: String = next.iter().map(to_line).collect();
            write_atomic(p, &text).map_err(|e| e.to_string())?;
        }
        inner.list = next;
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_core::rig::{Link, Power};

    fn on(hz: u32) -> RigState {
        RigState {
            link: Link::Sim,
            power: Power::On,
            freq_hz: Some(hz),
            mode: Some("AM".into()),
            passband_hz: None,
            strength_db: Some(-12),
            cw_pitch_hz: None,
        }
    }

    #[test]
    fn recorded_saved_and_cleared() {
        let dir = std::env::temp_dir().join(format!("swatlas-listening-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let file = dir.join("listening.jsonl");
        let cat = crate::sources::bundled_cell().get();
        let ctx = Ctx { cat: &cat, qth: atlas_core::geo::LatLon { lat: 0.0, lon: 0.0 } };
        let svc = ListeningService::new(Some(file.clone()));
        svc.set_dwell(10);
        let t0 = 1_781_049_600;
        for (i, hz) in [6_070_000, 6_070_000, 6_070_000, 9_500_000, 9_500_000, 9_500_000].iter().enumerate() {
            svc.observe(&on(*hz), false, t0 + i as i64 * 5, &ctx);
        }
        assert_eq!(svc.with(t0 + 30, |l, open| (l.len(), open.is_some())), (1, true));
        svc.close();
        assert_eq!(svc.with(t0 + 30, |l, open| (l.len(), open.is_some())), (2, false));

        // read back, with a bad line skipped
        let mut text = std::fs::read_to_string(&file).unwrap();
        text.push_str("{oops\n");
        std::fs::write(&file, text).unwrap();
        let again = ListeningService::new(Some(file.clone()));
        let ids = again.with(0, |l, _| l.iter().map(|s| s.freq_hz).collect::<Vec<_>>());
        assert_eq!(ids, [6_070_000, 9_500_000]);
        assert_eq!(again.clear(Some(t0 + 15)), Ok(1));
        assert_eq!(ListeningService::new(Some(file.clone())).with(0, |l, _| l.len()), 1);
        assert_eq!(again.clear(None), Ok(1));
        assert_eq!(ListeningService::new(Some(file)).with(0, |l, _| l.len()), 0);
        let _ = std::fs::remove_dir_all(dir);
    }
}

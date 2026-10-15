//! The NET band's listening history: what the listener played from the internet directory. It is
//! kept apart from the rig's (`listening`): its own sessions and file, its own totals in Stats,
//! and never part of the DX figures (distance, rarity, propagation, the heat strip). A session
//! counts once a stream has played for `DWELL_S`, rides out a reconnect of up to `GAP_S`, and ends
//! when the listener stops, plays another station, or the stream stays silent.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::listening::{DWELL_S, GAP_S, MAX_SESSIONS};
use crate::net::NetPlaying;
use crate::stats::{day_start, local_day, Period, StatsQuery};

/// `v` of the lines this build writes.
pub const FORMAT_VERSION: u32 = 1;
/// Stations the Stats card lists.
const TOP: usize = 5;

/// One stretch of listening to one internet station.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct NetSession {
    /// `1791648000-<uuid>`: when it started and the station.
    pub id: String,
    #[ts(type = "number")]
    pub start: i64,
    #[ts(type = "number")]
    pub end: i64,
    /// The directory's `stationuuid`.
    pub uuid: String,
    pub name: String,
    #[serde(default)]
    pub itu: Option<String>,
    #[serde(default)]
    pub flag: Option<String>,
    #[serde(default)]
    pub codec: Option<String>,
}

impl NetSession {
    pub fn secs(&self) -> i64 {
        (self.end - self.start).max(0)
    }

    fn of(st: &NetPlaying, start: i64, end: i64) -> Self {
        NetSession {
            id: format!("{start}-{}", st.uuid),
            start,
            end,
            uuid: st.uuid.clone(),
            name: st.name.clone(),
            itu: st.itu.clone(),
            flag: st.flag.clone(),
            codec: st.codec.clone(),
        }
    }
}

/// One line of `net-listening.jsonl`.
#[derive(Serialize, Deserialize)]
struct Line {
    v: u32,
    #[serde(flatten)]
    session: NetSession,
}

/// Read `net-listening.jsonl`: the sessions in the order written, and how many lines were bad.
pub fn parse_lines(text: &str) -> (Vec<NetSession>, usize) {
    let mut bad = 0;
    let mut out = vec![];
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<Line>(line) {
            Ok(l) => out.push(l.session),
            Err(_) => bad += 1,
        }
    }
    (out, bad)
}

/// A session as one line (with its newline).
pub fn to_line(s: &NetSession) -> String {
    let mut line = serde_json::to_string(&Line { v: FORMAT_VERSION, session: s.clone() }).expect("session serialises");
    line.push('\n');
    line
}

/// Keep at most `MAX_SESSIONS`, dropping the oldest; `true` if any went.
pub fn trim(list: &mut Vec<NetSession>) -> bool {
    let over = list.len().saturating_sub(MAX_SESSIONS);
    list.drain(..over);
    over > 0
}

/// Forget the sessions that ended before `before`, or all of them (`None`); how many went.
pub fn clear_before(list: &mut Vec<NetSession>, before: Option<i64>) -> usize {
    let n = list.len();
    match before {
        Some(t) => list.retain(|s| s.end >= t),
        None => list.clear(),
    }
    n - list.len()
}

struct Open {
    station: NetPlaying,
    start: i64,
    /// Last time its audio was going out.
    last: i64,
}

/// Turns "this station's audio is going out now" into sessions.
pub struct NetTracker {
    open: Option<Open>,
    /// Seconds of play before it counts (tests shorten it).
    pub dwell_s: i64,
}

impl Default for NetTracker {
    fn default() -> Self {
        Self { open: None, dwell_s: DWELL_S }
    }
}

impl NetTracker {
    /// `playing`: the station whose audio is going out at `now`, if any. Returns the session that
    /// ended, if it lasted long enough to count.
    pub fn observe(&mut self, playing: Option<&NetPlaying>, now: i64) -> Option<NetSession> {
        match (&mut self.open, playing) {
            (Some(o), Some(p)) if o.station.uuid == p.uuid => {
                o.last = now;
                None
            }
            (_, Some(p)) => {
                let ended = self.close();
                self.open = Some(Open { station: p.clone(), start: now, last: now });
                ended
            }
            (Some(o), None) if now - o.last > GAP_S => self.close(),
            _ => None,
        }
    }

    /// End the session under way (stopping, shutting down), if it counts.
    pub fn close(&mut self) -> Option<NetSession> {
        let o = self.open.take()?;
        let s = NetSession::of(&o.station, o.start, o.last);
        (s.secs() >= self.dwell_s).then_some(s)
    }

    /// The session under way, once it counts.
    pub fn open(&self) -> Option<NetSession> {
        let o = self.open.as_ref()?;
        (o.last - o.start >= self.dwell_s).then(|| NetSession::of(&o.station, o.start, o.last))
    }
}

/// One station in the NET totals.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct NetShare {
    pub uuid: String,
    pub name: String,
    pub itu: Option<String>,
    pub flag: Option<String>,
    pub seconds: u32,
    pub sessions: u32,
}

/// Result of `Call::NetStats`: internet listening in a period, apart from the DX figures.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct NetStats {
    pub period: Period,
    /// Where the period starts.
    #[ts(type = "number")]
    pub from: i64,
    pub seconds: u32,
    pub sessions: u32,
    pub stations: u32,
    pub countries: u32,
    /// The stations listened to longest, most first.
    pub top: Vec<NetShare>,
    pub now: Option<NetSession>,
}

/// The NET totals for `q`'s period, in the same windows as the rig's Stats.
pub fn net_stats(closed: &[NetSession], open: Option<&NetSession>, q: &StatsQuery, now: i64) -> NetStats {
    let all: Vec<&NetSession> = closed.iter().chain(open).collect();
    let from = match q.period.days() {
        Some(n) => day_start(local_day(now, q.tz_offset_min) - (n - 1), q.tz_offset_min),
        None => all.iter().map(|s| s.start).min().unwrap_or(now).min(now),
    };
    let overlap = |s: &NetSession| (s.end.min(now) - s.start.max(from)).max(0);
    let mut by: HashMap<&str, NetShare> = HashMap::new();
    let mut countries = HashSet::new();
    let (mut seconds, mut sessions) = (0i64, 0u32);
    for s in all.iter().filter(|s| overlap(s) > 0) {
        let secs = overlap(s);
        seconds += secs;
        sessions += 1;
        if let Some(c) = &s.itu {
            countries.insert(c.as_str());
        }
        let share = by.entry(&s.uuid).or_insert_with(|| NetShare {
            uuid: s.uuid.clone(),
            name: s.name.clone(),
            itu: s.itu.clone(),
            flag: s.flag.clone(),
            seconds: 0,
            sessions: 0,
        });
        share.seconds = share.seconds.saturating_add(secs.min(i64::from(u32::MAX)) as u32);
        share.sessions += 1;
    }
    let stations = by.len() as u32;
    let mut top: Vec<NetShare> = by.into_values().collect();
    top.sort_by(|a, b| b.seconds.cmp(&a.seconds).then_with(|| a.name.cmp(&b.name)));
    top.truncate(TOP);
    NetStats {
        period: q.period,
        from,
        seconds: seconds.clamp(0, i64::from(u32::MAX)) as u32,
        sessions,
        stations,
        countries: countries.len() as u32,
        top,
        now: open.cloned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(uuid: &str, itu: &str) -> NetPlaying {
        NetPlaying {
            uuid: uuid.into(),
            name: format!("Radio {uuid}"),
            itu: Some(itu.into()),
            flag: None,
            codec: Some("MP3".into()),
            bitrate: Some(128),
        }
    }

    const T0: i64 = 1_781_049_600;

    #[test]
    fn sessions_count_after_the_dwell() {
        let (a, b) = (st("a", "ARG"), st("b", "ROU"));
        let mut t = NetTracker::default();
        assert_eq!(t.observe(Some(&a), T0), None);
        assert_eq!(t.open(), None, "not yet");
        assert_eq!(t.observe(Some(&a), T0 + 40), None);
        assert_eq!(t.open().map(|s| s.secs()), Some(40));
        // a reconnect shorter than the gap keeps the session
        assert_eq!(t.observe(None, T0 + 50), None);
        assert_eq!(t.observe(Some(&a), T0 + 55), None);
        // another station: the first ends where it was last heard
        let s = t.observe(Some(&b), T0 + 60).unwrap();
        assert_eq!((s.id.as_str(), s.start, s.end, s.uuid.as_str()), ("1781049600-a", T0, T0 + 55, "a"));
        // too short to count, then silent past the gap
        assert_eq!(t.observe(None, T0 + 70), None);
        assert_eq!(t.observe(None, T0 + 90), None, "b played 0 s: nothing kept");
        assert_eq!(t.open(), None);
        t.observe(Some(&a), T0 + 100);
        t.observe(Some(&a), T0 + 200);
        assert_eq!(t.close().map(|s| s.secs()), Some(100));
        assert_eq!(t.close(), None);
    }

    #[test]
    fn lines_round_trip() {
        let s = NetSession::of(&st("a", "ARG"), T0, T0 + 60);
        let text = format!("{}{{oops\n\n{}", to_line(&s), to_line(&s));
        let (list, bad) = parse_lines(&text);
        assert_eq!((list.len(), bad), (2, 1));
        assert_eq!(list[0], s);
        assert!(to_line(&s).starts_with(r#"{"v":1,"id":"1781049600-a""#));
        let mut list = list;
        list[1].end = T0 + 500;
        assert_eq!(clear_before(&mut list, Some(T0 + 100)), 1);
        assert_eq!(clear_before(&mut list, None), 1);
    }

    #[test]
    fn totals_in_the_period() {
        let day = 86_400;
        let now = T0 + 12 * 3600;
        let list = vec![
            NetSession::of(&st("a", "ARG"), now - 10 * day, now - 10 * day + 600),
            NetSession::of(&st("a", "ARG"), now - 3600, now - 1800),
            NetSession::of(&st("b", "ROU"), now - 1800, now - 1200),
        ];
        let open = NetSession::of(&st("c", "ARG"), now - 300, now);
        let week = net_stats(&list, Some(&open), &StatsQuery { period: Period::Week, tz_offset_min: 0 }, now);
        assert_eq!((week.seconds, week.sessions, week.stations, week.countries), (1800 + 600 + 300, 3, 3, 2));
        assert_eq!(week.top.iter().map(|s| s.uuid.as_str()).collect::<Vec<_>>(), ["a", "b", "c"]);
        assert_eq!(week.now.as_ref().map(|s| s.uuid.as_str()), Some("c"));
        let all = net_stats(&list, None, &StatsQuery { period: Period::All, tz_offset_min: 0 }, now);
        assert_eq!((all.seconds, all.sessions, all.from), (2400 + 600, 3, now - 10 * day));
        assert_eq!(all.top[0].seconds, 2400);
        let empty = net_stats(&[], None, &StatsQuery::default(), now);
        assert_eq!((empty.seconds, empty.top.len(), empty.now), (0, 0, None));
    }
}

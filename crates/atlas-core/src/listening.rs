//! The listening history: what the listener stayed on, and for how long. The rig is a sensor
//! here too: a session starts once it has been on one frequency for `DWELL_S` (not scanning),
//! and ends when it moves, goes off or loses the link for longer than `GAP_S`. Each session
//! keeps a copy of the station heard (`StationRef`, as favorites do), so the history outlives
//! the schedule version it was recorded under.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::collect::rarity;
use crate::eibi::names::band_of;
use crate::geo::{dark_share, sun_at, LatLon, SunPhase};
use crate::rig::{Link, Power, RigState};
use crate::scan::StationRef;
use crate::schedule::any_on;
use crate::source::StationKey;
use crate::stations::{Catalog, LOOKUP_TOLERANCE_HZ};

/// Seconds on one frequency before it counts as listening.
pub const DWELL_S: i64 = 30;
/// A rig that stops answering for up to this long (a missed poll) keeps its session.
pub const GAP_S: i64 = 20;
/// Most sessions kept; the oldest go first.
pub const MAX_SESSIONS: usize = 100_000;
/// `ListeningLine::v` of what this build writes.
pub const FORMAT_VERSION: u32 = 1;

/// The Sun along the path when the session started.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Sky {
    /// At the listener.
    pub rx: SunPhase,
    /// At the transmitter.
    pub tx: SunPhase,
    /// Share of the path in the dark (0..=1).
    pub dark: f32,
    /// It was morning at the transmitter (its twilight is dawn, not dusk).
    #[serde(default)]
    pub tx_morning: bool,
}

impl Sky {
    /// Either end was in twilight: the grey line.
    pub fn grey_line(&self) -> bool {
        self.rx == SunPhase::Twilight || self.tx == SunPhase::Twilight
    }
}

/// One stretch of listening on one frequency.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Session {
    /// `1791648000-5905000`: when it started and the frequency.
    pub id: String,
    /// Unix seconds.
    #[ts(type = "number")]
    pub start: i64,
    #[ts(type = "number")]
    pub end: i64,
    pub freq_hz: u32,
    /// As the rig reports it.
    #[serde(default)]
    pub mode: Option<String>,
    /// The station heard; `None`: nothing on the schedule was on the air there.
    #[serde(default)]
    pub station: Option<StationRef>,
    /// The listener chose the station (not the best guess).
    #[serde(default)]
    pub picked: bool,
    /// From the listener's QTH at the time.
    #[serde(default)]
    pub km: Option<f32>,
    /// Broadcast band id (`49m`), or `oob`.
    pub band: String,
    /// S-meter, dB over S9: the best reading and the mean.
    #[serde(default)]
    pub best_db: Option<i16>,
    #[serde(default)]
    pub avg_db: Option<i16>,
    #[serde(default)]
    pub sky: Option<Sky>,
    /// 1 (on the air all the time) to 5 (under an hour a week).
    #[serde(default)]
    pub rarity: Option<u8>,
}

impl Session {
    pub fn secs(&self) -> i64 {
        (self.end - self.start).max(0)
    }

    /// The station's country, if identified.
    pub fn country(&self) -> Option<&str> {
        self.station.as_ref().map(|s| s.itu.as_str()).filter(|c| !c.is_empty())
    }
}

/// One line of `listening.jsonl`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ListeningLine {
    pub v: u32,
    #[serde(flatten)]
    pub session: Session,
}

/// Read `listening.jsonl`: the sessions in the order written, and how many lines were bad.
pub fn parse_lines(text: &str) -> (Vec<Session>, usize) {
    let mut bad = 0;
    let mut out = vec![];
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<ListeningLine>(line) {
            Ok(l) => out.push(l.session),
            Err(_) => bad += 1,
        }
    }
    (out, bad)
}

/// A session as one line of `listening.jsonl` (with its newline).
pub fn to_line(s: &Session) -> String {
    let mut line =
        serde_json::to_string(&ListeningLine { v: FORMAT_VERSION, session: s.clone() }).expect("session serialises");
    line.push('\n');
    line
}

/// Keep at most `MAX_SESSIONS`, dropping the oldest; `true` if any went.
pub fn trim(list: &mut Vec<Session>) -> bool {
    let over = list.len().saturating_sub(MAX_SESSIONS);
    list.drain(..over);
    over > 0
}

/// Forget the sessions that ended before `before`, or all of them (`None`); how many went.
pub fn clear_before(list: &mut Vec<Session>, before: Option<i64>) -> usize {
    let n = list.len();
    match before {
        Some(t) => list.retain(|s| s.end >= t),
        None => list.clear(),
    }
    n - list.len()
}

fn near(a: u32, b: u32) -> bool {
    a.abs_diff(b) <= LOOKUP_TOLERANCE_HZ
}

/// What the tracker needs to say who is on the air.
pub struct Ctx<'a> {
    pub cat: &'a Catalog,
    pub qth: LatLon,
}

/// A session opening or ending.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Opened(Session),
    Closed(Session),
}

#[derive(Clone, Debug)]
struct Open {
    session: Session,
    /// Last time the rig answered on this frequency.
    seen: i64,
    sum_db: i64,
    n_db: i64,
    /// Its station was on the air at the last look: when that stops, the session splits. A
    /// station picked while off the air never splits it.
    on: bool,
}

impl Open {
    fn new(session: Session, ctx: &Ctx, now: i64) -> Self {
        let on = on_air(&session, ctx, now);
        Self { session, seen: now, sum_db: 0, n_db: 0, on }
    }
}

fn on_air(s: &Session, ctx: &Ctx, now: i64) -> bool {
    s.station.as_ref().and_then(|r| r.find(ctx.cat)).is_some_and(|st| any_on(&st.slots, now))
}

#[derive(Clone, Debug)]
enum State {
    Idle,
    /// On `freq_hz` since `since`, not long enough yet.
    Pending { freq_hz: u32, since: i64, seen: i64 },
    Open(Box<Open>),
}

/// Turns rig states into sessions. No IO: the server feeds it every state and a tick.
#[derive(Clone, Debug)]
pub struct Tracker {
    pub dwell_s: i64,
    pub gap_s: i64,
    state: State,
    /// The station the listener chose, for the session there.
    hint: Option<StationKey>,
}

impl Default for Tracker {
    fn default() -> Self {
        Self::new(DWELL_S, GAP_S)
    }
}

/// The station on the air at `freq_hz` (the hinted one if it is a candidate), with the path.
fn attribute(ctx: &Ctx, freq_hz: u32, now: i64, hint: Option<&StationKey>) -> Attribution {
    let cands = ctx.cat.lookup(freq_hz, now, ctx.qth, LOOKUP_TOLERANCE_HZ);
    let hinted = hint.and_then(|h| cands.iter().find(|c| c.station.source == h.source && c.station.key == h.key));
    let Some(c) = hinted.or_else(|| cands.iter().find(|c| c.air.on)) else {
        return Attribution::default();
    };
    let rx = ctx.qth;
    let tx = sun_at(c.site.pos(), now);
    Attribution {
        station: Some(StationRef::of(ctx.cat, &c.station, now)),
        picked: hinted.is_some(),
        km: Some(c.route.km as f32),
        sky: Some(Sky {
            rx: sun_at(rx, now).phase,
            tx: tx.phase,
            dark: dark_share(rx, c.site.pos(), now),
            tx_morning: tx.solar_min < 720,
        }),
        rarity: Some(rarity(&c.station.slots)),
    }
}

#[derive(Default)]
struct Attribution {
    station: Option<StationRef>,
    picked: bool,
    km: Option<f32>,
    sky: Option<Sky>,
    rarity: Option<u8>,
}

impl Attribution {
    fn apply(self, s: &mut Session) {
        s.station = self.station;
        s.picked = self.picked;
        s.km = self.km;
        s.sky = self.sky;
        s.rarity = self.rarity;
    }
}

impl Tracker {
    pub fn new(dwell_s: i64, gap_s: i64) -> Self {
        Self { dwell_s, gap_s, state: State::Idle, hint: None }
    }

    /// The session under way, up to `now`.
    pub fn open(&self, now: i64) -> Option<Session> {
        match &self.state {
            State::Open(o) => {
                let mut s = o.session.clone();
                s.end = s.end.max(now.min(o.seen + self.gap_s));
                Some(s)
            }
            _ => None,
        }
    }

    fn start(&self, ctx: &Ctx, freq_hz: u32, mode: Option<String>, start: i64, now: i64) -> Session {
        let mut s = Session {
            id: format!("{start}-{freq_hz}"),
            start,
            end: now,
            freq_hz,
            mode,
            station: None,
            picked: false,
            km: None,
            band: band_of(freq_hz).to_string(),
            best_db: None,
            avg_db: None,
            sky: None,
            rarity: None,
        };
        let hint = self.hint.as_ref().filter(|h| ctx.cat.by_key(&h.source, &h.key).is_some_and(|st| near(st.freq_hz, freq_hz)));
        attribute(ctx, freq_hz, now, hint).apply(&mut s);
        s
    }

    /// Take a rig state at `now`. `scanning`: a band scan has the rig.
    pub fn observe(&mut self, st: &RigState, scanning: bool, now: i64, ctx: &Ctx) -> Vec<Event> {
        let mut out = vec![];
        let live = matches!(st.link, Link::Ok | Link::Sim) && st.power == Power::On && !scanning;
        // the rig said it is off, or something else has it: no grace
        let gone = scanning || st.power == Power::Off || st.link == Link::Off;
        let freq = st.freq_hz.filter(|_| live);
        let state = std::mem::replace(&mut self.state, State::Idle);
        self.state = match (state, freq) {
            (State::Idle, Some(f)) => State::Pending { freq_hz: f, since: now, seen: now },
            (State::Idle, None) => State::Idle,
            (State::Pending { freq_hz, since, .. }, Some(f)) if near(f, freq_hz) => {
                if now - since >= self.dwell_s {
                    let mut o = Open::new(self.start(ctx, freq_hz, st.mode.clone(), since, now), ctx, now);
                    take_db(&mut o, st);
                    out.push(Event::Opened(o.session.clone()));
                    State::Open(Box::new(o))
                } else {
                    State::Pending { freq_hz, since, seen: now }
                }
            }
            (State::Pending { .. }, Some(f)) => {
                self.keep_hint_for(ctx, f);
                State::Pending { freq_hz: f, since: now, seen: now }
            }
            (State::Pending { freq_hz, since, seen }, None) => {
                if gone || now - seen > self.gap_s {
                    State::Idle
                } else {
                    State::Pending { freq_hz, since, seen }
                }
            }
            (State::Open(mut o), Some(f)) if near(f, o.session.freq_hz) => {
                o.session.end = now;
                o.seen = now;
                if o.session.mode.is_none() {
                    o.session.mode = st.mode.clone();
                }
                take_db(&mut o, st);
                // its station went off the air: a new session from here, for whoever is on now
                let on = on_air(&o.session, ctx, now);
                if o.on && !on {
                    let freq_hz = o.session.freq_hz;
                    out.push(Event::Closed(o.session));
                    self.hint = None;
                    let mut n = Open::new(self.start(ctx, freq_hz, st.mode.clone(), now, now), ctx, now);
                    take_db(&mut n, st);
                    out.push(Event::Opened(n.session.clone()));
                    State::Open(Box::new(n))
                } else {
                    o.on = on;
                    State::Open(o)
                }
            }
            (State::Open(o), Some(f)) => {
                out.push(Event::Closed(o.session));
                self.keep_hint_for(ctx, f);
                State::Pending { freq_hz: f, since: now, seen: now }
            }
            (State::Open(o), None) => {
                if gone || now - o.seen > self.gap_s {
                    out.push(Event::Closed(o.session));
                    self.hint = None;
                    State::Idle
                } else {
                    State::Open(o)
                }
            }
        };
        out
    }

    /// The rig moved to `f`: a pick for a station there stays (the pick and the tune are sent
    /// together, and the rig gets there after), any other goes.
    fn keep_hint_for(&mut self, ctx: &Ctx, f: u32) {
        let there = self.hint.as_ref().and_then(|h| ctx.cat.by_key(&h.source, &h.key)).is_some_and(|st| near(st.freq_hz, f));
        if !there {
            self.hint = None;
        }
    }

    /// The listener chose `key` as the station they hear: the session there (or the one about
    /// to start) is credited to it, if it is on that frequency.
    pub fn hint(&mut self, key: StationKey, ctx: &Ctx, now: i64) {
        let freq = ctx.cat.by_key(&key.source, &key.key).map(|s| s.freq_hz);
        self.hint = Some(key);
        if let (State::Open(o), Some(f)) = (&mut self.state, freq) {
            if near(f, o.session.freq_hz) {
                let a = attribute(ctx, o.session.freq_hz, now, self.hint.as_ref());
                if a.picked {
                    a.apply(&mut o.session);
                    o.on = on_air(&o.session, ctx, now);
                }
            }
        }
    }

    /// The session under way, closed now (shutting down).
    pub fn close(&mut self) -> Option<Session> {
        match std::mem::replace(&mut self.state, State::Idle) {
            State::Open(o) => Some(o.session),
            _ => None,
        }
    }
}

fn take_db(o: &mut Open, st: &RigState) {
    if let Some(db) = st.strength_db {
        let db = db.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
        o.session.best_db = Some(o.session.best_db.map_or(db, |b| b.max(db)));
        o.sum_db += i64::from(db);
        o.n_db += 1;
        o.session.avg_db = Some((o.sum_db as f64 / o.n_db as f64).round() as i16);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::eibi::{parse, tests::files};

    pub(crate) fn catalog() -> Catalog {
        Catalog::from_eibi(parse(&files()).unwrap())
    }

    pub(crate) const QTH: LatLon = LatLon { lat: -33.45, lon: -70.67 };

    /// 2026-06-10 00:00 UTC, a Wednesday.
    pub(crate) const T0: i64 = 1_781_049_600;

    pub(crate) fn on(hz: u32, db: Option<i32>) -> RigState {
        RigState {
            link: Link::Sim,
            power: Power::On,
            freq_hz: Some(hz),
            mode: Some("AM".into()),
            passband_hz: None,
            strength_db: db,
            cw_pitch_hz: None,
        }
    }

    /// Feed `st` every 5 s from `from` to `to` (both included).
    fn feed(t: &mut Tracker, ctx: &Ctx, st: &RigState, from: i64, to: i64) -> Vec<Event> {
        (from..=to).step_by(5).flat_map(|now| t.observe(st, false, now, ctx)).collect()
    }

    #[test]
    fn dwell_then_a_session_until_the_rig_moves() {
        let cat = catalog();
        let ctx = Ctx { cat: &cat, qth: QTH };
        let mut t = Tracker::default();
        // 25 s is not listening
        assert!(feed(&mut t, &ctx, &on(6_070_000, Some(-20)), T0, T0 + 25).is_empty());
        let ev = feed(&mut t, &ctx, &on(6_070_000, Some(-10)), T0 + 30, T0 + 30);
        let [Event::Opened(s)] = ev.as_slice() else { panic!("{ev:?}") };
        assert_eq!((s.start, s.freq_hz, s.band.as_str()), (T0, 6_070_000, "49m"));
        let st = s.station.as_ref().unwrap();
        assert_eq!((st.name.as_str(), st.itu.as_str(), s.picked), ("China Radio Int.", "CHN", false));
        assert!(s.km.unwrap() > 15_000.0 && s.sky.is_some() && s.rarity == Some(1), "{s:?}");
        // fine tuning inside the tolerance keeps it; the S-meter is averaged
        feed(&mut t, &ctx, &on(6_070_500, Some(-30)), T0 + 35, T0 + 60);
        assert_eq!(t.open(T0 + 60).unwrap().end, T0 + 60);
        let ev = t.observe(&on(9_410_000, None), false, T0 + 65, &ctx);
        let [Event::Closed(s)] = ev.as_slice() else { panic!("{ev:?}") };
        assert_eq!((s.end, s.best_db), (T0 + 60, Some(-10)));
        assert!(s.avg_db.unwrap() < -20, "{s:?}");
        assert!(t.open(T0 + 65).is_none());
    }

    #[test]
    fn scans_power_and_gaps() {
        let cat = catalog();
        let ctx = Ctx { cat: &cat, qth: QTH };
        let mut t = Tracker::new(10, 20);
        feed(&mut t, &ctx, &on(6_070_000, None), T0, T0 + 10);
        assert!(t.open(T0 + 10).is_some());
        // a scan takes the rig: closed at once
        let ev = t.observe(&on(6_070_000, None), true, T0 + 15, &ctx);
        assert!(matches!(ev.as_slice(), [Event::Closed(s)] if s.end == T0 + 10), "{ev:?}");
        // scanning never counts
        assert!((0..20).all(|i| t.observe(&on(6_070_000, None), true, T0 + 20 + i * 5, &ctx).is_empty()));

        // a dropout shorter than the gap keeps it; a longer one ends it where it was last seen
        let mut t = Tracker::new(10, 20);
        feed(&mut t, &ctx, &on(6_070_000, None), T0, T0 + 10);
        let down = RigState::down();
        assert!(feed(&mut t, &ctx, &down, T0 + 15, T0 + 30).is_empty());
        assert!(feed(&mut t, &ctx, &on(6_070_000, None), T0 + 35, T0 + 40).is_empty());
        let ev = feed(&mut t, &ctx, &down, T0 + 45, T0 + 65);
        assert!(matches!(ev.as_slice(), [Event::Closed(s)] if s.end == T0 + 40), "{ev:?}");

        // turned off: closed at once
        let mut t = Tracker::new(10, 20);
        feed(&mut t, &ctx, &on(6_070_000, None), T0, T0 + 10);
        let off = RigState::without_rig(Link::Sim, Power::Off);
        assert!(matches!(t.observe(&off, false, T0 + 12, &ctx).as_slice(), [Event::Closed(_)]));
    }

    #[test]
    fn a_station_going_off_the_air_splits_the_session() {
        let cat = catalog();
        let ctx = Ctx { cat: &cat, qth: QTH };
        // Radio Bulgaria, 5910 kHz 14:30-15:00
        let at = T0 + 14 * 3600 + 50 * 60;
        let mut t = Tracker::new(10, 20);
        feed(&mut t, &ctx, &on(5_910_000, None), at, at + 10);
        assert_eq!(t.open(at + 10).unwrap().station.unwrap().name, "Radio Bulgaria");
        let ev = feed(&mut t, &ctx, &on(5_910_000, None), at + 15, at + 10 * 60 + 5);
        let [Event::Closed(a), Event::Opened(b)] = ev.as_slice() else { panic!("{ev:?}") };
        assert_eq!((a.end, b.start), (T0 + 15 * 3600, T0 + 15 * 3600));
        assert!(b.station.is_none(), "nothing else there: unidentified");
    }

    #[test]
    fn the_listeners_pick_wins() {
        let cat = catalog();
        let ctx = Ctx { cat: &cat, qth: QTH };
        // 05:30: BBC on 9410 and 9411 kHz, both within the tolerance of 9410
        let at = T0 + 5 * 3600 + 30 * 60;
        let mut t = Tracker::new(10, 20);
        feed(&mut t, &ctx, &on(9_410_000, None), at, at + 10);
        let s = t.open(at + 10).unwrap();
        let guess = s.station.as_ref().unwrap().freq_hz;
        assert!(!s.picked);
        let other = cat.stations().find(|s| (s.freq_hz == 9_410_000 || s.freq_hz == 9_411_000) && s.freq_hz != guess).unwrap();
        t.hint(other.station_key(), &ctx, at + 12);
        let s = t.open(at + 12).unwrap();
        assert_eq!((s.station.as_ref().map(|s| s.freq_hz), s.picked), (Some(other.freq_hz), true));
        // a station on another frequency changes nothing
        let far = cat.stations().find(|s| s.freq_hz == 6_070_000).unwrap();
        t.hint(far.station_key(), &ctx, at + 14);
        assert_eq!(t.open(at + 14).unwrap().station.unwrap().freq_hz, other.freq_hz);
        // a hint given before the session starts is used when it does
        let mut t = Tracker::new(10, 20);
        t.observe(&on(9_410_000, None), false, at, &ctx);
        t.hint(other.station_key(), &ctx, at + 2);
        feed(&mut t, &ctx, &on(9_410_000, None), at + 5, at + 10);
        assert!(t.open(at + 10).unwrap().picked);
        assert!(t.close().is_some() && t.close().is_none());
    }

    #[test]
    fn a_pick_sent_with_the_tune_survives_the_rig_catching_up() {
        let cat = catalog();
        let ctx = Ctx { cat: &cat, qth: QTH };
        let at = T0 + 5 * 3600 + 30 * 60;
        let target = cat.stations().find(|s| s.freq_hz == 9_411_000).unwrap();
        // listening to CRI on 6070; the listener picks BBC 9411: the pick arrives first
        let mut t = Tracker::new(10, 20);
        feed(&mut t, &ctx, &on(6_070_000, None), at, at + 10);
        t.hint(target.station_key(), &ctx, at + 11);
        assert!(!t.open(at + 11).unwrap().picked, "not for the session on 6070");
        let ev = feed(&mut t, &ctx, &on(9_410_000, None), at + 12, at + 22);
        assert!(matches!(ev.as_slice(), [Event::Closed(_), Event::Opened(_)]), "{ev:?}");
        let s = t.open(at + 22).unwrap();
        assert_eq!((s.station.unwrap().freq_hz, s.picked), (9_411_000, true));
        // a pick for elsewhere is dropped when the rig goes somewhere else again
        t.hint(target.station_key(), &ctx, at + 23);
        feed(&mut t, &ctx, &on(6_070_000, None), at + 25, at + 40);
        assert!(!t.open(at + 40).unwrap().picked);
    }

    #[test]
    fn a_station_picked_off_the_air_does_not_split() {
        let cat = catalog();
        let ctx = Ctx { cat: &cat, qth: QTH };
        // Radio Bulgaria (14:30-15:00) picked at 16:00 on its frequency
        let at = T0 + 16 * 3600;
        let rb = cat.stations().find(|s| s.name == "Radio Bulgaria").unwrap();
        let mut t = Tracker::new(10, 20);
        t.hint(rb.station_key(), &ctx, at);
        let ev = feed(&mut t, &ctx, &on(5_910_000, None), at, at + 120);
        assert_eq!(ev.len(), 1, "{ev:?}");
        let s = t.open(at + 120).unwrap();
        assert_eq!((s.station.unwrap().name.as_str(), s.picked, s.start), ("Radio Bulgaria", true, at));
    }

    #[test]
    fn lines_and_clearing() {
        let cat = catalog();
        let ctx = Ctx { cat: &cat, qth: QTH };
        let mut t = Tracker::new(0, 20);
        feed(&mut t, &ctx, &on(6_070_000, Some(3)), T0, T0 + 5);
        let s = t.close().unwrap();
        let text = format!("{}not json\n\n{}", to_line(&s), to_line(&Session { id: "x".into(), end: T0 + 99_999, ..s.clone() }));
        let (list, bad) = parse_lines(&text);
        assert_eq!((list.len(), bad, &list[0]), (2, 1, &s));
        // an older line without the optional fields still reads
        let (old, bad) = parse_lines(r#"{"v":1,"id":"a","start":1,"end":2,"freq_hz":9000000,"band":"oob"}"#);
        assert_eq!((old.len(), bad, old[0].station.is_none()), (1, 0, true));

        let mut l = list.clone();
        assert_eq!(clear_before(&mut l, Some(T0 + 1000)), 1);
        assert_eq!(l[0].id, "x");
        assert_eq!(clear_before(&mut l, None), 1);
        assert!(l.is_empty());
        let mut many = vec![s; MAX_SESSIONS + 3];
        assert!(trim(&mut many) && many.len() == MAX_SESSIONS);
    }
}

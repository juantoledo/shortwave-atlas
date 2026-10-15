//! Timings of the core's heavy calls on the bundled schedule and a large synthetic listening
//! history. Not a pass/fail test: run it by hand, in release, to compare before and after a
//! change:
//!
//! `cargo test --release -p atlas-server --test perf -- --ignored --nocapture`

use std::collections::HashSet;
use std::time::{Duration, Instant};

use atlas_core::geo::LatLon;
use atlas_core::hunt::hunt;
use atlas_core::lens::{self, Known};
use atlas_core::listening::{parse_lines, to_line, Session};
use atlas_core::stations::{Catalog, Lens, StationQuery, LOOKUP_TOLERANCE_HZ};
use atlas_core::stats::{freq_heat, listened_ids, stats, Period, StatsQuery};
use atlas_server::sources::{bundled, catalog};

/// 2026-06-10 12:00 UTC, inside the bundled A26 season.
const NOW: i64 = 1_781_092_800;
const QTH: LatLon = LatLon { lat: 40.4, lon: -3.7 };

fn cat() -> Catalog {
    let eibi = bundled();
    catalog(&eibi, &atlas_core::user_stations::to_source(&[], &eibi.gazetteer))
}

/// Run `f` `n` times; print the mean and the slowest.
fn time<T>(label: &str, n: u32, mut f: impl FnMut() -> T) {
    let mut worst = Duration::ZERO;
    let t0 = Instant::now();
    for _ in 0..n {
        let t = Instant::now();
        std::hint::black_box(f());
        worst = worst.max(t.elapsed());
    }
    let mean = t0.elapsed() / n;
    println!("{label:<44} mean {:>9.3} ms   worst {:>9.3} ms", ms(mean), ms(worst));
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1e3
}

/// `n` sessions of 10 minutes, one an hour back from `NOW`, on the catalog's stations in turn.
fn history(cat: &Catalog, n: usize) -> Vec<Session> {
    let stations: Vec<_> = cat.stations().collect();
    let mut out: Vec<Session> = (0..n)
        .map(|i| {
            let st = stations[(i * 7919) % stations.len()];
            let site = cat.site(st.site);
            let start = NOW - (n - i) as i64 * 3600;
            serde_json::from_value(serde_json::json!({
                "id": format!("{start}-{}", st.freq_hz),
                "start": start,
                "end": start + 600,
                "freq_hz": st.freq_hz,
                "mode": "AM",
                "station": {
                    "source": st.source, "key": st.key, "freq_hz": st.freq_hz, "name": st.name,
                    "itu": st.itu, "flag": st.flag,
                    "site_name": site.map(|s| s.name.clone()).unwrap_or_default(),
                    "site_itu": site.map(|s| s.itu.clone()).unwrap_or_default(),
                    "lang": "E", "target": "Eu",
                },
                "picked": false,
                "km": 1500.0,
                "band": "31m",
                "best_db": -10,
                "avg_db": -20,
            }))
            .expect("a session")
        })
        .collect();
    out.sort_by_key(|s| s.start);
    out
}

#[test]
#[ignore = "timings; run by hand in release"]
fn perf_probe() {
    let t = Instant::now();
    let cat = cat();
    println!("{:<44} {:>14.3} ms ({} rows)", "parse + Catalog::build", ms(t.elapsed()), cat.len());

    // every frequency of the schedule once: the dial after a tune
    let freqs: Vec<u32> = {
        let mut f: Vec<u32> = cat.stations().map(|s| s.freq_hz).collect();
        f.dedup();
        f
    };
    let mut worst = (Duration::ZERO, 0);
    let t0 = Instant::now();
    for &hz in &freqs {
        let t = Instant::now();
        std::hint::black_box(cat.lookup(hz, NOW, QTH, LOOKUP_TOLERANCE_HZ));
        worst = worst.max((t.elapsed(), hz));
    }
    println!(
        "{:<44} mean {:>9.3} ms   worst {:>9.3} ms (at {} kHz)",
        format!("lookup ({} frequencies)", freqs.len()),
        ms(t0.elapsed() / freqs.len() as u32),
        ms(worst.0),
        worst.1 / 1000
    );

    let none = HashSet::new();
    let known = Known { logbook: &[], signals: &[], favorites: &[], listened: &none, scan: None };
    let q = |lens, facets, limit| StationQuery { lens, facets, limit, ..Default::default() };
    time("search: first page with facets", 50, || lens::search(&cat, &q(Lens::All, true, 100), NOW, &known));
    time("search: a page of 200, no facets", 50, || lens::search(&cat, &q(Lens::All, false, 200), NOW, &known));
    time("search: \"radio\" with facets", 50, || {
        lens::search(&cat, &StationQuery { q: "radio".into(), facets: true, ..Default::default() }, NOW, &known)
    });
    time("overview", 20, || cat.overview(NOW));

    for n in [1_000, 100_000] {
        let list = history(&cat, n);
        let refs: Vec<&Session> = list.iter().collect();
        let q = StatsQuery { period: Period::Week, tz_offset_min: 60 };
        time(&format!("stats (week), {n} sessions"), 10, || stats(&list, None, &q, NOW, &cat));
        time(&format!("hunt, {n} sessions"), 10, || hunt(&cat, &refs, &[], NOW, QTH, &[]));
        time(&format!("listened_ids, {n} sessions"), 10, || listened_ids(list.iter(), &cat));
        time(&format!("freq_heat, {n} sessions"), 10, || freq_heat(list.iter()));
        // what startup does with listening.jsonl
        let text: String = list.iter().map(to_line).collect();
        time(&format!("parse listening.jsonl, {n} lines ({} KB)", text.len() / 1024), 5, || parse_lines(&text));
    }
}

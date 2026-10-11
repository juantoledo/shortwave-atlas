//! Favorites: frequencies the listener keeps to come back to, each with its mode, a label and
//! the station heard there. The station is a copy (`StationRef`), not a `Station::id`, so a
//! favorite outlives the schedule version it was saved under; `resolve` finds it again.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::rig::Mode;
use crate::scan::StationRef;
use crate::stations::Catalog;

/// `FavoritesFile::version` of what this build writes.
pub const FORMAT_VERSION: u32 = 1;
/// Most favorites one may keep, and the longest label (characters).
pub const MAX_FAVORITES: usize = 500;
pub const MAX_LABEL: usize = 60;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Favorite {
    /// `1791648000-5905000`: when it was added and the frequency.
    pub id: String,
    pub freq_hz: u32,
    pub mode: Mode,
    pub label: String,
    /// The station on the air there when it was saved, if one was chosen.
    pub station: Option<StationRef>,
    /// Unix seconds.
    #[ts(type = "number")]
    pub added: i64,
}

/// `favorites.json`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FavoritesFile {
    pub version: u32,
    pub favorites: Vec<Favorite>,
}

/// Argument of `Call::AddFavorite`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct FavoriteRequest {
    pub freq_hz: u32,
    pub mode: Mode,
    /// `None` = the station's name (or nothing, without a station).
    pub label: Option<String>,
    /// `Station::id` in the running catalog.
    pub station_id: Option<u32>,
}

/// A favorite and its station in the running catalog (result of `Call::Favorites`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct FavoriteView {
    pub favorite: Favorite,
    /// `None`: saved without a station, or the schedule no longer has it.
    pub station_id: Option<u32>,
}

fn clean_label(label: &str) -> Result<String, String> {
    let label = label.trim();
    if label.chars().count() > MAX_LABEL {
        return Err(format!("A label has at most {MAX_LABEL} characters"));
    }
    Ok(label.to_string())
}

/// A favorite's id: when it was added and the frequency, made unique among `list`.
fn fav_id(list: &[Favorite], added: i64, freq_hz: u32) -> String {
    let base = format!("{added}-{freq_hz}");
    (1..)
        .map(|n| if n == 1 { base.clone() } else { format!("{base}-{n}") })
        .find(|id| !list.iter().any(|f| &f.id == id))
        .expect("some suffix is free")
}

/// Save `req` in `list` and return it. The same frequency, mode and station again updates
/// that favorite (its label, when one is given) instead of adding another.
pub fn add(list: &mut Vec<Favorite>, req: &FavoriteRequest, cat: &Catalog, now: i64) -> Result<Favorite, String> {
    if req.freq_hz == 0 {
        return Err("No frequency to keep".into());
    }
    let label = req.label.as_deref().map(clean_label).transpose()?;
    let station = match req.station_id {
        Some(id) => {
            let st = cat.station(id).ok_or("That station is not in the schedule")?;
            Some(StationRef::of(cat, st, now))
        }
        None => None,
    };
    let same = |f: &Favorite| {
        f.freq_hz == req.freq_hz
            && f.mode == req.mode
            && match (&f.station, &station) {
                (Some(a), Some(b)) => a.same(b),
                (None, None) => true,
                _ => false,
            }
    };
    if let Some(f) = list.iter_mut().find(|f| same(f)) {
        if let Some(label) = label {
            f.label = label;
        }
        f.station = station.or(f.station.take());
        return Ok(f.clone());
    }
    if list.len() >= MAX_FAVORITES {
        return Err(format!("There are already {MAX_FAVORITES} favorites"));
    }
    let label = label.unwrap_or_else(|| station.as_ref().map(|s| s.name.clone()).unwrap_or_default());
    let fav = Favorite {
        id: fav_id(list, now, req.freq_hz),
        freq_hz: req.freq_hz,
        mode: req.mode,
        label,
        station,
        added: now,
    };
    list.push(fav.clone());
    Ok(fav)
}

/// Relabel favorite `id`; `false` if there is none.
pub fn rename(list: &mut [Favorite], id: &str, label: &str) -> Result<bool, String> {
    let label = clean_label(label)?;
    Ok(list.iter_mut().find(|f| f.id == id).map(|f| f.label = label).is_some())
}

/// Forget favorite `id`; `false` if there is none.
pub fn remove(list: &mut Vec<Favorite>, id: &str) -> bool {
    let before = list.len();
    list.retain(|f| f.id != id);
    list.len() != before
}

/// Favorites against the running catalog: by station id where the schedule still has their
/// station, and the others (no station, or one it no longer has).
pub struct Resolved<'a> {
    pub by_station: HashMap<u32, &'a Favorite>,
    pub off: Vec<&'a Favorite>,
}

pub fn resolve<'a>(list: &'a [Favorite], cat: &Catalog) -> Resolved<'a> {
    let mut r = Resolved { by_station: HashMap::new(), off: vec![] };
    for f in list {
        match f.station.as_ref().and_then(|s| s.find(cat)) {
            Some(st) => {
                r.by_station.entry(st.id).or_insert(f);
            }
            None => r.off.push(f),
        }
    }
    r
}

/// Every favorite with its station id, by frequency.
pub fn views(list: &[Favorite], cat: &Catalog) -> Vec<FavoriteView> {
    let mut out: Vec<FavoriteView> = list
        .iter()
        .map(|f| FavoriteView {
            favorite: f.clone(),
            station_id: f.station.as_ref().and_then(|s| s.find(cat)).map(|st| st.id),
        })
        .collect();
    out.sort_by(|a, b| {
        a.favorite.freq_hz.cmp(&b.favorite.freq_hz).then_with(|| a.favorite.added.cmp(&b.favorite.added))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eibi::{parse, tests::files};

    fn catalog() -> Catalog {
        Catalog::from_eibi(parse(&files()).unwrap())
    }

    fn req(freq_hz: u32, station_id: Option<u32>, label: Option<&str>) -> FavoriteRequest {
        FavoriteRequest { freq_hz, mode: Mode::AM, label: label.map(Into::into), station_id }
    }

    #[test]
    fn adding_twice_updates_the_first() {
        let c = catalog();
        let st = c.station(0).unwrap();
        let mut list = vec![];
        let a = add(&mut list, &req(st.freq_hz, Some(st.id), None), &c, 100).unwrap();
        assert_eq!((a.label.as_str(), a.id.clone()), (st.name.as_str(), format!("100-{}", st.freq_hz)));
        assert_eq!(a.station.as_ref().map(|s| s.name.as_str()), Some(st.name.as_str()));
        let again = add(&mut list, &req(st.freq_hz, Some(st.id), Some("  Evening news ")), &c, 200).unwrap();
        assert_eq!((list.len(), again.id, again.label.as_str()), (1, a.id.clone(), "Evening news"));
        // without a label the one there stays
        add(&mut list, &req(st.freq_hz, Some(st.id), None), &c, 300).unwrap();
        assert_eq!(list[0].label, "Evening news");
        // the same frequency without the station, or in another mode, is another favorite
        let bare = add(&mut list, &req(st.freq_hz, None, None), &c, 100).unwrap();
        assert_eq!((bare.label.as_str(), bare.id.as_str()), ("", format!("100-{}-2", st.freq_hz).as_str()));
        add(&mut list, &FavoriteRequest { mode: Mode::USB, ..req(st.freq_hz, None, None) }, &c, 400).unwrap();
        assert_eq!(list.len(), 3);
    }

    #[test]
    fn requests_are_checked() {
        let c = catalog();
        let mut list = vec![];
        assert!(add(&mut list, &req(0, None, None), &c, 0).is_err());
        assert!(add(&mut list, &req(5_000_000, None, Some(&"x".repeat(MAX_LABEL + 1))), &c, 0).is_err());
        assert!(add(&mut list, &req(5_000_000, Some(u32::MAX), None), &c, 0).is_err());
        let ok = "é".repeat(MAX_LABEL);
        assert!(add(&mut list, &req(5_000_000, None, Some(&ok)), &c, 0).is_ok(), "characters, not bytes");
        for i in 1..MAX_FAVORITES as u32 {
            add(&mut list, &req(5_000_000 + i, None, None), &c, 0).unwrap();
        }
        assert!(add(&mut list, &req(4_000_000, None, None), &c, 0).unwrap_err().contains("500"));
        assert!(add(&mut list, &req(5_000_000, None, Some("still")), &c, 0).is_ok(), "an update still fits");
    }

    #[test]
    fn rename_and_remove() {
        let c = catalog();
        let mut list = vec![];
        let f = add(&mut list, &req(9_580_000, None, None), &c, 5).unwrap();
        assert_eq!(rename(&mut list, &f.id, " Utility "), Ok(true));
        assert_eq!(list[0].label, "Utility");
        assert_eq!(rename(&mut list, "nope", "x"), Ok(false));
        assert!(rename(&mut list, &f.id, &"x".repeat(MAX_LABEL + 1)).is_err());
        assert!(remove(&mut list, &f.id));
        assert!(!remove(&mut list, &f.id));
        assert!(list.is_empty());
    }

    #[test]
    fn favorites_outlive_the_schedule() {
        let c = catalog();
        let (a, b) = (c.station(0).unwrap(), c.station(3).unwrap());
        let mut list = vec![];
        add(&mut list, &req(b.freq_hz, Some(b.id), None), &c, 1).unwrap();
        add(&mut list, &req(a.freq_hz, Some(a.id), None), &c, 2).unwrap();
        add(&mut list, &req(7_000_000, None, Some("Net")), &c, 3).unwrap();
        // next season the station moved: the favorite stays, without a row
        let mut moved = list[1].clone();
        moved.id = "moved".into();
        moved.station.as_mut().unwrap().freq_hz += 5_000;
        list.push(moved);
        let r = resolve(&list, &c);
        assert_eq!(r.by_station.len(), 2);
        assert_eq!(r.by_station[&a.id].id, list[1].id);
        let off: Vec<&str> = r.off.iter().map(|f| f.id.as_str()).collect();
        assert_eq!(off, ["3-7000000", "moved"]);
        let v = views(&list, &c);
        assert!(v.windows(2).all(|w| w[0].favorite.freq_hz <= w[1].favorite.freq_hz));
        assert_eq!(v.iter().find(|v| v.favorite.id == "moved").unwrap().station_id, None);
        assert_eq!(v.iter().find(|v| v.favorite.id == list[1].id).unwrap().station_id, Some(a.id));
    }

    #[test]
    fn json() {
        let c = catalog();
        let mut list = vec![];
        add(&mut list, &req(c.station(0).unwrap().freq_hz, Some(0), None), &c, 1).unwrap();
        let file = FavoritesFile { version: FORMAT_VERSION, favorites: list };
        let back: FavoritesFile = serde_json::from_str(&serde_json::to_string(&file).unwrap()).unwrap();
        assert_eq!(back, file);
    }
}

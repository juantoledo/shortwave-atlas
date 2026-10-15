//! The NET band's favorites: internet stations the listener keeps, which are also its memory
//! channels (numbered in the order they were added). Kept apart from the radio favorites, which
//! are by frequency and mode: an internet station has neither. Each keeps a copy of what the
//! directory said of the station, so it shows even if the directory no longer lists it.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::favorites::{clean_label, MAX_FAVORITES};
use crate::net::NetStation;

/// `NetFavoritesFile::version` of what this build writes.
pub const FORMAT_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct NetFavorite {
    /// `<added>-<uuid>`.
    pub id: String,
    /// The directory's `stationuuid`.
    pub uuid: String,
    pub name: String,
    pub itu: Option<String>,
    pub flag: Option<String>,
    pub codec: Option<String>,
    /// The listener's name for it; empty: the station's.
    pub label: String,
    #[ts(type = "number")]
    pub added: i64,
}

/// `net-favorites.json`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NetFavoritesFile {
    pub version: u32,
    pub favorites: Vec<NetFavorite>,
}

/// Keep `st`, or update the favorite already kept for it: its copy of the station, and its label
/// if one is given.
pub fn add(list: &mut Vec<NetFavorite>, st: &NetStation, label: Option<&str>, now: i64) -> Result<NetFavorite, String> {
    let label = label.map(clean_label).transpose()?;
    if let Some(f) = list.iter_mut().find(|f| f.uuid == st.uuid) {
        f.name = st.name.clone();
        f.itu = st.itu.clone();
        f.flag = st.flag.clone();
        f.codec = st.codec.clone();
        if let Some(l) = label {
            f.label = l;
        }
        return Ok(f.clone());
    }
    if list.len() >= MAX_FAVORITES {
        return Err(format!("At most {MAX_FAVORITES} internet favorites"));
    }
    let f = NetFavorite {
        id: format!("{now}-{}", st.uuid),
        uuid: st.uuid.clone(),
        name: st.name.clone(),
        itu: st.itu.clone(),
        flag: st.flag.clone(),
        codec: st.codec.clone(),
        label: label.unwrap_or_default(),
        added: now,
    };
    list.push(f.clone());
    Ok(f)
}

/// `Ok(false)`: no such favorite.
pub fn rename(list: &mut [NetFavorite], id: &str, label: &str) -> Result<bool, String> {
    let label = clean_label(label)?;
    Ok(list.iter_mut().find(|f| f.id == id).map(|f| f.label = label).is_some())
}

pub fn remove(list: &mut Vec<NetFavorite>, id: &str) -> bool {
    let n = list.len();
    list.retain(|f| f.id != id);
    list.len() < n
}

/// In memory-channel order: as added, oldest first.
pub fn views(list: &[NetFavorite]) -> Vec<NetFavorite> {
    let mut v = list.to_vec();
    v.sort_by(|a, b| a.added.cmp(&b.added).then_with(|| a.id.cmp(&b.id)));
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(uuid: &str) -> NetStation {
        NetStation {
            uuid: uuid.into(),
            name: format!("Radio {uuid}"),
            itu: Some("ARG".into()),
            flag: Some("ar".into()),
            langs: vec![],
            tags: vec![],
            codec: Some("MP3".into()),
            bitrate: Some(128),
            hls: false,
            homepage: None,
            ok: true,
            votes: 0,
            playable: true,
            pos: None,
        }
    }

    #[test]
    fn kept_once_per_station_in_channel_order() {
        let mut list = vec![];
        let b = add(&mut list, &st("b"), None, 200).unwrap();
        let a = add(&mut list, &st("a"), Some("  Mitre  "), 100).unwrap();
        assert_eq!((b.id.as_str(), b.label.as_str(), a.label.as_str()), ("200-b", "", "Mitre"));
        assert_eq!(views(&list).iter().map(|f| f.uuid.as_str()).collect::<Vec<_>>(), ["a", "b"]);
        // again: updated, not doubled; the label stays unless given
        let renamed = NetStation { name: "Radio B (new)".into(), ..st("b") };
        let again = add(&mut list, &renamed, None, 300).unwrap();
        assert_eq!((list.len(), again.id.as_str(), again.name.as_str()), (2, "200-b", "Radio B (new)"));
        assert_eq!(rename(&mut list, "200-b", "B"), Ok(true));
        assert_eq!(rename(&mut list, "nope", "B"), Ok(false));
        assert!(rename(&mut list, "200-b", &"x".repeat(61)).is_err());
        assert_eq!(list[0].label, "B");
        assert!(remove(&mut list, "200-b"));
        assert!(!remove(&mut list, "200-b"));
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn capped() {
        let mut list: Vec<NetFavorite> =
            (0..MAX_FAVORITES).map(|i| add(&mut vec![], &st(&i.to_string()), None, 0).unwrap()).collect();
        assert!(add(&mut list, &st("more"), None, 1).is_err());
        assert!(add(&mut list, &st("0"), Some("kept"), 1).is_ok(), "an update still goes through");
    }
}

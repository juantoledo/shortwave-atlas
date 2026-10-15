//! The NET band's favorites (its memory channels), kept in `net-favorites.json`, apart from the
//! radio favorites.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use atlas_core::api::{ApiError, ErrorKind};
use atlas_core::net::NetStation;
use atlas_core::net_favorites::{self, NetFavorite, NetFavoritesFile, FORMAT_VERSION};

use crate::files::write_atomic;

pub struct NetFavoriteService {
    /// `net-favorites.json`; `None` keeps them in memory only (tests).
    file: Option<PathBuf>,
    list: Mutex<Vec<NetFavorite>>,
}

fn invalid(message: impl Into<String>) -> ApiError {
    ApiError { kind: ErrorKind::Invalid, message: message.into() }
}

impl NetFavoriteService {
    /// Load the saved favorites; a file that does not read gives none (and a warning).
    pub fn new(file: Option<PathBuf>) -> Arc<Self> {
        let list = file
            .as_ref()
            .and_then(|p| {
                let s = std::fs::read_to_string(p).ok()?;
                serde_json::from_str::<NetFavoritesFile>(&s)
                    .map_err(|e| tracing::warn!("ignoring internet favorites {}: {e}", p.display()))
                    .ok()
            })
            .map(|f| f.favorites)
            .unwrap_or_default();
        Arc::new(Self { file, list: Mutex::new(list) })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<NetFavorite>> {
        self.list.lock().expect("net favorites lock")
    }

    /// In memory-channel order.
    pub fn views(&self) -> Vec<NetFavorite> {
        net_favorites::views(&self.lock())
    }

    /// Apply `change` to a copy of the list, save that, and only then keep it.
    fn change<T>(&self, change: impl FnOnce(&mut Vec<NetFavorite>) -> Result<T, ApiError>) -> Result<T, ApiError> {
        let mut list = self.lock();
        let mut next = list.clone();
        let out = change(&mut next)?;
        if let Some(p) = &self.file {
            let file = NetFavoritesFile { version: FORMAT_VERSION, favorites: next.clone() };
            let json = serde_json::to_string_pretty(&file).expect("favorites serialise");
            write_atomic(p, &json)
                .map_err(|e| ApiError::internal(format!("Could not save the internet favorites: {e}")))?;
        }
        *list = next;
        Ok(out)
    }

    pub fn add(&self, st: &NetStation, label: Option<&str>, now: i64) -> Result<NetFavorite, ApiError> {
        self.change(|l| net_favorites::add(l, st, label, now).map_err(invalid))
    }

    pub fn rename(&self, id: &str, label: &str) -> Result<(), ApiError> {
        self.change(|l| match net_favorites::rename(l, id, label) {
            Ok(true) => Ok(()),
            Ok(false) => Err(invalid("No such favorite")),
            Err(e) => Err(invalid(e)),
        })
    }

    pub fn delete(&self, id: &str) -> Result<(), ApiError> {
        self.change(|l| if net_favorites::remove(l, id) { Ok(()) } else { Err(invalid("No such favorite")) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_and_loaded() {
        let dir = std::env::temp_dir().join(format!("swatlas-net-favs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let file = dir.join("net-favorites.json");
        let st = NetStation {
            uuid: "u1".into(),
            name: "Radio Mitre".into(),
            itu: Some("ARG".into()),
            flag: Some("ar".into()),
            langs: vec![],
            tags: vec![],
            codec: Some("AAC+".into()),
            bitrate: Some(64),
            hls: false,
            homepage: None,
            ok: true,
            votes: 0,
            playable: true,
            pos: None,
        };
        let svc = NetFavoriteService::new(Some(file.clone()));
        let f = svc.add(&st, None, 100).unwrap();
        svc.rename(&f.id, "Mitre").unwrap();
        assert_eq!(svc.rename("nope", "x").unwrap_err().kind, ErrorKind::Invalid);
        let again = NetFavoriteService::new(Some(file.clone()));
        assert_eq!(again.views().iter().map(|f| f.label.as_str()).collect::<Vec<_>>(), ["Mitre"]);
        again.delete(&f.id).unwrap();
        assert!(NetFavoriteService::new(Some(file)).views().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }
}

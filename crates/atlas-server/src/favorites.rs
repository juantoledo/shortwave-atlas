//! Favorites, kept in `favorites.json` and matched to the running catalog.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use atlas_core::api::{ApiError, ErrorKind};
use atlas_core::favorites::{self, Favorite, FavoriteRequest, FavoriteView, FavoritesFile, FORMAT_VERSION};
use atlas_core::stations::CatalogCell;

use crate::files::write_atomic;

pub struct FavoriteService {
    /// `favorites.json`; `None` keeps them in memory only (tests).
    file: Option<PathBuf>,
    catalog: Arc<CatalogCell>,
    list: Mutex<Vec<Favorite>>,
}

fn invalid(message: impl Into<String>) -> ApiError {
    ApiError { kind: ErrorKind::Invalid, message: message.into() }
}

impl FavoriteService {
    /// Load the saved favorites; a file that does not read gives none (and a warning).
    pub fn new(file: Option<PathBuf>, catalog: Arc<CatalogCell>) -> Arc<Self> {
        let list = file
            .as_ref()
            .and_then(|p| {
                let s = std::fs::read_to_string(p).ok()?;
                serde_json::from_str::<FavoritesFile>(&s)
                    .map_err(|e| tracing::warn!("ignoring favorites {}: {e}", p.display()))
                    .ok()
            })
            .map(|f| f.favorites)
            .unwrap_or_default();
        Arc::new(Self { file, catalog, list: Mutex::new(list) })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<Favorite>> {
        self.list.lock().expect("favorites lock")
    }

    pub fn list(&self) -> Vec<Favorite> {
        self.lock().clone()
    }

    pub fn views(&self) -> Vec<FavoriteView> {
        favorites::views(&self.lock(), &self.catalog.get())
    }

    /// Apply `change` to a copy of the list, save that, and only then keep it.
    fn change<T>(&self, change: impl FnOnce(&mut Vec<Favorite>) -> Result<T, ApiError>) -> Result<T, ApiError> {
        let mut list = self.lock();
        let mut next = list.clone();
        let out = change(&mut next)?;
        if let Some(p) = &self.file {
            let file = FavoritesFile { version: FORMAT_VERSION, favorites: next.clone() };
            let json = serde_json::to_string_pretty(&file).expect("favorites serialise");
            write_atomic(p, &json).map_err(|e| ApiError::internal(format!("Could not save the favorites: {e}")))?;
        }
        *list = next;
        Ok(out)
    }

    pub fn add(&self, req: &FavoriteRequest, now: i64) -> Result<Favorite, ApiError> {
        let cat = self.catalog.get();
        self.change(|l| {
            favorites::add(l, req, &cat, now).map_err(|e| match e.as_str() {
                favorites::STALE => ApiError { kind: ErrorKind::Conflict, message: e },
                _ => invalid(e),
            })
        })
    }

    pub fn rename(&self, id: &str, label: &str) -> Result<(), ApiError> {
        self.change(|l| match favorites::rename(l, id, label) {
            Ok(true) => Ok(()),
            Ok(false) => Err(invalid("No such favorite")),
            Err(e) => Err(invalid(e)),
        })
    }

    pub fn delete(&self, id: &str) -> Result<(), ApiError> {
        self.change(|l| if favorites::remove(l, id) { Ok(()) } else { Err(invalid("No such favorite")) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_core::rig::Mode;

    #[test]
    fn saved_and_loaded() {
        let dir = std::env::temp_dir().join(format!("swatlas-favs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let file = dir.join("favorites.json");
        let svc = FavoriteService::new(Some(file.clone()), crate::sources::bundled_cell());
        let req = FavoriteRequest {
            freq_hz: 7_000_000,
            mode: Mode::LSB,
            label: Some("Net".into()),
            station_id: None,
            generation: None,
        };
        let f = svc.add(&req, 10).unwrap();
        svc.add(&FavoriteRequest { freq_hz: 3_000_000, ..req.clone() }, 11).unwrap();
        svc.rename(&f.id, "Morning net").unwrap();
        assert_eq!(svc.rename("nope", "x").unwrap_err().kind, ErrorKind::Invalid);
        assert_eq!(svc.add(&FavoriteRequest { freq_hz: 0, ..req }, 12).unwrap_err().kind, ErrorKind::Invalid);

        let again = FavoriteService::new(Some(file.clone()), crate::sources::bundled_cell());
        let v = again.views();
        assert_eq!(v.iter().map(|v| v.favorite.freq_hz).collect::<Vec<_>>(), [3_000_000, 7_000_000]);
        assert_eq!(v[1].favorite.label, "Morning net");
        again.delete(&f.id).unwrap();
        assert!(again.delete(&f.id).is_err());
        assert_eq!(FavoriteService::new(Some(file.clone()), crate::sources::bundled_cell()).list().len(), 1);
        assert!(!dir.read_dir().unwrap().flatten().any(|e| e.path().to_string_lossy().ends_with(".tmp")));

        std::fs::write(&file, "{").unwrap();
        assert!(FavoriteService::new(Some(file), crate::sources::bundled_cell()).list().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }
}

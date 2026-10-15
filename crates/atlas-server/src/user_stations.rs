//! The listener's own stations, kept in `user-stations.json`. Each change builds the catalog
//! again from the sources and puts it in the running cell, so every lookup, list and the globe
//! see it at once.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use atlas_core::api::{ApiError, ErrorKind};
use atlas_core::eibi::EibiSource;
use atlas_core::source::{Gazetteer, Vocabulary};
use atlas_core::stations::{CatalogCell, CodeTables};
use atlas_core::user_stations::{
    self, parse_file, to_source, ImportReport, UserStation, UserStationDraft, UserStationView, UserStationsFile,
    FORMAT_VERSION,
};

use crate::files::write_atomic;

pub struct UserStationService {
    /// `user-stations.json`; `None` keeps them in memory only (tests).
    file: Option<PathBuf>,
    eibi: Arc<EibiSource>,
    cell: Arc<CatalogCell>,
    list: Mutex<Vec<UserStation>>,
}

fn invalid(message: impl Into<String>) -> ApiError {
    ApiError { kind: ErrorKind::Invalid, message: message.into() }
}

/// The saved stations. A file that does not read as stations is set aside (`.bad-<now>`), so
/// that the next change does not write over what the listener typed in.
fn load(p: &Path, now: i64) -> Vec<UserStation> {
    let text = match std::fs::read_to_string(p) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return vec![],
        Err(e) => {
            tracing::warn!("could not read {}: {e}", p.display());
            return vec![];
        }
    };
    match parse_file(&text) {
        Ok(f) => f.stations,
        Err(e) => {
            let aside = p.with_extension(format!("json.bad-{now}"));
            tracing::warn!("{}: {e}; set aside as {}", p.display(), aside.display());
            if let Err(e) = std::fs::rename(p, &aside) {
                tracing::warn!("could not set {} aside: {e}", p.display());
            }
            vec![]
        }
    }
}

impl UserStationService {
    /// Load the saved stations and build the first catalog, with EiBi's.
    pub fn new(file: Option<PathBuf>, eibi: Arc<EibiSource>, now: i64) -> Arc<Self> {
        let list = file.as_deref().map(|p| load(p, now)).unwrap_or_default();
        let cell = CatalogCell::new(crate::sources::catalog(&eibi, &to_source(&list, &eibi.gazetteer)));
        Arc::new(Self { file, eibi, cell, list: Mutex::new(list) })
    }

    /// The running catalog.
    pub fn cell(&self) -> Arc<CatalogCell> {
        self.cell.clone()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<UserStation>> {
        self.list.lock().expect("user stations lock")
    }

    pub fn list(&self) -> Vec<UserStation> {
        self.lock().clone()
    }

    pub fn views(&self) -> Vec<UserStationView> {
        user_stations::views(&self.lock(), &self.cell.get(), &self.eibi.gazetteer)
    }

    fn view(&self, id: &str) -> Result<UserStationView, ApiError> {
        self.views().into_iter().find(|v| v.station.id == id).ok_or_else(|| ApiError::internal("station not kept"))
    }

    /// The transmitter sites known (internet stations are placed by country with them).
    pub fn gazetteer(&self) -> &Gazetteer {
        &self.eibi.gazetteer
    }

    pub fn code_tables(&self) -> CodeTables {
        self.cell.get().code_tables(&self.eibi.vocabulary, &self.eibi.gazetteer)
    }

    /// Apply `change` to a copy of the list, build the catalog with it, save it, and only then
    /// keep both. Slow (the whole catalog): call it off the async threads.
    fn change<T>(
        &self,
        change: impl FnOnce(&mut Vec<UserStation>, &Vocabulary, &Gazetteer) -> Result<T, ApiError>,
    ) -> Result<T, ApiError> {
        let mut list = self.lock();
        let mut next = list.clone();
        let out = change(&mut next, &self.eibi.vocabulary, &self.eibi.gazetteer)?;
        let t = Instant::now();
        let catalog = crate::sources::catalog(&self.eibi, &to_source(&next, &self.eibi.gazetteer));
        tracing::debug!("stations: catalog rebuilt in {} ms", t.elapsed().as_millis());
        if let Some(p) = &self.file {
            let file = UserStationsFile { version: FORMAT_VERSION, exported: None, stations: next.clone() };
            let json = serde_json::to_string_pretty(&file).expect("stations serialise");
            write_atomic(p, &json).map_err(|e| ApiError::internal(format!("Could not save the stations: {e}")))?;
        }
        self.cell.replace(catalog);
        *list = next;
        Ok(out)
    }

    pub fn add(&self, d: &UserStationDraft, now: i64) -> Result<UserStationView, ApiError> {
        let u = self.change(|l, v, g| user_stations::add(l, d, v, g, now).map_err(invalid))?;
        self.view(&u.id)
    }

    pub fn update(&self, id: &str, d: &UserStationDraft, now: i64) -> Result<UserStationView, ApiError> {
        self.change(|l, v, g| match user_stations::update(l, id, d, v, g, now) {
            Ok(Some(_)) => Ok(()),
            Ok(None) => Err(invalid("No such station")),
            Err(e) => Err(invalid(e)),
        })?;
        self.view(id)
    }

    pub fn delete(&self, id: &str) -> Result<(), ApiError> {
        self.change(|l, _, _| if user_stations::remove(l, id) { Ok(()) } else { Err(invalid("No such station")) })
    }

    pub fn export(&self, now: i64) -> String {
        user_stations::export(&self.lock(), now)
    }

    /// Bring an export in; a file that does not read changes nothing.
    pub fn import(&self, json: &str, replace: bool, now: i64) -> Result<ImportReport, ApiError> {
        let file = parse_file(json).map_err(invalid)?;
        self.change(|l, v, g| Ok(user_stations::import(l, &file, replace, v, g, now)))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use atlas_core::rig::Mode;
    use atlas_core::schedule::Days;
    use atlas_core::user_stations::{UserSite, UserSlot, MINE};

    pub(crate) fn draft(khz: u32, name: &str) -> UserStationDraft {
        UserStationDraft {
            freq_hz: khz * 1000,
            mode: Mode::USB,
            name: name.into(),
            itu: "CHL".into(),
            site: UserSite::Custom { itu: "CHL".into(), name: "Santiago".into(), lat: -33.45, lon: -70.67 },
            slots: vec![UserSlot { start: 0, end: 1440, days: Days::Daily, lang: "S".into(), target: String::new() }],
            notes: String::new(),
            url: None,
        }
    }

    #[test]
    fn changes_rebuild_the_catalog_and_are_saved() {
        let dir = std::env::temp_dir().join(format!("swatlas-mine-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let file = dir.join("user-stations.json");
        let svc = UserStationService::new(Some(file.clone()), crate::sources::bundled(), 0);
        let cell = svc.cell();
        let before = cell.get();
        let v = svc.add(&draft(6075, "Radio Local"), 10).unwrap();
        let cat = cell.get();
        assert_eq!((cat.generation(), cat.len()), (before.generation() + 1, before.len() + 1));
        let st = cat.station(v.station_id.unwrap()).unwrap();
        assert_eq!(
            (st.name.as_str(), st.source.as_str(), st.key.as_str()),
            ("Radio Local", MINE, v.station.id.as_str())
        );
        assert_eq!(svc.add(&draft(6075, ""), 11).unwrap_err().kind, ErrorKind::Invalid);
        assert_eq!(cell.get().generation(), cat.generation(), "a refused change builds nothing");

        let v = svc.update(&v.station.id, &draft(6080, "Radio Local FM"), 12).unwrap();
        assert_eq!(cell.get().station(v.station_id.unwrap()).unwrap().freq_hz, 6_080_000);
        assert_eq!(svc.update("nope", &draft(6080, "x"), 12).unwrap_err().kind, ErrorKind::Invalid);
        let tables = svc.code_tables();
        assert!(tables.countries.len() > 200 && !tables.sites.is_empty());

        // a new instance reads them back
        let again = UserStationService::new(Some(file.clone()), crate::sources::bundled(), 0);
        assert_eq!(
            again.views().iter().map(|v| v.station.station.name.as_str()).collect::<Vec<_>>(),
            ["Radio Local FM"]
        );
        assert!(again.cell().get().by_key(MINE, &v.station.id).is_some());
        again.delete(&v.station.id).unwrap();
        assert!(again.delete(&v.station.id).is_err());
        assert!(again.list().is_empty());
        assert!(!dir.read_dir().unwrap().flatten().any(|e| e.path().to_string_lossy().ends_with(".tmp")));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_file_that_does_not_read_is_set_aside() {
        let dir = std::env::temp_dir().join(format!("swatlas-mine-bad-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("user-stations.json");
        std::fs::write(&file, "{ not json").unwrap();
        let svc = UserStationService::new(Some(file.clone()), crate::sources::bundled(), 77);
        assert!(svc.list().is_empty());
        assert_eq!(std::fs::read_to_string(dir.join("user-stations.json.bad-77")).unwrap(), "{ not json");
        svc.add(&draft(6075, "Radio Local"), 1).unwrap();
        assert!(file.is_file());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn export_and_import() {
        let a = UserStationService::new(None, crate::sources::bundled(), 0);
        a.add(&draft(6075, "Radio Local"), 1).unwrap();
        a.add(&draft(7000, "Net"), 2).unwrap();
        let b = UserStationService::new(None, crate::sources::bundled(), 0);
        let gen = b.cell().get().generation();
        assert_eq!(b.import("{", false, 3).unwrap_err().kind, ErrorKind::Invalid);
        assert_eq!(b.cell().get().generation(), gen, "nothing changed");
        let r = b.import(&a.export(3), false, 3).unwrap();
        assert_eq!((r.added, r.updated, r.unchanged), (2, 0, 0));
        assert_eq!(b.views().len(), 2);
        assert_eq!(b.cell().get().meta(0).sources[1].rows, 2);
    }
}

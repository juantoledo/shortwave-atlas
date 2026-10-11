//! The data sources: the EiBi schedule bundled at build time (`data/eibi`), or the files in
//! `eibi_dir`, and the listener's own stations (`user_stations`); and the catalog built from
//! them.

use std::path::Path;
use std::sync::{Arc, OnceLock};
use std::time::Instant;

use atlas_core::eibi::{parse, EibiFiles, EibiSource};
use atlas_core::source::SourceData;
use atlas_core::stations::Catalog;
#[cfg(test)]
use atlas_core::stations::CatalogCell;

mod bundle {
    include!(concat!(env!("OUT_DIR"), "/eibi_bundle.rs"));
}

fn read_eibi(files: &EibiFiles, from: &str) -> Result<Arc<EibiSource>, String> {
    let t = Instant::now();
    let parsed = parse(files).map_err(|e| format!("{from}: {e}"))?;
    let r = &parsed.report;
    r.check_strict().map_err(|e| format!("{from}: {e}"))?;
    tracing::info!(
        "stations: {} {} ({}), {} rows kept of {}, {} sites, {} ms",
        from,
        r.source_file,
        r.season.as_deref().unwrap_or("season unknown"),
        r.kept,
        r.total_rows,
        r.sites,
        t.elapsed().as_millis()
    );
    for w in &r.warnings {
        tracing::debug!("stations: {w}");
    }
    Ok(Arc::new(parsed.into_source()))
}

/// The bundled schedule, parsed once per process.
pub fn bundled() -> Arc<EibiSource> {
    static EIBI: OnceLock<Arc<EibiSource>> = OnceLock::new();
    EIBI.get_or_init(|| {
        let files = EibiFiles {
            csv: bundle::CSV,
            csv_name: bundle::CSV_NAME,
            readme: bundle::README,
            overrides: bundle::OVERRIDES,
            utility: bundle::UTILITY,
        };
        read_eibi(&files, "bundled").expect("the bundled EiBi data is checked by the tests")
    })
    .clone()
}

/// Load the EiBi files from a directory laid out like `data/eibi`: `source/sked-*.csv` (exactly
/// one) and `source/README.TXT`, plus optional `site_overrides.csv` and `utility_patterns.txt`
/// (missing ones fall back to the bundled copies).
pub fn load(dir: &Path) -> Result<Arc<EibiSource>, String> {
    let src = dir.join("source");
    let mut csvs: Vec<_> = std::fs::read_dir(&src)
        .map_err(|e| format!("{}: {e}", src.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            let n = p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_ascii_lowercase();
            n.starts_with("sked-") && n.ends_with(".csv")
        })
        .collect();
    csvs.sort();
    let [csv_path] = csvs.as_slice() else {
        return Err(format!("{}: expected exactly one sked-*.csv, found {}", src.display(), csvs.len()));
    };
    let read = |p: &Path| std::fs::read(p).map_err(|e| format!("{}: {e}", p.display()));
    let optional = |name: &str, bundled: &str| -> Result<String, String> {
        match std::fs::read_to_string(dir.join(name)) {
            Ok(s) => Ok(s),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(bundled.to_string()),
            Err(e) => Err(format!("{}: {e}", dir.join(name).display())),
        }
    };
    let csv = read(csv_path)?;
    let readme = read(&src.join("README.TXT"))?;
    let overrides = optional("site_overrides.csv", bundle::OVERRIDES)?;
    let utility = optional("utility_patterns.txt", bundle::UTILITY)?;
    let csv_name = csv_path.file_name().and_then(|n| n.to_str()).unwrap_or("sked.csv");
    let files = EibiFiles { csv: &csv, csv_name, readme: &readme, overrides: &overrides, utility: &utility };
    read_eibi(&files, &dir.display().to_string())
}

/// The catalog of EiBi and the listener's stations (`mine`), with EiBi's code names and sites.
pub fn catalog(eibi: &EibiSource, mine: &SourceData) -> Catalog {
    Catalog::build(&[&eibi.data, mine], &eibi.vocabulary, &eibi.gazetteer)
}

/// The bundled schedule's catalog without stations of the listener's, in a cell of its own
/// (tests).
#[cfg(test)]
pub fn bundled_cell() -> Arc<CatalogCell> {
    let eibi = bundled();
    CatalogCell::new(catalog(&eibi, &atlas_core::user_stations::to_source(&[], &eibi.gazetteer)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_data_loads() {
        let s = bundled();
        assert!(Arc::ptr_eq(&s, &bundled()));
        let c = bundled_cell().get();
        assert!(c.len() > 1000, "{} rows", c.len());
        let ids: Vec<String> = c.meta(0).sources.into_iter().map(|s| s.id).collect();
        assert_eq!(ids, ["eibi", "mine"]);
    }

    #[test]
    fn a_directory_loads_and_falls_back_to_bundled_rules() {
        let dir = std::env::temp_dir().join(format!("swatlas-eibi-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("source")).unwrap();
        assert!(matches!(load(&dir), Err(e) if e.contains("exactly one")));

        std::fs::write(dir.join("source/README.TXT"), "   IV) Transmitter site codes.\n   CAN: Toronto 43N30-79W38\n")
            .unwrap();
        let csv =
            "kHz;Time(UTC);ITU;Station;Remarks;P\n6070;0000-2400;CAN;CFRX Toronto;;1\n9000;0000-0100;CAN;Volmet;;1\n";
        std::fs::write(dir.join("source/sked-b26.csv"), csv).unwrap();
        let eibi = load(&dir).unwrap_or_else(|e| panic!("{e}"));
        let c = catalog(&eibi, &atlas_core::user_stations::to_source(&[], &eibi.gazetteer));
        assert_eq!(c.len(), 1); // the bundled utility patterns drop the Volmet
        assert_eq!(c.meta(0).sources[0].season.as_deref(), Some("B26"));

        std::fs::write(dir.join("source/sked-b27.csv"), csv).unwrap();
        assert!(load(&dir).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}

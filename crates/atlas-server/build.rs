//! Bundles the EiBi schedule in `data/eibi` (see its README). The CSV's name changes every
//! season (`sked-a26.csv`, `sked-b26.csv`), so the `include_bytes!` lines are generated.

use std::path::{Path, PathBuf};

fn main() {
    let data = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/eibi");
    println!("cargo:rerun-if-changed={}", data.display());
    println!("cargo:rerun-if-changed={}", data.join("source").display());

    let mut csvs: Vec<PathBuf> = std::fs::read_dir(data.join("source"))
        .unwrap_or_else(|e| panic!("{}: {e}", data.join("source").display()))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_ascii_lowercase();
            name.starts_with("sked-") && name.ends_with(".csv")
        })
        .collect();
    csvs.sort();
    let [csv] = csvs.as_slice() else {
        panic!("data/eibi/source must hold exactly one sked-*.csv, found {csvs:?}");
    };
    let path = |p: &Path| {
        let p = p.canonicalize().unwrap_or_else(|e| panic!("{}: {e}", p.display()));
        println!("cargo:rerun-if-changed={}", p.display());
        format!("{:?}", p.display().to_string())
    };
    let name = csv.file_name().unwrap().to_str().unwrap();
    let code = format!(
        "pub const CSV_NAME: &str = {name:?};\n\
         pub const CSV: &[u8] = include_bytes!({});\n\
         pub const README: &[u8] = include_bytes!({});\n\
         pub const OVERRIDES: &str = include_str!({});\n\
         pub const UTILITY: &str = include_str!({});\n",
        path(csv),
        path(&data.join("source/README.TXT")),
        path(&data.join("site_overrides.csv")),
        path(&data.join("utility_patterns.txt")),
    );
    let out = Path::new(&std::env::var("OUT_DIR").unwrap()).join("eibi_bundle.rs");
    std::fs::write(out, code).unwrap();
}

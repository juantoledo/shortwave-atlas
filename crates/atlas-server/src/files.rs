//! Writing the app's own files (scans, favorites) so a crash never leaves half a file.

use std::path::Path;

/// Write `contents` to `path` through a temporary file next to it, creating the folder.
pub fn write_atomic(path: &Path, contents: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, path)
}

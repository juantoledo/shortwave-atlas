//! The operating system, as data. Per-OS logic takes an `Os` argument instead of using
//! `cfg!`, so the Windows and macOS variants are unit-tested on any machine.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Os {
    Linux,
    Windows,
    Macos,
}

impl Os {
    /// The OS this binary was built for (anything else Unix-like counts as Linux).
    pub const CURRENT: Os = if cfg!(windows) {
        Os::Windows
    } else if cfg!(target_os = "macos") {
        Os::Macos
    } else {
        Os::Linux
    };

    /// `PATH` separator.
    pub fn path_list_sep(self) -> char {
        if self == Os::Windows {
            ';'
        } else {
            ':'
        }
    }

    /// Directory separator.
    pub fn dir_sep(self) -> char {
        if self == Os::Windows {
            '\\'
        } else {
            '/'
        }
    }

    /// `dir` + `name`, with this OS's separator.
    pub fn join(self, dir: &str, name: &str) -> String {
        let sep = self.dir_sep();
        format!("{}{sep}{name}", dir.trim_end_matches(['/', '\\']))
    }
}

//! Where to look for the helper program Shortwave Atlas runs (`rigctld`).
//!
//! Desktop installers ship it as a sidecar next to the app's executable. Otherwise it
//! comes from PATH, or from the usual install directories, which matters on macOS: an app
//! started from Finder does not get the shell's PATH (no Homebrew).

use crate::platform::Os;

/// Candidate paths for `name`, in lookup order. A configured path (anything with a
/// directory separator) is used as is. `env` reads environment variables and `list_dir`
/// lists a directory's entry names (both injected for tests).
pub fn tool_candidates(
    os: Os,
    name: &str,
    exe_dir: Option<&str>,
    env: impl Fn(&str) -> Option<String>,
    list_dir: impl Fn(&str) -> Vec<String>,
) -> Vec<String> {
    if name.contains('/') || name.contains('\\') {
        return vec![name.to_string()];
    }
    let file = if os == Os::Windows && !name.to_ascii_lowercase().ends_with(".exe") {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    let mut dirs: Vec<String> = vec![];
    dirs.extend(exe_dir.map(String::from));
    if let Some(path) = env("PATH") {
        dirs.extend(path.split(os.path_list_sep()).filter(|d| !d.is_empty()).map(String::from));
    }
    match os {
        Os::Linux => {}
        Os::Macos => dirs.extend(["/opt/homebrew/bin", "/usr/local/bin", "/opt/local/bin"].map(String::from)),
        Os::Windows => {
            // Hamlib's installer: C:\Program Files\hamlib-w64-4.6\bin (newest first)
            for (var, prefix) in [("ProgramFiles", "hamlib-w64-"), ("ProgramFiles(x86)", "hamlib-w32-")] {
                if let Some(pf) = env(var) {
                    let mut found: Vec<String> = list_dir(&pf).into_iter().filter(|n| n.starts_with(prefix)).collect();
                    found.sort_by(|a, b| b.cmp(a));
                    dirs.extend(found.iter().map(|n| os.join(&os.join(&pf, n), "bin")));
                }
            }
            if let Some(local) = env("LOCALAPPDATA") {
                dirs.push(os.join(&local, r"Microsoft\WinGet\Links"));
            }
        }
    }
    let mut out: Vec<String> = vec![];
    for d in dirs {
        let p = os.join(&d, &file);
        if !out.contains(&p) {
            out.push(p);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let m: HashMap<String, String> = pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        move |k| m.get(k).cloned()
    }

    #[test]
    fn configured_paths_are_used_as_is() {
        let none = |_: &str| vec![];
        assert_eq!(tool_candidates(Os::Linux, "/opt/hamlib/rigctld", None, env(&[]), none), ["/opt/hamlib/rigctld"]);
        assert_eq!(tool_candidates(Os::Windows, r"D:\ham\rigctld.exe", None, env(&[]), none), [r"D:\ham\rigctld.exe"]);
    }

    #[test]
    fn linux_sidecar_then_path() {
        let c =
            tool_candidates(Os::Linux, "rigctld", Some("/opt/swatlas"), env(&[("PATH", "/usr/bin:/bin:")]), |_| vec![]);
        assert_eq!(c, ["/opt/swatlas/rigctld", "/usr/bin/rigctld", "/bin/rigctld"]);
    }

    #[test]
    fn macos_finds_homebrew_without_a_shell_path() {
        let c = tool_candidates(
            Os::Macos,
            "rigctld",
            Some("/Applications/Shortwave Atlas.app/Contents/MacOS"),
            env(&[("PATH", "/usr/bin:/bin")]),
            |_| vec![],
        );
        assert_eq!(c[0], "/Applications/Shortwave Atlas.app/Contents/MacOS/rigctld");
        assert!(c.contains(&"/opt/homebrew/bin/rigctld".to_string()));
        assert!(c.contains(&"/usr/local/bin/rigctld".to_string()));
    }

    #[test]
    fn windows_adds_exe_and_finds_the_hamlib_installer() {
        let e = env(&[
            ("PATH", r"C:\Windows\system32;C:\Windows"),
            ("ProgramFiles", r"C:\Program Files"),
            ("LOCALAPPDATA", r"C:\Users\me\AppData\Local"),
        ]);
        let list = |d: &str| {
            assert_eq!(d, r"C:\Program Files");
            vec!["hamlib-w64-4.5.5".into(), "hamlib-w64-4.6".into(), "Git".into()]
        };
        let c = tool_candidates(Os::Windows, "rigctld", Some(r"C:\Program Files\Shortwave Atlas"), e, list);
        assert_eq!(
            c,
            [
                r"C:\Program Files\Shortwave Atlas\rigctld.exe",
                r"C:\Windows\system32\rigctld.exe",
                r"C:\Windows\rigctld.exe",
                r"C:\Program Files\hamlib-w64-4.6\bin\rigctld.exe",
                r"C:\Program Files\hamlib-w64-4.5.5\bin\rigctld.exe",
                r"C:\Users\me\AppData\Local\Microsoft\WinGet\Links\rigctld.exe",
            ]
        );
    }
}

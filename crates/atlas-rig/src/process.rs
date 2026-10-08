//! Starting the helper programs (`rigctld`, `ffmpeg`): found next to the app first (the
//! installers' sidecars), then on PATH and in the usual install directories, and never
//! with a console window popping up on Windows.

use std::path::Path;

use atlas_core::platform::Os;
use atlas_core::tools::tool_candidates;
use tokio::process::Command;

/// The program to run for `name` (a bare name like `rigctld`, or a configured path):
/// the first candidate that exists, else `name` itself, so the spawn error says what's missing.
pub fn locate(name: &str) -> String {
    let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_string_lossy().into_owned()));
    let list_dir = |d: &str| -> Vec<String> {
        std::fs::read_dir(d)
            .map(|rd| rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
            .unwrap_or_default()
    };
    tool_candidates(Os::CURRENT, name, exe_dir.as_deref(), |k| std::env::var(k).ok(), list_dir)
        .into_iter()
        .find(|c| Path::new(c).is_file())
        .unwrap_or_else(|| name.to_string())
}

/// `Command` for the helper `name` (see `locate`). On Windows it gets no console window:
/// the desktop app has none, and each spawn would flash one.
pub fn command(name: &str) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(locate(name));
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_tools_fall_back_to_the_name() {
        assert_eq!(locate("swatlas-no-such-tool"), "swatlas-no-such-tool");
        assert_eq!(locate("/nonexistent/rigctld"), "/nonexistent/rigctld");
    }

    #[cfg(unix)]
    #[test]
    fn finds_tools_on_path() {
        assert_eq!(
            locate("sh"),
            std::env::var("PATH")
                .unwrap()
                .split(':')
                .map(|d| format!("{d}/sh"))
                .find(|p| Path::new(p).is_file())
                .unwrap()
        );
    }
}

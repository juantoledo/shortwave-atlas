//! Starting the helper program (`rigctld`): found next to the app first (the
//! installers' sidecar), then on PATH and in the usual install directories, and never
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

/// Tie a started helper to this process, so it cannot outlive Shortwave Atlas. A clean exit stops
/// the helpers itself (`Atlas::shutdown`); this covers a crash or Task Manager on Windows,
/// where the helper joins a job that the OS kills when our last handle to it closes.
/// Elsewhere it does nothing.
pub fn tie_to_app(child: &tokio::process::Child) {
    #[cfg(windows)]
    if let Some(h) = child.raw_handle() {
        job::assign(h);
    }
    #[cfg(not(windows))]
    let _ = child;
}

#[cfg(windows)]
mod job {
    use std::os::windows::io::RawHandle;
    use std::sync::OnceLock;

    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    /// The job's handle (0 if it could not be made), open until the process ends.
    static JOB: OnceLock<usize> = OnceLock::new();

    fn job() -> Option<HANDLE> {
        let h = *JOB.get_or_init(|| unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() {
                tracing::warn!("cannot create a job for helper processes: {}", std::io::Error::last_os_error());
                return 0;
            }
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let ok = SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                (&raw const info).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            if ok == 0 {
                tracing::warn!("cannot set up the helper job: {}", std::io::Error::last_os_error());
                CloseHandle(job);
                return 0;
            }
            job as usize
        });
        (h != 0).then_some(h as HANDLE)
    }

    pub fn assign(process: RawHandle) {
        let Some(job) = job() else { return };
        // fails only if the helper already exited, or our own job forbids it
        if unsafe { AssignProcessToJobObject(job, process as HANDLE) } == 0 {
            tracing::debug!("helper not tied to the app: {}", std::io::Error::last_os_error());
        }
    }
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

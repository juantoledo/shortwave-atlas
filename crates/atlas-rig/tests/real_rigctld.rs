//! Against a real `rigctld -m 1` (Hamlib's dummy rig). Skipped when rigctld is not installed.

use std::process::Stdio;
use std::time::Duration;

use atlas_core::rig::{Link, Mode, Power, RigState};
use atlas_rig::hamlib::Hamlib;
use atlas_rig::RigBackend;
use tokio::process::Command;

const PORT: u16 = 45_321;

#[tokio::test]
async fn dummy_rig_round_trip() {
    let Ok(_child) = Command::new("rigctld")
        .args(["-m", "1", "-T", "127.0.0.1", "-t", &PORT.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
    else {
        eprintln!("rigctld not installed: skipping");
        return;
    };
    let h = Hamlib::new("127.0.0.1", PORT);
    let mut s = RigState::down();
    for _ in 0..50 {
        s = h.get_state(&s).await;
        if s.link == Link::Ok {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!((s.link, s.power), (Link::Ok, Power::On));

    h.set_freq(13_570_000).await.unwrap();
    h.set_mode(Mode::AM).await.unwrap();
    let s = h.get_state(&s).await;
    assert_eq!(s.freq_hz, Some(13_570_000));
    assert_eq!(s.mode.as_deref(), Some("AM"));
    assert!(s.strength_db.is_some());
}

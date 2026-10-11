//! A tiny in-process rigctld for tests: answers the commands in `hamlib::Cmd`.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

struct Rig {
    freq: u32,
    mode: String,
    power: bool,
    fail_next: Option<String>,
    /// S-meter per frequency (else -54, S0).
    strength: HashMap<u32, i32>,
    /// A band stack, like the FTDX-10's: tuning into another band (by whole MHz here) recalls
    /// the mode last used there. Off unless `band_mode` set one.
    bands: Option<HashMap<u32, String>>,
    /// How far entering CW (or CWR) moves the frequency; leaving it moves it back (a rig that
    /// shows CW with its pitch offset).
    cw_shift: i32,
    log: Vec<String>,
    /// Every command, reads included.
    all: Vec<String>,
}

pub struct FakeRigctld {
    pub port: u16,
    rig: Arc<Mutex<Rig>>,
}

impl FakeRigctld {
    pub async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let rig = Arc::new(Mutex::new(Rig {
            freq: 14_074_000,
            mode: "USB".into(),
            power: true,
            fail_next: None,
            strength: HashMap::new(),
            bands: None,
            cw_shift: 0,
            log: vec![],
            all: vec![],
        }));
        let shared = rig.clone();
        tokio::spawn(async move {
            while let Ok((sock, _)) = listener.accept().await {
                let rig = shared.clone();
                tokio::spawn(async move {
                    let (r, mut w) = sock.into_split();
                    let mut lines = BufReader::new(r).lines();
                    while let Ok(Some(line)) = lines.next_line().await {
                        let reply = answer(&mut *rig.lock().await, &line);
                        if w.write_all(reply.as_bytes()).await.is_err() {
                            break;
                        }
                    }
                });
            }
        });
        Self { port, rig }
    }

    /// Make the next command fail with this `RPRT` line.
    pub async fn fail_next(&self, rprt: &str) {
        self.rig.lock().await.fail_next = Some(rprt.into());
    }

    /// Write commands received so far.
    pub async fn log(&self) -> Vec<String> {
        self.rig.lock().await.log.clone()
    }

    /// Every command received so far.
    pub async fn all(&self) -> Vec<String> {
        self.rig.lock().await.all.clone()
    }

    /// Keep a band stack, with `mode` last used in the band from `mhz` MHz.
    pub async fn band_mode(&self, mhz: u32, mode: &str) {
        self.rig.lock().await.bands.get_or_insert_default().insert(mhz, mode.into());
    }

    /// Entering CW moves the frequency by `hz`; leaving it moves it back.
    pub async fn cw_shift(&self, hz: i32) {
        self.rig.lock().await.cw_shift = hz;
    }

    /// The S-meter reads `db` (relative to S9) while tuned to `hz`.
    pub async fn signal(&self, hz: u32, db: i32) {
        self.rig.lock().await.strength.insert(hz, db);
    }
}

fn answer(rig: &mut Rig, line: &str) -> String {
    rig.all.push(line.into());
    if let Some(e) = rig.fail_next.take() {
        return format!("{e}\n");
    }
    let mut parts = line.split_whitespace();
    let ok = "RPRT 0\n".to_string();
    match (parts.next(), parts.next()) {
        (Some("+\\dump_state"), _) => concat!(
            "dump_state:\n1\n3073\n2\n",
            "30000.000000 470000000.000000 0x1bf -1 -1 0x3 0x0\n0 0 0 0 0 0 0\n",
            "1800000.000000 54000000.000000 0x1bf 5000 100000 0x3 0x0\n0 0 0 0 0 0 0\n",
            "RPRT 0\n"
        )
        .into(),
        (Some("\\get_powerstat"), _) => format!("{}\n", u8::from(rig.power)),
        (Some("\\set_powerstat"), Some(v)) => {
            rig.log.push(line.into());
            rig.power = v == "1";
            ok
        }
        (_, _) if !rig.power => "RPRT -5\n".into(),
        (Some("f"), _) => format!("{}\n", rig.freq),
        (Some("F"), Some(v)) => {
            rig.log.push(line.into());
            let (from, to) = (rig.freq, v.parse().unwrap());
            if let Some(bands) = rig.bands.as_mut() {
                if from / 1_000_000 != to / 1_000_000 {
                    bands.insert(from / 1_000_000, rig.mode.clone());
                    if let Some(m) = bands.get(&(to / 1_000_000)) {
                        rig.mode = m.clone();
                    }
                }
            }
            rig.freq = to;
            ok
        }
        (Some("m"), _) => format!("{}\n3000\n", rig.mode),
        (Some("M"), Some(v)) => {
            rig.log.push(line.into());
            let cw = |m: &str| matches!(m, "CW" | "CWR");
            match (cw(&rig.mode), cw(v)) {
                (false, true) => rig.freq = rig.freq.saturating_add_signed(rig.cw_shift),
                (true, false) => rig.freq = rig.freq.saturating_add_signed(-rig.cw_shift),
                _ => {}
            }
            rig.mode = v.into();
            ok
        }
        (Some("l"), Some("STRENGTH")) => format!("{}\n", rig.strength.get(&rig.freq).copied().unwrap_or(-54)),
        (Some("l"), Some("CWPITCH")) => "600\n".into(),
        _ => "RPRT -1\n".into(),
    }
}

//! A tiny in-process rigctld for tests: answers the commands in `hamlib::Cmd`.

use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

struct Rig {
    freq: u32,
    mode: String,
    power: bool,
    fail_next: Option<String>,
    log: Vec<String>,
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
            log: vec![],
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
}

fn answer(rig: &mut Rig, line: &str) -> String {
    if let Some(e) = rig.fail_next.take() {
        return format!("{e}\n");
    }
    let mut parts = line.split_whitespace();
    let ok = "RPRT 0\n".to_string();
    match (parts.next(), parts.next()) {
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
            rig.freq = v.parse().unwrap();
            ok
        }
        (Some("m"), _) => format!("{}\n3000\n", rig.mode),
        (Some("M"), Some(v)) => {
            rig.log.push(line.into());
            rig.mode = v.into();
            ok
        }
        (Some("l"), Some("STRENGTH")) => "-54\n".into(),
        (Some("l"), Some("CWPITCH")) => "600\n".into(),
        _ => "RPRT -1\n".into(),
    }
}

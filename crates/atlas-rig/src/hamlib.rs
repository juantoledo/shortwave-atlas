//! `rigctld` client over TCP (port of `rig()` / `get_status()` from `reference/ftdx10_web.py`).
//!
//! One persistent connection, serialised by a mutex. Any IO error or timeout drops the
//! connection so a late answer can never be read as the reply to the next command.

use std::io;
use std::time::Duration;

use async_trait::async_trait;
use atlas_core::rig::{CommandError, Link, Mode, Power, RigState};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio::time::timeout;

use crate::RigBackend;

const TIMEOUT: Duration = Duration::from_secs(3);
/// Hamlib retries for ~6 s when the rig is off.
const POWERSTAT_TIMEOUT: Duration = Duration::from_secs(10);
/// Powering on makes Hamlib wake the rig first.
const SET_POWERSTAT_TIMEOUT: Duration = Duration::from_secs(20);

/// Every command SW Atlas may send. Being a closed enum, it is also the "no TX" whitelist.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cmd {
    GetFreq,
    SetFreq(u32),
    GetMode,
    SetMode(Mode),
    GetStrength,
    GetCwPitch,
    GetPowerstat,
    SetPowerstat(bool),
}

impl Cmd {
    pub fn line(self) -> String {
        match self {
            Self::GetFreq => "f".into(),
            Self::SetFreq(hz) => format!("F {hz}"),
            Self::GetMode => "m".into(),
            // passband 0 = the rig's default for that mode
            Self::SetMode(m) => format!("M {} 0", m.as_str()),
            Self::GetStrength => "l STRENGTH".into(),
            Self::GetCwPitch => "l CWPITCH".into(),
            Self::GetPowerstat => "\\get_powerstat".into(),
            Self::SetPowerstat(on) => format!("\\set_powerstat {}", u8::from(on)),
        }
    }

    /// Lines in a successful answer (an error is always a single `RPRT -n` line).
    fn reply_lines(self) -> usize {
        if self == Self::GetMode {
            2
        } else {
            1
        }
    }

    fn timeout(self) -> Duration {
        match self {
            Self::GetPowerstat => POWERSTAT_TIMEOUT,
            Self::SetPowerstat(_) => SET_POWERSTAT_TIMEOUT,
            _ => TIMEOUT,
        }
    }
}

#[derive(Debug)]
pub enum CmdError {
    /// Could not talk to rigctld at all.
    Io(io::Error),
    /// rigctld answered `RPRT -n` (or something unparsable).
    Rig(String),
}

impl std::fmt::Display for CmdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "rigctld: {e}"),
            Self::Rig(r) => write!(f, "rigctld: {r}"),
        }
    }
}

impl From<CmdError> for CommandError {
    fn from(e: CmdError) -> Self {
        CommandError::Rig(e.to_string())
    }
}

struct Conn {
    reader: BufReader<OwnedReadHalf>,
    writer: OwnedWriteHalf,
}

pub struct Hamlib {
    addr: String,
    conn: Mutex<Option<Conn>>,
    /// Last failure, for the settings page; cleared by the next good answer.
    last_error: std::sync::Mutex<Option<String>>,
}

impl Hamlib {
    pub fn new(host: &str, port: u16) -> Self {
        Self { addr: format!("{host}:{port}"), conn: Mutex::new(None), last_error: Default::default() }
    }

    fn note(&self, err: Option<&CmdError>) {
        *self.last_error.lock().expect("last_error lock") = err.map(|e| e.to_string());
    }

    /// Send one command and return its answer lines.
    pub async fn send(&self, cmd: Cmd) -> Result<Vec<String>, CmdError> {
        let mut guard = self.conn.lock().await;
        let res = timeout(cmd.timeout(), Self::exchange(&self.addr, &mut guard, cmd)).await;
        let res = res.unwrap_or_else(|_| Err(io::Error::new(io::ErrorKind::TimedOut, "timed out")));
        let res = match res {
            Ok(lines) => match lines.first() {
                Some(l) if l.starts_with("RPRT") && l != "RPRT 0" => Err(CmdError::Rig(l.clone())),
                _ => Ok(lines),
            },
            Err(e) => {
                *guard = None;
                Err(CmdError::Io(e))
            }
        };
        self.note(res.as_ref().err());
        res
    }

    async fn exchange(addr: &str, slot: &mut Option<Conn>, cmd: Cmd) -> io::Result<Vec<String>> {
        if slot.is_none() {
            let stream = TcpStream::connect(addr).await?;
            stream.set_nodelay(true)?;
            let (r, w) = stream.into_split();
            *slot = Some(Conn { reader: BufReader::new(r), writer: w });
        }
        let conn = slot.as_mut().expect("connected above");
        conn.writer.write_all(format!("{}\n", cmd.line()).as_bytes()).await?;
        let mut lines = Vec::with_capacity(2);
        while lines.len() < cmd.reply_lines() {
            let mut buf = String::new();
            if conn.reader.read_line(&mut buf).await? == 0 {
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "rigctld closed the connection"));
            }
            let line = buf.trim().to_string();
            let is_error = line.starts_with("RPRT");
            lines.push(line);
            if is_error {
                break;
            }
        }
        Ok(lines)
    }

    /// Send a write command and require `RPRT 0`.
    async fn write(&self, cmd: Cmd) -> Result<(), CmdError> {
        let lines = self.send(cmd).await?;
        match lines.first().map(String::as_str) {
            Some("RPRT 0") => Ok(()),
            other => Err(CmdError::Rig(other.unwrap_or("no answer").to_string())),
        }
    }

    async fn number(&self, cmd: Cmd) -> Result<f64, CmdError> {
        let lines = self.send(cmd).await?;
        let l = lines.first().map(String::as_str).unwrap_or("");
        l.parse::<f64>().map_err(|_| CmdError::Rig(format!("unexpected answer {l:?}")))
    }

    async fn power(&self) -> Result<Power, CmdError> {
        match self.number(Cmd::GetPowerstat).await {
            Ok(v) => Ok(parse_powerstat(v)),
            Err(CmdError::Rig(_)) => Ok(Power::Unknown),
            Err(e) => Err(e),
        }
    }

    /// Frequency, mode, S-meter and CW pitch, assuming the rig is on.
    async fn read_on(&self) -> Result<RigState, CmdError> {
        let freq = self.number(Cmd::GetFreq).await?;
        let mode_lines = self.send(Cmd::GetMode).await?;
        let mode = mode_lines.first().cloned();
        let passband = mode_lines.get(1).and_then(|p| p.parse::<f64>().ok());
        // a failed meter or pitch read is not worth failing the whole poll
        let strength = self.number(Cmd::GetStrength).await.ok();
        let cw_pitch =
            if matches!(mode.as_deref(), Some("CW" | "CWR")) { self.number(Cmd::GetCwPitch).await.ok() } else { None };
        Ok(RigState {
            link: Link::Ok,
            power: Power::On,
            freq_hz: Some(freq.round() as u32),
            mode,
            passband_hz: passband.map(|p| p.round() as u32),
            strength_db: strength.map(|s| s.round() as i32),
            cw_pitch_hz: cw_pitch.map(|p| p.round() as u32),
        })
    }
}

/// Hamlib powerstat: 1 = ON, 4 = OPERATE; anything else is off/standby.
fn parse_powerstat(v: f64) -> Power {
    if v == 1.0 || v == 4.0 {
        Power::On
    } else {
        Power::Off
    }
}

#[async_trait]
impl RigBackend for Hamlib {
    fn last_error(&self) -> Option<String> {
        self.last_error.lock().expect("last_error lock").clone()
    }

    async fn get_state(&self, last: &RigState) -> RigState {
        // While the rig is known to be on, skip the slow powerstat query.
        if last.power == Power::On {
            if let Ok(s) = self.read_on().await {
                return s;
            }
        }
        match self.power().await {
            Ok(Power::On) => self.read_on().await.unwrap_or_else(|_| RigState::without_rig(Link::Ok, Power::Unknown)),
            // don't ask for frequency/mode while off: each would time out
            Ok(p) => RigState::without_rig(Link::Ok, p),
            Err(_) => RigState::down(),
        }
    }

    async fn set_freq(&self, hz: u32) -> Result<(), CommandError> {
        Ok(self.write(Cmd::SetFreq(hz)).await?)
    }

    async fn set_mode(&self, mode: Mode) -> Result<(), CommandError> {
        Ok(self.write(Cmd::SetMode(mode)).await?)
    }

    async fn set_power(&self, on: bool) -> Result<(), CommandError> {
        Ok(self.write(Cmd::SetPowerstat(on)).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::FakeRigctld;

    #[test]
    fn command_lines() {
        assert_eq!(Cmd::SetFreq(13_570_000).line(), "F 13570000");
        assert_eq!(Cmd::SetMode(Mode::AM).line(), "M AM 0");
        assert_eq!(Cmd::SetPowerstat(true).line(), "\\set_powerstat 1");
        assert_eq!(Cmd::GetStrength.line(), "l STRENGTH");
    }

    #[test]
    fn powerstat_values() {
        assert_eq!(parse_powerstat(1.0), Power::On);
        assert_eq!(parse_powerstat(4.0), Power::On);
        assert_eq!(parse_powerstat(0.0), Power::Off);
        assert_eq!(parse_powerstat(2.0), Power::Off);
    }

    #[tokio::test]
    async fn reads_state_from_fake() {
        let fake = FakeRigctld::start().await;
        let h = Hamlib::new("127.0.0.1", fake.port);
        let s = h.get_state(&RigState::down()).await;
        assert_eq!(s.link, Link::Ok);
        assert_eq!(s.power, Power::On);
        assert_eq!(s.freq_hz, Some(14_074_000));
        assert_eq!(s.mode.as_deref(), Some("USB"));
        assert_eq!(s.passband_hz, Some(3000));
        assert_eq!(s.strength_db, Some(-54));
        assert_eq!(s.cw_pitch_hz, None);
    }

    #[tokio::test]
    async fn writes_and_reads_back() {
        let fake = FakeRigctld::start().await;
        let h = Hamlib::new("127.0.0.1", fake.port);
        h.set_freq(13_570_000).await.unwrap();
        h.set_mode(Mode::CW).await.unwrap();
        let s = h.get_state(&RigState::down()).await;
        assert_eq!(s.freq_hz, Some(13_570_000));
        assert_eq!(s.cw_pitch_hz, Some(600));
        assert_eq!(fake.log().await, ["F 13570000", "M CW 0"]);
    }

    #[tokio::test]
    async fn off_rig_skips_frequency() {
        let fake = FakeRigctld::start().await;
        let h = Hamlib::new("127.0.0.1", fake.port);
        h.set_power(false).await.unwrap();
        let s = h.get_state(&RigState::down()).await;
        assert_eq!(s, RigState::without_rig(Link::Ok, Power::Off));
    }

    #[tokio::test]
    async fn rig_error_is_reported() {
        let fake = FakeRigctld::start().await;
        let h = Hamlib::new("127.0.0.1", fake.port);
        fake.fail_next("RPRT -11").await;
        let e = h.set_freq(7_000_000).await.unwrap_err();
        assert_eq!(e, CommandError::Rig("rigctld: RPRT -11".into()));
        assert_eq!(h.last_error().as_deref(), Some("rigctld: RPRT -11"));
        // the connection is still in sync afterwards, and the error clears
        h.set_freq(7_100_000).await.unwrap();
        assert_eq!(h.last_error(), None);
    }

    #[tokio::test]
    async fn mode_error_is_one_line() {
        let fake = FakeRigctld::start().await;
        let h = Hamlib::new("127.0.0.1", fake.port);
        fake.fail_next("RPRT -9").await;
        assert!(matches!(h.send(Cmd::GetMode).await, Err(CmdError::Rig(_))));
        assert_eq!(h.send(Cmd::GetMode).await.unwrap(), ["USB", "3000"]);
    }

    #[tokio::test]
    async fn unreachable_rigctld_is_down() {
        let h = Hamlib::new("127.0.0.1", 1); // nothing listens on port 1
        assert_eq!(h.get_state(&RigState::down()).await, RigState::down());
    }
}

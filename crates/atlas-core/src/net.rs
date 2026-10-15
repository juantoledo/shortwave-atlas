//! Internet radio from the radio-browser.info directory: the NET band. To the listener it is a
//! band in the station list; inside, it is a source of its own (`NET_SOURCE`, keyed by the
//! directory's `stationuuid`) with its own rows and query. It never goes through the schedule
//! `Catalog`, which is a frequency index, and nothing here touches the rig or the listening
//! history.
//!
//! Pure: building the directory's queries and reading its answers, which streams can play and
//! from where, and the jitter buffer that paces a stream's PCM. `atlas-server` does the HTTP and
//! the decoding.

use std::collections::VecDeque;
use std::net::{IpAddr, Ipv4Addr};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::country::{flag, iso_of_itu, itu_of_iso, COUNTRIES};

/// `SourceDesc::id` of the internet stations.
pub const NET_SOURCE: &str = "radiobrowser";
/// The band id the station list uses for them.
pub const NET_BAND: &str = "net";
/// Answers the directory's server list from any of its servers.
pub const SERVERS_URL: &str = "https://all.api.radio-browser.info/json/servers";
/// Rows per page when the query does not say.
pub const DEFAULT_LIMIT: u32 = 50;
pub const MAX_LIMIT: u32 = 200;
/// Sample rate of the PCM a stream is decoded to (mono, 16-bit, like the rig's).
pub const NET_RATE: u32 = 32_000;
/// Stream audio held before it starts, and again after it ran dry: rides out network bursts.
pub const NET_PREBUFFER_MS: u32 = 1500;
/// Most stream audio held; past this the oldest goes (the source runs fast).
pub const NET_BUFFER_MAX_MS: u32 = 5000;

/// Argument of `Call::NetSearch`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct NetQuery {
    /// Part of the station's name.
    pub q: String,
    /// The station's country (EiBi code).
    pub country: Option<String>,
    pub offset: u32,
    /// 0 = `DEFAULT_LIMIT`; capped at `MAX_LIMIT`.
    pub limit: u32,
}

impl NetQuery {
    pub fn limit(&self) -> u32 {
        match self.limit {
            0 => DEFAULT_LIMIT,
            n => n.min(MAX_LIMIT),
        }
    }
}

/// One internet station.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct NetStation {
    /// The directory's `stationuuid`: the station's key in `NET_SOURCE`.
    pub uuid: String,
    pub name: String,
    /// The station's country (EiBi code), if the directory gives one we know.
    pub itu: Option<String>,
    /// flag-icons key.
    pub flag: Option<String>,
    /// As the directory words them (`english`, `romanian`).
    pub langs: Vec<String>,
    pub tags: Vec<String>,
    /// `MP3`, `AAC+`…; `None` if the directory could not tell.
    pub codec: Option<String>,
    /// kbit/s; `None` if unknown.
    pub bitrate: Option<u32>,
    /// HTTP live streaming rather than one endless stream.
    pub hls: bool,
    /// The station's website (http or https only).
    pub homepage: Option<String>,
    /// The directory's last check reached the stream.
    pub ok: bool,
    pub votes: u32,
    /// Its format can be decoded here (not HLS; MP3, AAC, Ogg Vorbis, FLAC, or not said).
    pub playable: bool,
}

/// A station being played: what the dock and the history show of it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct NetPlaying {
    pub uuid: String,
    pub name: String,
    pub itu: Option<String>,
    pub flag: Option<String>,
    pub codec: Option<String>,
    pub bitrate: Option<u32>,
}

/// A station to play: as above, and where its stream is.
#[derive(Clone, Debug, PartialEq)]
pub struct NetStream {
    pub station: NetPlaying,
    pub url: String,
}

/// How the stream being played is doing.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "state", rename_all = "snake_case")]
#[ts(export)]
pub enum NetState {
    Connecting,
    Playing,
    Failed { message: String },
}

/// Result of `Call::NetNow`: the internet station the receiver plays instead of the rig.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct NetNow {
    pub station: NetPlaying,
    pub state: NetState,
}

/// Whether a stream in this format can be decoded here. HLS is a playlist of segments, not one
/// stream; a codec the directory couldn't tell is mostly one of those, or a stream that fails.
pub fn playable(codec: Option<&str>, hls: bool) -> bool {
    !hls && codec
        .is_some_and(|c| ["MP3", "AAC", "AAC+", "OGG", "VORBIS", "FLAC"].contains(&c.to_ascii_uppercase().as_str()))
}

/// A stream URL the server may open: http or https, and not an address on the listener's own
/// machine or network (a directory entry must not make the server reach into the LAN).
pub fn stream_url_ok(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    let Some(rest) = lower.strip_prefix("http://").or_else(|| lower.strip_prefix("https://")) else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.is_empty() || authority.contains('@') {
        return false;
    }
    if authority.starts_with('[') {
        // an IPv6 literal: none of the directory's streams need one
        return false;
    }
    let host = authority.split(':').next().unwrap_or("");
    if host.is_empty() || host == "localhost" || host.ends_with(".localhost") || host.ends_with(".local") {
        return false;
    }
    match host.parse::<Ipv4Addr>() {
        Ok(ip) => public(IpAddr::V4(ip)),
        Err(_) => host.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-'),
    }
}

fn public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let [a, b, ..] = v4.octets();
            !(v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || a == 0
                || (a == 100 && (64..128).contains(&b)))
        }
        IpAddr::V6(_) => false,
    }
}

/// A station uuid as the directory writes them: hex digits and dashes.
fn uuid_ok(uuid: &str) -> bool {
    (8..=64).contains(&uuid.len()) && uuid.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-')
}

/// Path of a station's details on any directory server.
pub fn station_path(uuid: &str) -> Option<String> {
    uuid_ok(uuid).then(|| format!("/json/stations/byuuid/{uuid}"))
}

/// Path that counts a click on a station and answers its stream URL.
pub fn click_path(uuid: &str) -> Option<String> {
    uuid_ok(uuid).then(|| format!("/json/url/{uuid}"))
}

/// The station and its stream from a `station_path` answer.
pub fn parse_stream(json: &str) -> Result<NetStream, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| format!("station: {e}"))?;
    let r = v.as_array().and_then(|a| a.first()).ok_or("the directory does not know that station")?;
    let st = station(r).ok_or("the directory's answer has no station")?;
    if !st.playable {
        return Err("this stream's format can't be played yet".into());
    }
    let url = text(r, "url_resolved").or_else(|| text(r, "url")).ok_or("the station has no stream")?;
    let station =
        NetPlaying { uuid: st.uuid, name: st.name, itu: st.itu, flag: st.flag, codec: st.codec, bitrate: st.bitrate };
    Ok(NetStream { station, url })
}

/// The stream URL in a `click_path` answer, if it gives one.
pub fn parse_click(json: &str) -> Option<String> {
    let v: Value = serde_json::from_str(json).ok()?;
    (v.get("ok").and_then(Value::as_bool) == Some(true)).then(|| text(&v, "url")).flatten()
}

/// Paces a stream's PCM (16-bit mono) at real time: holds `NET_PREBUFFER_MS` before it starts and
/// again after running dry, and at most `NET_BUFFER_MAX_MS`, dropping the oldest past that.
pub struct JitterBuffer {
    buf: VecDeque<u8>,
    block: usize,
    prebuffer: usize,
    max: usize,
    playing: bool,
}

impl JitterBuffer {
    /// For PCM at `rate`, handed out `block_ms` at a time.
    pub fn new(rate: u32, block_ms: u32) -> Self {
        let bytes = |ms: u32| (rate as usize * 2 * ms as usize / 1000) & !1;
        let block = bytes(block_ms).max(2);
        Self {
            buf: VecDeque::new(),
            block,
            prebuffer: bytes(NET_PREBUFFER_MS).max(block),
            max: bytes(NET_BUFFER_MAX_MS).max(2 * block),
            playing: false,
        }
    }

    pub fn push(&mut self, pcm: &[u8]) {
        self.buf.extend(pcm);
        let over = self.buf.len().saturating_sub(self.max);
        // whole samples only, so the bytes stay paired
        let drop = (over + (over & 1)).min(self.buf.len());
        self.buf.drain(..drop);
    }

    /// The next block, when one is due; `None` while filling up.
    pub fn pop(&mut self) -> Option<Vec<u8>> {
        if !self.playing {
            if self.buf.len() < self.prebuffer {
                return None;
            }
            self.playing = true;
        }
        if self.buf.len() < self.block {
            self.playing = false;
            return None;
        }
        Some(self.buf.drain(..self.block).collect())
    }

    /// Audio held, in bytes.
    pub fn held(&self) -> usize {
        self.buf.len()
    }

    /// Handing blocks out (not filling up).
    pub fn playing(&self) -> bool {
        self.playing
    }
}

/// Result of `Call::NetSearch`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct NetPage {
    /// Only stations that can be listened to here (`playable`).
    pub items: Vec<NetStation>,
    /// A full page came back: there may be more (the directory gives no total).
    pub more: bool,
    /// Where the next page starts in the directory: the rows it sent, not only those kept.
    pub next_offset: u32,
}

/// A country the directory can search, for the NET band's country filter.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct NetCountry {
    /// EiBi code.
    pub itu: String,
    pub flag: Option<String>,
    pub en: String,
    pub es: String,
}

/// Result of `Call::NetCountries`: every country with an ISO code, by EiBi code.
pub fn countries() -> Vec<NetCountry> {
    COUNTRIES
        .iter()
        .filter(|c| iso_of_itu(c.itu).is_some())
        .map(|c| NetCountry { itu: c.itu.into(), flag: c.flag.map(Into::into), en: c.en.into(), es: c.es.into() })
        .collect()
}

/// Path and query of a station search on any directory server, or `None` when it can only find
/// nothing (a country the directory has no code for).
pub fn search_path(q: &NetQuery) -> Option<String> {
    let mut params: Vec<(&str, String)> = Vec::new();
    let name = q.q.trim();
    if !name.is_empty() {
        params.push(("name", name.to_string()));
    }
    if let Some(itu) = &q.country {
        params.push(("countrycode", iso_of_itu(itu)?));
    }
    params.extend([
        ("hidebroken", "true".to_string()),
        ("order", "clickcount".to_string()),
        ("reverse", "true".to_string()),
        ("offset", q.offset.to_string()),
        ("limit", q.limit().to_string()),
    ]);
    let query: Vec<String> = params.iter().map(|(k, v)| format!("{k}={}", encode(v))).collect();
    Some(format!("/json/stations/search?{}", query.join("&")))
}

/// Percent-encode everything but RFC 3986's unreserved characters.
fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// The directory's server names from its server list (`SERVERS_URL`), each once, in order.
/// Anything that is not a plain radio-browser.info host name is dropped.
pub fn parse_servers(json: &str) -> Result<Vec<String>, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| format!("server list: {e}"))?;
    let list = v.as_array().ok_or("server list: not a list")?;
    let mut names: Vec<String> = Vec::new();
    for s in list {
        let Some(name) = s.get("name").and_then(Value::as_str).map(str::to_ascii_lowercase) else { continue };
        let plain = name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-');
        if plain && name.ends_with(".radio-browser.info") && !names.contains(&name) {
            names.push(name);
        }
    }
    Ok(names)
}

/// The stations in a search answer, and how many rows were unusable (no uuid or name).
pub fn parse_stations(json: &str) -> Result<(Vec<NetStation>, usize), String> {
    let v: Value = serde_json::from_str(json).map_err(|e| format!("station list: {e}"))?;
    let rows = v.as_array().ok_or("station list: not a list")?;
    let mut out = Vec::with_capacity(rows.len());
    let mut bad = 0;
    for r in rows {
        match station(r) {
            Some(s) => out.push(s),
            None => bad += 1,
        }
    }
    Ok((out, bad))
}

/// A page from one search answer, keeping only the stations that can be listened to here: `more`
/// while the directory filled the page, whatever was left out of it.
pub fn page(json: &str, q: &NetQuery) -> Result<NetPage, String> {
    let (all, bad) = parse_stations(json)?;
    let sent = all.len() + bad;
    let items: Vec<NetStation> = all.into_iter().filter(|s| s.playable).collect();
    Ok(NetPage { more: sent >= q.limit() as usize, next_offset: q.offset + sent as u32, items })
}

fn station(r: &Value) -> Option<NetStation> {
    let uuid = text(r, "stationuuid")?;
    let name = text(r, "name")?;
    let itu = text(r, "countrycode").and_then(|c| itu_of_iso(&c)).map(str::to_string);
    let codec = text(r, "codec").filter(|c| !c.eq_ignore_ascii_case("unknown"));
    let homepage = text(r, "homepage").filter(|h| {
        let h = h.to_ascii_lowercase();
        h.starts_with("http://") || h.starts_with("https://")
    });
    Some(NetStation {
        uuid,
        name,
        flag: itu.as_deref().and_then(flag).map(str::to_string),
        itu,
        langs: list(r, "language"),
        tags: list(r, "tags"),
        codec,
        bitrate: number(r, "bitrate").filter(|&b| b > 0),
        hls: number(r, "hls").is_some_and(|n| n > 0) || r.get("hls").and_then(Value::as_bool) == Some(true),
        homepage,
        ok: number(r, "lastcheckok").is_some_and(|n| n > 0)
            || r.get("lastcheckok").and_then(Value::as_bool) == Some(true),
        votes: number(r, "votes").unwrap_or(0),
        playable: false,
    })
    .map(|mut s| {
        s.playable = playable(s.codec.as_deref(), s.hls);
        s
    })
}

/// A string field, trimmed and with its runs of whitespace made one space; `None` if empty.
fn text(r: &Value, key: &str) -> Option<String> {
    let s = r.get(key)?.as_str()?.split_whitespace().collect::<Vec<_>>().join(" ");
    (!s.is_empty()).then_some(s)
}

/// A number field, also when the directory sends it as a string.
fn number(r: &Value, key: &str) -> Option<u32> {
    match r.get(key)? {
        Value::Number(n) => n.as_u64().map(|n| n.min(u64::from(u32::MAX)) as u32),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

/// A comma-separated field as its non-empty parts.
fn list(r: &Value, key: &str) -> Vec<String> {
    text(r, key)
        .map(|s| s.split(',').map(str::trim).filter(|p| !p.is_empty()).map(str::to_string).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two rows as the directory sent them (trimmed of fields we don't read), plus the cases it
    /// can send: no codec, a string number, a bad homepage, no uuid.
    const ANSWER: &str = r#"[
      {"stationuuid":"9644d860-0601-11e8-ae97-52543be04c81","name":"Radio Romania International",
       "url":"http://stream2.srr.ro:8052/;","homepage":"http://www.rri.ro/","tags":"folk,pop",
       "country":"Romania","countrycode":"RO","language":"romanian","languagecodes":"ro","votes":1067,
       "codec":"MP3","bitrate":128,"hls":0,"lastcheckok":1,"geo_lat":null,"geo_long":null},
      {"stationuuid":"ca4b428b-7652-44f6-ae27-dccf8213d76a","name":"Radio Romania International 3",
       "homepage":"https://www.rri.ro/","tags":"international","countrycode":"RO",
       "language":"english/ españa/ russian /francia /italia,romanian","votes":4,
       "codec":"AAC+","bitrate":96,"hls":0,"lastcheckok":1},
      {"stationuuid":"c1","name":"  CRI   News Plus AM846 ","homepage":"javascript:alert(1)","tags":"",
       "countrycode":"CN","language":"english","votes":"12","codec":"UNKNOWN","bitrate":0,"hls":1,
       "lastcheckok":0},
      {"stationuuid":"u2","name":"Radio Nowhere","countrycode":"ZZ","codec":"","lastcheckok":true},
      {"stationuuid":"","name":"No uuid"},
      {"stationuuid":"u3","name":"   "}
    ]"#;

    #[test]
    fn reads_the_directory_answer() {
        let (st, bad) = parse_stations(ANSWER).unwrap();
        assert_eq!((st.len(), bad), (4, 2));
        let rri = &st[0];
        assert_eq!(rri.uuid, "9644d860-0601-11e8-ae97-52543be04c81");
        assert_eq!((rri.itu.as_deref(), rri.flag.as_deref()), (Some("ROU"), Some("ro")));
        assert_eq!((rri.codec.as_deref(), rri.bitrate, rri.hls, rri.ok), (Some("MP3"), Some(128), false, true));
        assert_eq!(rri.tags, ["folk", "pop"]);
        assert_eq!(rri.homepage.as_deref(), Some("http://www.rri.ro/"));
        assert_eq!(st[1].langs, ["english/ españa/ russian /francia /italia", "romanian"]);

        let cri = &st[2];
        assert_eq!(cri.name, "CRI News Plus AM846");
        assert_eq!((cri.itu.as_deref(), cri.codec.as_deref(), cri.bitrate), (Some("CHN"), None, None));
        assert_eq!((cri.hls, cri.ok, cri.votes), (true, false, 12));
        assert_eq!(cri.homepage, None, "only http(s) links are kept");
        assert!(cri.tags.is_empty());

        let nowhere = &st[3];
        assert_eq!((nowhere.itu.as_deref(), nowhere.flag.as_deref(), nowhere.codec.as_deref()), (None, None, None));
        assert!(nowhere.ok);
    }

    #[test]
    fn a_page_is_full_or_the_last() {
        let q = NetQuery { limit: 6, offset: 12, ..Default::default() };
        let p = page(ANSWER, &q).unwrap();
        assert!(p.more, "6 rows came back (2 unusable, 2 that can't play) for 6 asked");
        assert_eq!(
            p.items.iter().map(|s| s.uuid.as_str()).collect::<Vec<_>>(),
            ["9644d860-0601-11e8-ae97-52543be04c81", "ca4b428b-7652-44f6-ae27-dccf8213d76a"]
        );
        assert_eq!(p.next_offset, 18, "the next page starts after every row sent");
        let q = NetQuery { limit: 10, ..Default::default() };
        let p = page(ANSWER, &q).unwrap();
        assert_eq!((p.items.len(), p.more, p.next_offset), (2, false, 6));
        assert!(parse_stations("{}").is_err());
        assert!(parse_stations("not json").is_err());
    }

    #[test]
    fn search_paths() {
        let q = NetQuery { q: " Radio Nacional ".into(), country: Some("ARG".into()), offset: 50, limit: 0 };
        assert_eq!(
            search_path(&q).unwrap(),
            "/json/stations/search?name=Radio%20Nacional&countrycode=AR&hidebroken=true&order=clickcount\
             &reverse=true&offset=50&limit=50"
        );
        let q = NetQuery { limit: 1000, ..Default::default() };
        assert_eq!(
            search_path(&q).unwrap(),
            "/json/stations/search?hidebroken=true&order=clickcount&reverse=true&offset=0&limit=200"
        );
        let q = NetQuery { q: "Música & más/1".into(), ..Default::default() };
        assert!(search_path(&q).unwrap().starts_with("/json/stations/search?name=M%C3%BAsica%20%26%20m%C3%A1s%2F1&"));
        // no ISO code: nothing to find
        assert_eq!(search_path(&NetQuery { country: Some("ALS".into()), ..Default::default() }), None);
        assert_eq!(search_path(&NetQuery { country: Some("NOPE".into()), ..Default::default() }), None);
    }

    #[test]
    fn server_names() {
        let json = r#"[{"ip":"91.98.4.78","name":"de1.api.radio-browser.info"},
            {"ip":"2a01:4f8:1c1d:699::1","name":"de1.api.radio-browser.info"},
            {"ip":"1.2.3.4","name":"NL1.api.radio-browser.info"},
            {"ip":"6.6.6.6","name":"evil.example.com"},
            {"ip":"6.6.6.6","name":"x.radio-browser.info/../"},
            {"ip":"7.7.7.7"}]"#;
        assert_eq!(parse_servers(json).unwrap(), ["de1.api.radio-browser.info", "nl1.api.radio-browser.info"]);
        assert!(parse_servers("{}").is_err());
    }

    #[test]
    fn countries_the_directory_knows() {
        let c = countries();
        assert!(c.len() > 200, "{}", c.len());
        assert!(c.iter().any(|c| c.itu == "ARG" && c.es == "Argentina"));
        assert!(c.iter().all(|c| c.itu != "ALS" && c.itu != "CLA"));
        assert!(c
            .iter()
            .all(|c| search_path(&NetQuery { country: Some(c.itu.clone()), ..Default::default() }).is_some()));
    }

    #[test]
    fn what_can_play() {
        let (st, _) = parse_stations(ANSWER).unwrap();
        assert_eq!(st.iter().map(|s| s.playable).collect::<Vec<_>>(), [true, true, false, false], "HLS; no codec");
        assert!(playable(Some("mp3"), false));
        assert!(playable(Some("AAC+"), false));
        assert!(!playable(None, false));
        assert!(!playable(Some("MP3"), true));
        assert!(!playable(Some("OPUS"), false));
    }

    #[test]
    fn stream_urls_the_server_may_open() {
        for ok in [
            "http://stream2.srr.ro:8052/;",
            "https://stream4.srr.ro:8443/romania-rri3",
            "http://8.8.8.8/x",
            "HTTP://Example.com",
            "http://100.128.0.1/",
        ] {
            assert!(stream_url_ok(ok), "{ok}");
        }
        for bad in [
            "file:///etc/passwd",
            "ftp://example.com/a",
            "http://127.0.0.1:8080/api/audio",
            "http://localhost:8080/",
            "http://radio.localhost/",
            "http://printer.local/",
            "http://192.168.1.1/",
            "http://10.0.0.5/",
            "http://172.16.3.4/",
            "http://169.254.1.1/",
            "http://0.0.0.0/",
            "http://100.64.0.1/",
            "http://[::1]/",
            "http://user:pw@example.com/",
            "http:///nohost",
            "http://exa mple.com/",
        ] {
            assert!(!stream_url_ok(bad), "{bad}");
        }
    }

    #[test]
    fn station_and_click_answers() {
        let rri = "9644d860-0601-11e8-ae97-52543be04c81";
        assert_eq!(station_path(rri), Some(format!("/json/stations/byuuid/{rri}")));
        assert_eq!(click_path(rri), Some(format!("/json/url/{rri}")));
        assert_eq!(station_path("../servers"), None);
        assert_eq!(click_path("x?y=1"), None);

        let json = r#"[{"stationuuid":"9644d860-0601-11e8-ae97-52543be04c81","name":"Radio Romania International",
            "url":"http://stream2.srr.ro:8052/;","url_resolved":"http://stream2.srr.ro:8052/;","countrycode":"RO",
            "codec":"MP3","bitrate":128,"hls":0,"lastcheckok":1}]"#;
        let s = parse_stream(json).unwrap();
        assert_eq!(s.url, "http://stream2.srr.ro:8052/;");
        assert_eq!(
            s.station,
            NetPlaying {
                uuid: rri.into(),
                name: "Radio Romania International".into(),
                itu: Some("ROU".into()),
                flag: Some("ro".into()),
                codec: Some("MP3".into()),
                bitrate: Some(128),
            }
        );
        assert!(parse_stream("[]").is_err());
        assert!(parse_stream(r#"[{"stationuuid":"u","name":"n","codec":"MP3","hls":1,"url":"http://a.b/"}]"#).is_err());
        assert!(parse_stream(r#"[{"stationuuid":"u","name":"n","codec":"MP3"}]"#).is_err(), "no stream");

        let click = r#"{"ok":true,"message":"retrieved station url","stationuuid":"9644d860-0601-11e8-ae97-52543be04c81",
            "name":"Radio Romania International","url":"http://stream2.srr.ro:8052/;"}"#;
        assert_eq!(parse_click(click).as_deref(), Some("http://stream2.srr.ro:8052/;"));
        assert_eq!(parse_click(r#"{"ok":false,"url":"http://a.b/"}"#), None);
        assert_eq!(parse_click(""), None);
    }

    #[test]
    fn jitter_buffer_paces_the_stream() {
        // 1000 Hz, 100 ms blocks: 200 bytes a block, 3000 to start, 10000 at most
        let mut jb = JitterBuffer::new(1000, 100);
        jb.push(&[1; 2800]);
        assert_eq!(jb.pop(), None, "still filling up");
        jb.push(&[2; 200]);
        assert_eq!(jb.pop().map(|b| b.len()), Some(200));
        assert!(jb.playing());
        for _ in 0..14 {
            assert!(jb.pop().is_some());
        }
        assert_eq!(jb.held(), 0);
        assert_eq!(jb.pop(), None, "ran dry: fills up again");
        assert!(!jb.playing());
        jb.push(&[3; 1000]);
        assert_eq!(jb.pop(), None);
        // a fast source: the oldest goes, in whole samples
        jb.push(&[4; 12_000]);
        assert_eq!(jb.held(), 10_000);
        assert_eq!(jb.pop().unwrap()[0], 4);
    }

    #[test]
    fn query_json_shape() {
        let q: NetQuery = serde_json::from_str(r#"{"q":"bbc","country":"G"}"#).unwrap();
        assert_eq!(q, NetQuery { q: "bbc".into(), country: Some("G".into()), offset: 0, limit: 0 });
    }
}

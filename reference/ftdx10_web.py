#!/usr/bin/env python3
"""
Receive-only web control for the Yaesu FTDX10 (via Hamlib rigctld).

- Frequency, mode and S-meter; ham and SW broadcast bands.
- Scan: one pass over a band at the selected step (S-meter per point), listing the
  active frequencies as they are found; the radio then returns to your frequency.
- Low-latency audio: ffmpeg captures the radio's USB codec and the page plays
  raw PCM through the Web Audio API (no Icecast, no <audio> buffering).
- Audio waterfall: the page runs an FFT on the received audio, so it shows the
  receiver passband (what you hear), not the whole band.
- Does NOT expose PTT or transmit in any way.
- Python 3 standard library only; audio needs the ffmpeg binary.

Environment variables (all optional):
  RIGCTLD_HOST  (127.0.0.1)   RIGCTLD_PORT (4532)
  WEB_PORT      (8080)        WEB_BIND     (0.0.0.0)
  AUDIO_DEVICE  (plughw:CARD=CODEC,DEV=0)   AUDIO_RATE (16000)
  WEB_AUTH      ("user:password") -> enables basic authentication
"""
import base64
import hmac
import json
import os
import queue
import signal
import socket
import subprocess
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlparse

RIG_HOST = os.environ.get("RIGCTLD_HOST", "127.0.0.1")
RIG_PORT = int(os.environ.get("RIGCTLD_PORT", "4532"))
WEB_PORT = int(os.environ.get("WEB_PORT", "8080"))
WEB_BIND = os.environ.get("WEB_BIND", "0.0.0.0")
AUDIO_DEVICE = os.environ.get("AUDIO_DEVICE", "plughw:CARD=CODEC,DEV=0")
AUDIO_RATE = int(os.environ.get("AUDIO_RATE", "16000"))
WEB_AUTH = os.environ.get("WEB_AUTH", "")

ALLOWED_MODES = {"USB", "LSB", "CW", "CWR", "AM", "FM"}
FREQ_MIN, FREQ_MAX = 30_000, 56_000_000
AUDIO_CHUNK = AUDIO_RATE * 2 // 50   # bytes per block sent to listeners (20 ms)
AUDIO_QUEUE = 50                     # blocks buffered per listener (1 s) before dropping
SCAN_MAX_POINTS = 1000  # points per scan
SCAN_SETTLE_MS = 100    # wait after each frequency change before reading the S-meter
SCAN_FLOOR_MIN = 10     # readings needed before the noise floor is trusted
SCAN_SPLIT_DB = 6       # dip between two peaks that makes them separate signals

_lock = threading.Lock()


def rig(cmd, nlines=1, timeout=3):
    """Send a command to rigctld and return the response lines."""
    with _lock:
        with socket.create_connection((RIG_HOST, RIG_PORT), timeout=timeout) as s:
            s.settimeout(timeout)
            f = s.makefile("rw", newline="\n")
            f.write(cmd + "\n")
            f.flush()
            return [f.readline().strip() for _ in range(nlines)]


# --------------------------------------------------------------------------
# Audio: one ffmpeg reads the sound card (raw 16-bit mono PCM) and the blocks
# are fanned out to every listener. ffmpeg runs only while someone listens.
# --------------------------------------------------------------------------
class AudioHub:
    def __init__(self):
        self.lock = threading.Lock()
        self.proc = None
        self.subs = set()

    @staticmethod
    def _cmd():
        return ["ffmpeg", "-hide_banner", "-loglevel", "error", "-nostdin",
                "-fflags", "nobuffer", "-f", "alsa", "-ac", "2", "-ar", "48000",
                "-i", AUDIO_DEVICE, "-ac", "1", "-ar", str(AUDIO_RATE),
                "-f", "s16le", "-flush_packets", "1", "pipe:1"]

    def subscribe(self):
        q = queue.Queue(maxsize=AUDIO_QUEUE)
        with self.lock:
            if self.proc is None:
                self.proc = subprocess.Popen(self._cmd(), stdin=subprocess.DEVNULL,
                                             stdout=subprocess.PIPE, bufsize=0)
                threading.Thread(target=self._pump, args=(self.proc,), daemon=True).start()
            self.subs.add(q)
        return q

    def unsubscribe(self, q):
        with self.lock:
            self.subs.discard(q)
            if not self.subs:
                self._kill()

    def close(self):
        with self.lock:
            self._kill()

    def _kill(self):
        if self.proc is not None:
            self.proc.terminate()
            self.proc = None

    def _pump(self, proc):
        buf = bytearray()
        while True:
            data = proc.stdout.read(4096)
            if not data:
                break
            buf += data
            if len(buf) < AUDIO_CHUNK:
                continue
            block, buf = bytes(buf), bytearray()
            with self.lock:
                subs = list(self.subs) if self.proc is proc else []
            for q in subs:
                try:
                    q.put_nowait(block)
                except queue.Full:
                    pass  # slow listener: drop the block, the page resyncs
        proc.wait()
        with self.lock:
            if self.proc is proc:  # ffmpeg exited on its own (device busy/unplugged)
                self.proc = None
                for q in self.subs:
                    while True:
                        try:
                            q.put_nowait(None)  # tell the listener the stream ended
                            break
                        except queue.Full:
                            q.get_nowait()


AUDIO = AudioHub()


class Conflict(Exception):
    """Operation not allowed in the current state (HTTP 409)."""


# --------------------------------------------------------------------------
# Scan: one pass over a band (set frequency, wait, read the S-meter), then
# back to the original frequency. Active frequencies are found as it goes.
# --------------------------------------------------------------------------
def split_run(run):
    """Peaks of one run of active points [(freq, level)]. Neighbouring peaks are one
    signal unless the level dips SCAN_SPLIT_DB below the weaker of the two."""
    peaks = []
    for i, (f, v) in enumerate(run):
        left = run[i - 1][1] if i else None
        right = run[i + 1][1] if i + 1 < len(run) else None
        if (left is None or v >= left) and (right is None or v > right):
            peaks.append((i, f, v))
    out = [peaks[0]]
    for p in peaks[1:]:
        q = out[-1]
        valley = min(v for _, v in run[q[0]:p[0] + 1])
        if min(q[2], p[2]) - valley < SCAN_SPLIT_DB:
            if p[2] > q[2]:
                out[-1] = p  # same signal: keep the stronger point
        else:
            out.append(p)
    return [{"freq": f, "level": v} for _, f, v in out]


def find_hits(freqs, levels, margin):
    """Noise floor (25th percentile, low enough for busy bands) and the active signals.

    Points at least `margin` dB over the floor are active; each signal is reported at
    its strongest point. Returns (floor, hits); floor is None until enough points.
    """
    vals = sorted(v for v in levels if v is not None)
    if len(vals) < SCAN_FLOOR_MIN:
        return None, []
    floor = vals[len(vals) // 4]
    hits, run = [], []
    for f, v in zip(freqs, levels):
        if v is not None and v >= floor + margin:
            run.append((f, v))
        elif run:
            hits += split_run(run)
            run = []
    if run:
        hits += split_run(run)
    return floor, hits


class Scanner:
    def __init__(self):
        self.lock = threading.Lock()        # guards the state below
        self.start_lock = threading.Lock()  # serializes starts
        self.stop_ev = threading.Event()
        self.thread = None
        self.running = False
        self.lo = self.hi = self.step = self.n = self.pos = 0
        self.cur_freq = None
        self.levels, self.floor, self.hits, self.error = [], None, [], None
        self.orig = {}

    def start(self, lo, hi, step, margin):
        lo, hi, step, margin = int(lo), int(hi), int(step), int(margin)
        if not FREQ_MIN <= lo < hi <= FREQ_MAX:
            raise ValueError("Invalid frequency range")
        if step < 100:
            raise ValueError("The step must be at least 100 Hz")
        if not 3 <= margin <= 40:
            raise ValueError("Sensitivity must be between 3 and 40 dB")
        n = (hi - lo) // step + 1
        if n < 2:
            raise ValueError("The step is larger than the band")
        if n > SCAN_MAX_POINTS:
            raise ValueError(f"Too many points ({n}, max {SCAN_MAX_POINTS}): choose a larger step")

        with self.start_lock:
            if self.running:
                raise Conflict("A scan is already running")
            if get_power() != 1:
                raise Conflict("The radio is off")
            orig = int(float(rig("f")[0]))
            mode, passband = rig("m", 2)
            freqs = [lo + i * step for i in range(n)]
            with self.lock:
                self.lo, self.hi, self.step, self.n, self.pos = lo, hi, step, n, 0
                self.cur_freq = None
                self.levels, self.floor, self.hits, self.error = [], None, [], None
                self.orig = {"freq": orig, "mode": mode, "passband": passband}
                self.stop_ev.clear()
                self.running = True
            self.thread = threading.Thread(target=self._run, args=(freqs, margin, orig), daemon=True)
            self.thread.start()
        return n

    def _run(self, freqs, margin, orig):
        err = None
        try:
            for f in freqs:
                if self.stop_ev.is_set():
                    break
                with self.lock:
                    self.cur_freq = f
                r = rig(f"F {f}")[0]
                if not r.startswith("RPRT 0"):
                    raise RuntimeError(f"rigctld: {r}")
                time.sleep(SCAN_SETTLE_MS / 1000)
                v = None
                for _ in range(2):  # one retry if the read fails
                    try:
                        v = int(round(float(rig("l STRENGTH")[0])))
                        break
                    except ValueError:
                        continue
                with self.lock:
                    self.levels.append(v)
                    self.pos = len(self.levels)
                    self.floor, self.hits = find_hits(freqs, self.levels, margin)
        except Exception as e:  # noqa: BLE001
            err = str(e) or e.__class__.__name__
        finally:
            try:
                rig(f"F {orig}")  # back to the frequency you were listening on
            except Exception as e:  # noqa: BLE001
                err = err or f"Could not restore the frequency: {e}"
            with self.lock:
                self.running = False
                self.error = err
                self.cur_freq = None

    def stop(self):
        """Request a stop and wait until the original frequency is restored."""
        self.stop_ev.set()
        t = self.thread
        if t is not None and t.is_alive() and t is not threading.current_thread():
            t.join(timeout=20)

    def status_while_running(self):
        with self.lock:
            return {"power": 1, **self.orig, "strength": None, "cwpitch": None, "scanning": True}

    def snapshot(self):
        with self.lock:
            return {"running": self.running, "lo": self.lo, "hi": self.hi, "step": self.step,
                    "n": self.n, "pos": self.pos, "cur_freq": self.cur_freq, "floor": self.floor,
                    "hits": list(self.hits), "error": self.error}


SCANNER = Scanner()


def ensure_idle():
    if SCANNER.running:
        raise Conflict("A scan is running: stop it first")


def get_power():
    """1 = on, 0 = off, None = the radio doesn't answer. Hamlib retries for ~6 s when it's off."""
    r = rig("\\get_powerstat", timeout=10)[0]
    try:
        return 1 if int(r) in (1, 4) else 0  # 1 = ON, 4 = OPERATE
    except ValueError:
        return None  # "RPRT -n"


def set_power(on):
    ensure_idle()
    if not isinstance(on, bool):
        raise ValueError("'on' must be true or false")
    # Powering on makes Hamlib wake the radio first, so allow plenty of time
    r = rig(f"\\set_powerstat {int(on)}", timeout=20)[0]
    if not r.startswith("RPRT 0"):
        raise RuntimeError(f"rigctld: {r}")


def get_status():
    if SCANNER.running:  # the rig is hopping: report where it will return to
        return SCANNER.status_while_running()
    power = get_power()
    if power != 1:  # don't ask for frequency/mode: each would time out
        return {"power": power}
    freq = int(float(rig("f")[0]))
    mode, passband = rig("m", 2)
    try:
        strength = int(float(rig("l STRENGTH")[0]))
    except (ValueError, OSError):
        strength = None
    cwpitch = None
    if mode in ("CW", "CWR"):  # the waterfall needs it for click-to-tune
        try:
            cwpitch = int(float(rig("l CWPITCH")[0]))
        except (ValueError, OSError):
            pass
    return {"power": 1, "freq": freq, "mode": mode, "passband": passband,
            "strength": strength, "cwpitch": cwpitch, "scanning": False}


def set_freq(hz):
    ensure_idle()
    hz = int(hz)
    if not FREQ_MIN <= hz <= FREQ_MAX:
        raise ValueError("Frequency out of range")
    r = rig(f"F {hz}")[0]
    if not r.startswith("RPRT 0"):
        raise RuntimeError(f"rigctld: {r}")


def set_mode(mode):
    ensure_idle()
    mode = str(mode).upper()
    if mode not in ALLOWED_MODES:
        raise ValueError("Mode not allowed")
    r = rig(f"M {mode} 0")[0]
    if not r.startswith("RPRT 0"):
        raise RuntimeError(f"rigctld: {r}")


PAGE = r"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>FTDX10 Remote</title>
<style>
  :root { --bg:#10141a; --card:#1a212b; --fg:#e6edf3; --muted:#8b98a8; --acc:#3fb950; --line:#2b3544; }
  * { box-sizing: border-box; }
  body { margin:0; font-family: system-ui, sans-serif; background:var(--bg); color:var(--fg); }
  main { max-width: 760px; margin: 0 auto; padding: 16px; display:grid; gap:14px; }
  .card { background:var(--card); border:1px solid var(--line); border-radius:12px; padding:14px; }
  h1 { font-size:16px; margin:0 0 4px; color:var(--muted); font-weight:600; }
  #freq { font: 600 44px/1.1 ui-monospace, Menlo, Consolas, monospace; text-align:center; letter-spacing:1px; }
  #freq small { font-size:18px; color:var(--muted); }
  .row { display:flex; flex-wrap:wrap; gap:6px; justify-content:center; margin-top:10px; }
  button { background:#242d3a; color:var(--fg); border:1px solid var(--line); border-radius:8px;
           padding:9px 12px; font-size:14px; cursor:pointer; }
  button:hover { background:#2d3848; }
  button:disabled { opacity:.45; cursor:default; }
  button.on { background:var(--acc); color:#04120a; border-color:var(--acc); font-weight:700; }
  input[type=text], select { background:#0d1117; color:var(--fg); border:1px solid var(--line); border-radius:8px;
           padding:9px 12px; font-size:16px; }
  input[type=text] { width:140px; text-align:center; }
  #meter { height:14px; background:#0d1117; border:1px solid var(--line); border-radius:7px; overflow:hidden; }
  #bar { height:100%; width:0%; background:linear-gradient(90deg,#3fb950,#d29922 70%,#f85149); transition:width .3s; }
  #s { text-align:center; margin-top:6px; color:var(--muted); font-size:14px; }
  .vol { display:flex; align-items:center; gap:8px; color:var(--muted); font-size:14px; }
  .vol input { width:160px; accent-color:var(--acc); }
  #msg { text-align:center; color:var(--muted); font-size:13px; min-height:18px; }
  .note { text-align:center; color:var(--muted); font-size:12px; min-height:16px; margin-top:6px; }
  #wfwrap { position:relative; margin-top:10px; border:1px solid var(--line); border-radius:8px;
            overflow:hidden; background:#0d1117; cursor:crosshair; }
  #sp { width:100%; height:90px; display:block; border-bottom:1px solid var(--line); }
  #wf { width:100%; height:300px; display:block; }
  #mk { position:absolute; top:0; bottom:0; width:2px; margin-left:-1px; background:rgba(255,255,255,.7);
        pointer-events:none; display:none; }
  #axis { display:flex; justify-content:space-between; font-size:12px; color:var(--muted);
          margin-top:4px; font-variant-numeric:tabular-nums; }
  .sub { text-align:center; color:var(--muted); font-size:12px; margin-top:12px; }
  .tune { transition: opacity .2s; }
  body.scanning .tune { opacity:.4; pointer-events:none; }
  #shits { display:grid; grid-template-columns:repeat(auto-fill, minmax(180px, 1fr)); gap:6px; margin-top:8px; }
  #shits button { display:flex; justify-content:space-between; gap:8px; font-variant-numeric:tabular-nums; }
  #shits .lvl { color:var(--muted); }
</style>
</head>
<body>
<main>
  <div class="card tune">
    <h1>Power</h1>
    <div class="row">
      <button id="pon">&#9211; On</button><button id="poff">&#9211; Off</button>
    </div>
    <div id="pstat" class="note"></div>
  </div>

  <div class="card tune">
    <h1>Frequency</h1>
    <div id="freq">--<small> MHz</small></div>
    <div class="row">
      <input id="fin" type="text" inputmode="decimal" placeholder="MHz, e.g. 7.100">
      <button id="go">Go</button>
    </div>
    <div class="row" id="steps"></div>
    <div class="row">
      <button data-d="-1">&minus; step</button>
      <button data-d="1">+ step</button>
    </div>
  </div>

  <div class="card tune">
    <h1>Mode</h1>
    <div class="row" id="modes"></div>
  </div>

  <div class="card tune">
    <h1>Bands</h1>
    <div class="sub">Ham</div>
    <div class="row" id="bands"></div>
    <div class="sub">Broadcast (SW)</div>
    <div class="row" id="swbands"></div>
  </div>

  <div class="card">
    <h1>Scan</h1>
    <div class="row">
      <label class="vol">Band <select id="sband"></select></label>
      <label class="vol">Sensitivity <select id="ssens">
        <option value="6">High</option><option value="10" selected>Normal</option><option value="15">Low</option>
      </select></label>
    </div>
    <div id="sinfo" class="note"></div>
    <div class="row">
      <button id="sgo">&#9654; Scan</button><button id="sstop" disabled>&#9632; Stop</button>
    </div>
    <div id="sstat" class="note"></div>
    <div id="shead" class="note"></div>
    <div id="shits"></div>
    <div class="note">Uses the step selected above. Audio is muted while scanning; at the end the radio
      returns to your frequency. Click a result to tune it.</div>
  </div>

  <div class="card">
    <h1>Signal</h1>
    <div id="meter"><div id="bar"></div></div>
    <div id="s">--</div>
  </div>

  <div class="card">
    <h1>Audio</h1>
    <div class="row">
      <button id="play">&#9654; Listen</button><button id="stop" disabled>&#9632; Stop</button>
      <label class="vol">Vol <input id="vol" type="range" min="0" max="1" step="0.01" value="0.8"></label>
    </div>
    <div id="astat" class="note"></div>
  </div>

  <div class="card">
    <h1>Audio waterfall</h1>
    <div class="row"><label class="vol">Span <select id="wspan"></select></label></div>
    <div id="wfwrap">
      <canvas id="sp" width="600" height="90"></canvas>
      <canvas id="wf" width="600" height="300"></canvas>
      <div id="mk"></div>
    </div>
    <div id="axis"></div>
    <div id="hover" class="note"></div>
    <div class="note">Shows the receiver passband (what you hear), not the whole band. Press Listen to start it.
      In CW, the white line is your pitch: click a signal to tune it there.</div>
  </div>

  <div id="msg"></div>
</main>

<script>
const AUDIO_RATE = __AUDIO_RATE__;
const STEPS = [100, 500, 1000, 5000, 9000, 10000, 100000, 1000000];
const MODES = ["USB","LSB","CW","CWR","AM","FM"];
// [name, low Hz, high Hz, tune Hz, mode, group]; low/high 0 = tune only (not scannable)
const BANDS = [
  ["160m",1800000,2000000,1850000,"LSB","ham"], ["80m",3500000,4000000,3700000,"LSB","ham"],
  ["40m",7000000,7300000,7100000,"LSB","ham"], ["30m",10100000,10150000,10120000,"CW","ham"],
  ["20m",14000000,14350000,14200000,"USB","ham"], ["17m",18068000,18168000,18130000,"USB","ham"],
  ["15m",21000000,21450000,21300000,"USB","ham"], ["12m",24890000,24990000,24950000,"USB","ham"],
  ["10m",28000000,29700000,28400000,"USB","ham"], ["6m",50000000,54000000,50150000,"USB","ham"],
  ["WWV 10",0,0,10000000,"AM","ham"], ["WWV 15",0,0,15000000,"AM","ham"],
  ["120m",2300000,2495000,2300000,"AM","sw"], ["90m",3200000,3400000,3200000,"AM","sw"],
  ["75m",3900000,4000000,3900000,"AM","sw"], ["60m",4750000,5060000,4750000,"AM","sw"],
  ["49m",5900000,6200000,5900000,"AM","sw"], ["41m",7200000,7450000,7200000,"AM","sw"],
  ["31m",9400000,9900000,9400000,"AM","sw"], ["25m",11600000,12100000,11600000,"AM","sw"],
  ["22m",13570000,13870000,13570000,"AM","sw"], ["19m",15100000,15800000,15100000,"AM","sw"],
  ["16m",17480000,17900000,17480000,"AM","sw"], ["15m",18900000,19020000,18900000,"AM","sw"],
  ["13m",21450000,21850000,21450000,"AM","sw"], ["11m",25670000,26100000,25670000,"AM","sw"]
];
const SW_STEP = 5000;  // broadcast channel spacing

let step = 1000, cur = {power:null, freq:0, mode:"", cwpitch:null}, busy = false;
const $ = id => document.getElementById(id);
const msg = t => { $("msg").textContent = t; };

async function api(path, body) {
  const r = await fetch(path, body ? {method:"POST", headers:{"Content-Type":"application/json"}, body:JSON.stringify(body)} : {});
  const j = await r.json();
  if (!r.ok) throw new Error(j.error || r.statusText);
  return j;
}
function fmtFreq(hz) {
  const mhz = Math.floor(hz / 1e6), khz = String(Math.floor((hz % 1e6) / 1000)).padStart(3,"0"), h = String(hz % 1000).padStart(3,"0");
  return `${mhz}.${khz}.${h}`;
}
function smeter(db) {
  if (db === null || db === undefined) return ["--", 0];
  let label, pct;
  if (db <= 0) { const s = Math.max(0, Math.round(9 + db / 6)); label = "S" + s; pct = s / 9 * 60; }
  else { label = "S9+" + db + " dB"; pct = Math.min(100, 60 + db / 60 * 40); }
  return [label, pct];
}
function render(st) {
  cur = st;
  const on = st.power === 1;
  $("pon").classList.toggle("on", on); $("poff").classList.toggle("on", st.power === 0);
  $("pstat").textContent = on ? "" : st.power === 0 ? "Radio is off" : "Radio not answering (off?)";
  $("freq").innerHTML = (on ? fmtFreq(st.freq) : "--") + "<small> MHz</small>";
  document.querySelectorAll("#modes button").forEach(b => b.classList.toggle("on", on && b.dataset.m === st.mode));
  const [lbl, pct] = smeter(on ? st.strength : null);
  $("s").textContent = st.scanning ? "scanning…" : lbl; $("bar").style.width = pct + "%";
  if (on) followBand(st.freq);
  wfMarker();
}
async function poll() {
  if (busy) return;
  try { render(await api("/api/status")); msg(""); }
  catch (e) { msg("No connection to the radio: " + e.message); }
}
async function setFreq(hz) {
  busy = true;
  try { await api("/api/freq", {freq: Math.round(hz)}); cur.freq = Math.round(hz); render(cur); }
  catch (e) { msg("Error: " + e.message); }
  busy = false; setTimeout(poll, 150);
}
async function setMode(m) {
  busy = true;
  try { await api("/api/mode", {mode: m}); cur.mode = m; render(cur); }
  catch (e) { msg("Error: " + e.message); }
  busy = false; setTimeout(poll, 150);
}
async function setPower(on) {
  if (!on && !confirm("Turn the radio off? You can turn it back on from here.")) return;
  busy = true; $("pon").disabled = $("poff").disabled = true;
  $("pstat").textContent = on ? "Turning on… (takes a few seconds)" : "Turning off…";
  try { await api("/api/power", {on}); msg(""); }
  catch (e) { msg("Power: " + e.message); }
  busy = false; $("pon").disabled = $("poff").disabled = false;
  setTimeout(poll, 500);
}
$("pon").onclick = () => setPower(true);
$("poff").onclick = () => setPower(false);

const stepLabel = s => s >= 1e6 ? (s/1e6)+" MHz" : s >= 1000 ? (s/1000)+" kHz" : s+" Hz";
function setStep(s) {
  step = s;
  document.querySelectorAll("#steps button").forEach(x => x.classList.toggle("on", Number(x.dataset.s) === s));
  scanInfo();
}
STEPS.forEach(s => {
  const b = document.createElement("button");
  b.textContent = stepLabel(s); b.dataset.s = s; b.onclick = () => setStep(s);
  $("steps").appendChild(b);
});
MODES.forEach(m => {
  const b = document.createElement("button"); b.textContent = m; b.dataset.m = m; b.onclick = () => setMode(m);
  $("modes").appendChild(b);
});
BANDS.forEach(([n, lo, hi, f, m, g]) => {
  const b = document.createElement("button"); b.textContent = n;
  b.onclick = async () => {
    const i = scanBands.findIndex(x => x[0] === n && x[5] === g);
    if (i >= 0) $("sband").value = i;   // the scan follows the band you pick
    if (g === "sw") setStep(SW_STEP); else scanInfo();
    await setMode(m); await setFreq(f);
  };
  $(g === "sw" ? "swbands" : "bands").appendChild(b);
});
document.querySelectorAll("button[data-d]").forEach(b => b.onclick = () => setFreq(cur.freq + step * Number(b.dataset.d)));
$("go").onclick = () => {
  const v = parseFloat($("fin").value.replace(",", "."));
  if (isNaN(v)) return msg("Invalid frequency");
  setFreq(v * 1e6); $("fin").value = "";
};
$("fin").addEventListener("keydown", e => { if (e.key === "Enter") $("go").click(); });
$("freq").addEventListener("wheel", e => {
  e.preventDefault();
  setFreq(cur.freq + (e.deltaY < 0 ? step : -step));
}, {passive:false});

/* ---------------- Audio: raw PCM over HTTP + Web Audio ----------------
   Blocks are scheduled back to back on the AudioContext clock. The queue ahead
   of the playhead is kept between AUD_CUSHION (after an underrun) and AUD_MAX
   (blocks beyond that are dropped), so latency cannot grow over time.
   Graph: sources -> bus -> volume -> speakers
                         \-> analyser (waterfall, independent of the volume) */
const AUD_BLOCK = Math.round(AUDIO_RATE * 0.04), AUD_CUSHION = 0.12, AUD_MAX = 0.5;
let aud = null, volume = 0.8;
const audGain = () => scan.running ? 0 : volume;  // silent while scanning

function audioStart() {
  if (aud) return;
  const ctx = new (window.AudioContext || window.webkitAudioContext)();
  ctx.resume();
  const bus = ctx.createGain(), gain = ctx.createGain(), an = ctx.createAnalyser();
  gain.gain.value = audGain(); gain.connect(ctx.destination);
  an.fftSize = WF_FFT; an.smoothingTimeConstant = 0.2;
  bus.connect(gain); bus.connect(an);
  aud = {ctx, bus, gain, an, spec: new Float32Array(an.frequencyBinCount),
         ctl: null, t: 0, buf: new Float32Array(AUD_BLOCK), n: 0, odd: null, shown: 0};
  $("play").disabled = true; $("stop").disabled = false;
  $("astat").textContent = "Connecting…";
  wfStart();
  audioLoop(aud);
}
function audioStop() {
  const a = aud;
  if (!a) return;
  aud = null;
  wfStop();
  if (a.ctl) a.ctl.abort();
  a.ctx.close();
  $("play").disabled = false; $("stop").disabled = true; $("astat").textContent = "";
}
async function audioLoop(a) {
  while (aud === a) {
    a.ctl = new AbortController();
    try {
      const r = await fetch("/api/audio", {signal: a.ctl.signal, cache: "no-store"});
      if (!r.ok) throw new Error((await r.json().catch(() => ({}))).error || r.statusText);
      const rd = r.body.getReader();
      for (;;) {
        const {value, done} = await rd.read();
        if (done || aud !== a) break;
        audioFeed(a, value);
      }
      if (aud === a) $("astat").textContent = "Stream ended · reconnecting…";
    } catch (e) {
      if (aud !== a) return;
      $("astat").textContent = "Audio: " + e.message + " · retrying…";
    }
    a.t = 0; a.n = 0; a.odd = null;
    await new Promise(res => setTimeout(res, 1000));
  }
}
function audioFeed(a, bytes) {
  let b = bytes;
  if (a.odd !== null) { const m = new Uint8Array(b.length + 1); m[0] = a.odd; m.set(b, 1); b = m; a.odd = null; }
  if (b.length & 1) { a.odd = b[b.length - 1]; b = b.subarray(0, b.length - 1); }
  const dv = new DataView(b.buffer, b.byteOffset, b.length);
  for (let i = 0; i < b.length; i += 2) {
    a.buf[a.n++] = dv.getInt16(i, true) / 32768;
    if (a.n === AUD_BLOCK) { audioPlay(a); a.n = 0; }
  }
}
function audioPlay(a) {
  const ctx = a.ctx, now = ctx.currentTime;
  if (a.t < now) a.t = now + AUD_CUSHION;    // underrun (or first block): rebuild a small cushion
  else if (a.t - now > AUD_MAX) return;      // too far behind live: drop this block
  const ab = ctx.createBuffer(1, AUD_BLOCK, AUDIO_RATE);
  ab.getChannelData(0).set(a.buf);
  const src = ctx.createBufferSource();
  src.buffer = ab; src.connect(a.bus); src.start(a.t);
  a.t += AUD_BLOCK / AUDIO_RATE;
  if (now - a.shown > 0.5) {
    a.shown = now;
    $("astat").textContent = `Playing · buffer ${Math.round((a.t - now) * 1000)} ms`;
  }
}
$("play").onclick = audioStart;
$("stop").onclick = audioStop;
$("vol").addEventListener("input", e => {
  volume = Number(e.target.value);
  if (aud) aud.gain.gain.value = audGain();
});

/* ---------------- Audio waterfall ----------------
   Every WF_ROW_MS the analyser spectrum (0..span Hz) is mapped to WF_W columns
   (max of the FFT bins under each column, so narrow CW carriers aren't lost),
   coloured relative to a slowly tracked noise floor, and pushed in at the top. */
const WF_FFT = 4096, WF_W = 600, WF_H = 300, WF_ROW_MS = 50, WF_RANGE_DB = 50;
const WF_SPANS = [...new Set([3000, 4000, AUDIO_RATE / 2])].filter(s => s <= AUDIO_RATE / 2);
let wfSpan = WF_SPANS[0], wfFloor = null, wfTimer = null;
const wfRow = new Float32Array(WF_W);

function color(t) {
  const PAL = [[0,[8,10,40]],[0.25,[20,50,160]],[0.5,[0,170,200]],[0.75,[240,220,60]],[1,[240,60,40]]];
  t = Math.min(1, Math.max(0, t));
  for (let i = 1; i < PAL.length; i++) {
    if (t <= PAL[i][0]) {
      const [t0, c0] = PAL[i-1], [t1, c1] = PAL[i], k = (t - t0) / (t1 - t0);
      return c0.map((v, j) => v + (c1[j] - v) * k);
    }
  }
  return PAL[PAL.length - 1][1];
}
const LUT = Array.from({length: 256}, (_, i) => color(i / 255));
const wfCtx = $("wf").getContext("2d"), spCtx = $("sp").getContext("2d");
const wfLine = wfCtx.createImageData(WF_W, 1);

function wfStart() { wfStop(); wfFloor = null; wfTimer = setInterval(wfTick, WF_ROW_MS); }
function wfStop() { if (wfTimer) clearInterval(wfTimer); wfTimer = null; }
function wfTick() {
  const a = aud;
  if (!a) return;
  a.an.getFloatFrequencyData(a.spec);
  const binsPerHz = WF_FFT / a.ctx.sampleRate;
  for (let x = 0; x < WF_W; x++) {
    const b0 = Math.floor(x / WF_W * wfSpan * binsPerHz);
    const b1 = Math.max(b0, Math.ceil((x + 1) / WF_W * wfSpan * binsPerHz) - 1);
    let m = -200;
    for (let b = b0; b <= b1; b++) if (a.spec[b] > m) m = a.spec[b];  // -Infinity (silence) stays -200
    wfRow[x] = m;
  }
  const floor = Float32Array.from(wfRow).sort()[Math.floor(WF_W * 0.2)];
  // track the floor slowly, but jump when it rises a lot (first audio after silence)
  wfFloor = wfFloor === null || floor > wfFloor + 20 ? floor : wfFloor + (floor - wfFloor) * 0.05;
  const lo = wfFloor - 3, k = 255 / WF_RANGE_DB;
  // waterfall: scroll down one pixel, new line on top
  wfCtx.drawImage($("wf"), 0, 0, WF_W, WF_H - 1, 0, 1, WF_W, WF_H - 1);
  const d = wfLine.data;
  for (let x = 0; x < WF_W; x++) {
    const c = LUT[Math.max(0, Math.min(255, Math.round((wfRow[x] - lo) * k)))];
    d[x*4] = c[0]; d[x*4+1] = c[1]; d[x*4+2] = c[2]; d[x*4+3] = 255;
  }
  wfCtx.putImageData(wfLine, 0, 0);
  // spectrum line
  const H = $("sp").height;
  spCtx.clearRect(0, 0, WF_W, H);
  spCtx.beginPath();
  for (let x = 0; x < WF_W; x++) {
    const y = H - 3 - Math.min(1, Math.max(0, (wfRow[x] - lo) / WF_RANGE_DB)) * (H - 6);
    if (x) spCtx.lineTo(x, y); else spCtx.moveTo(x, y);
  }
  spCtx.strokeStyle = "#5aa0e6"; spCtx.lineWidth = 1.5; spCtx.stroke();
}
function wfAxis() {
  const parts = [];
  for (let i = 0; i <= 4; i++) parts.push(`<span>${Math.round(wfSpan * i / 4)}${i === 4 ? " Hz" : ""}</span>`);
  $("axis").innerHTML = parts.join("");
}
const isCW = () => cur.mode === "CW" || cur.mode === "CWR";
function wfMarker() {
  const mk = $("mk");
  if (!isCW() || !cur.cwpitch || cur.cwpitch > wfSpan) { mk.style.display = "none"; return; }
  mk.style.display = "block"; mk.style.left = (cur.cwpitch / wfSpan * 100) + "%";
}
WF_SPANS.forEach(s => {
  const o = document.createElement("option"); o.value = s; o.textContent = (s / 1000) + " kHz";
  $("wspan").appendChild(o);
});
$("wspan").onchange = () => { wfSpan = Number($("wspan").value); wfAxis(); wfMarker(); };
wfAxis();

const audioHzAt = e => {
  const r = $("wfwrap").getBoundingClientRect();
  return Math.min(1, Math.max(0, (e.clientX - r.left) / r.width)) * wfSpan;
};
// Retune by the offset between the clicked tone and the CW pitch. Raising the VFO
// lowers the tones in CW (upper sideband) and raises them in CWR.
const cwDelta = f => Math.round((f - cur.cwpitch) / 10) * 10 * (cur.mode === "CWR" ? -1 : 1);
$("wfwrap").addEventListener("mousemove", e => {
  const f = audioHzAt(e);
  let t = `${Math.round(f)} Hz`;
  if (isCW() && cur.cwpitch) t += ` · click to tune it to your ${cur.cwpitch} Hz pitch (${cwDelta(f) >= 0 ? "+" : ""}${cwDelta(f)} Hz)`;
  $("hover").textContent = t;
});
$("wfwrap").addEventListener("mouseleave", () => { $("hover").textContent = ""; });
$("wfwrap").addEventListener("click", e => {
  if (!isCW() || !cur.cwpitch) return;
  const dlt = cwDelta(audioHzAt(e));
  if (dlt) setFreq(cur.freq + dlt);
});

/* ---------------- Scan ----------------
   One pass over the selected band at the current step; the server finds the
   active frequencies as it goes and the list below grows while it runs. */
const SCAN_MAX = 1000, SCAN_PT_S = 0.17;  // server limit; ~seconds per point (settle + CAT)
const scanBands = BANDS.filter(b => b[2] > b[1]);
let scan = {running: false, hits: []}, scanWas = false, hitsKey = "";

scanBands.forEach((b, i) => {
  const o = document.createElement("option"); o.value = i;
  o.textContent = `${b[5] === "sw" ? "SW " : ""}${b[0]} (${(b[1]/1e6).toFixed(3)}–${(b[2]/1e6).toFixed(3)})`;
  $("sband").appendChild(o);
});
function scanInfo() {
  if (scan.running) return;
  const b = scanBands[$("sband").value], n = Math.floor((b[2] - b[1]) / step) + 1;
  const bad = n < 2 ? "the step is larger than the band"
            : n > SCAN_MAX ? `too many (max ${SCAN_MAX}): choose a larger step` : "";
  $("sinfo").textContent = `${n} points at ${stepLabel(step)}` + (bad ? ": " + bad : ` · ~${Math.ceil(n * SCAN_PT_S)} s`);
  $("sgo").disabled = !!bad;
}
function followBand(hz) {  // keep the scan band on the band you're tuned to
  if (scan.running) return;
  const sel = scanBands[$("sband").value];
  if (sel && hz >= sel[1] && hz <= sel[2]) return;
  const i = scanBands.findIndex(b => hz >= b[1] && hz <= b[2]);
  if (i >= 0) { $("sband").value = i; scanInfo(); }
}
function renderHits(s) {
  const k = s.hits.length;
  $("shead").textContent = k ? `${k} active frequenc${k === 1 ? "y" : "ies"}` : (s.n && !s.running ? "No activity found" : "");
  const key = JSON.stringify(s.hits);
  if (key === hitsKey) return;  // rebuild only on change, so clicks don't hit a vanishing button
  hitsKey = key;
  const box = $("shits"); box.innerHTML = "";
  s.hits.forEach(h => {
    const b = document.createElement("button");
    b.innerHTML = `<span>${fmtFreq(h.freq)}</span><span class="lvl">${smeter(h.level)[0]}</span>`;
    b.onclick = () => tuneHit(h.freq);
    box.appendChild(b);
  });
}
async function pollScan() {
  let s;
  try { s = await api("/api/scan"); } catch (e) { return; }
  scan = s;
  document.body.classList.toggle("scanning", s.running);
  $("sstop").disabled = !s.running; $("sband").disabled = $("ssens").disabled = s.running;
  if (s.running) $("sgo").disabled = true; else scanInfo();
  if (aud) aud.gain.gain.value = audGain();
  let t = "";
  if (s.running) t = `Scanning ${fmtFreq(s.cur_freq || s.lo)} MHz · ${s.pos}/${s.n}` + (s.floor !== null ? ` · floor ${smeter(s.floor)[0]}` : "");
  else if (s.error) t = "Scan error: " + s.error;
  else if (s.n) t = s.pos < s.n ? `Stopped at ${s.pos}/${s.n}` : `Done · ${s.n} points`;
  $("sstat").textContent = t;
  renderHits(s);
  if (scanWas && !s.running) setTimeout(poll, 200);  // back on your frequency
  scanWas = s.running;
}
async function stopScan() {
  try { await api("/api/scan/stop", {}); } catch (e) { msg("Error stopping: " + e.message); }
  await pollScan();
}
async function tuneHit(f) {
  if (scan.running) await stopScan();
  await setFreq(f);
}
$("sgo").onclick = async () => {
  const b = scanBands[$("sband").value];
  try { await api("/api/scan/start", {lo: b[1], hi: b[2], step, margin: Number($("ssens").value)}); msg(""); }
  catch (e) { msg("Scan: " + e.message); }
  await pollScan();
};
$("sstop").onclick = stopScan;
$("sband").onchange = scanInfo;
setStep(step);
(function sloop() { pollScan().finally(() => setTimeout(sloop, scan.running ? 500 : 3000)); })();

// next status poll only after the previous one answered (slow while the radio is off)
(function loop() { poll().finally(() => setTimeout(loop, 1000)); })();
</script>
</body>
</html>
"""


class Handler(BaseHTTPRequestHandler):
    server_version = "FTDX10Web/3.0"

    def log_message(self, fmt, *args):
        pass

    def _authorized(self):
        if not WEB_AUTH:
            return True
        h = self.headers.get("Authorization", "")
        if h.startswith("Basic "):
            try:
                given = base64.b64decode(h[6:]).decode()
                return hmac.compare_digest(given, WEB_AUTH)
            except Exception:
                return False
        return False

    def _deny(self):
        self.send_response(401)
        self.send_header("WWW-Authenticate", 'Basic realm="FTDX10"')
        self.end_headers()

    def _json(self, code, obj):
        data = json.dumps(obj).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        if not self._authorized():
            return self._deny()
        u = urlparse(self.path)
        if u.path in ("/", "/index.html"):
            html = PAGE.replace("__AUDIO_RATE__", str(AUDIO_RATE)).encode()
            self.send_response(200)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.send_header("Content-Length", str(len(html)))
            self.end_headers()
            self.wfile.write(html)
        elif u.path == "/api/status":
            try:
                self._json(200, get_status())
            except Exception as e:
                self._json(502, {"error": str(e)})
        elif u.path == "/api/audio":
            self._audio()
        elif u.path == "/api/scan":
            self._json(200, SCANNER.snapshot())
        else:
            self._json(404, {"error": "not found"})

    def _audio(self):
        """Endless raw PCM stream (s16le, mono, AUDIO_RATE); ends when the client leaves."""
        try:
            q = AUDIO.subscribe()
        except OSError as e:
            return self._json(502, {"error": f"Could not start ffmpeg: {e}"})
        try:
            self.send_response(200)
            self.send_header("Content-Type", "application/octet-stream")
            self.send_header("Cache-Control", "no-store")
            self.end_headers()
            self.connection.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
            while True:
                block = q.get(timeout=5)
                if block is None:
                    break
                self.wfile.write(block)
        except (OSError, queue.Empty):
            pass  # client left, or no audio for 5 s
        finally:
            AUDIO.unsubscribe(q)

    def do_POST(self):
        if not self._authorized():
            return self._deny()
        try:
            path = urlparse(self.path).path
            n = int(self.headers.get("Content-Length", 0))
            body = json.loads(self.rfile.read(n) or b"{}")
            if path == "/api/freq":
                set_freq(body["freq"])
            elif path == "/api/mode":
                set_mode(body["mode"])
            elif path == "/api/power":
                set_power(body["on"])
            elif path == "/api/scan/start":
                points = SCANNER.start(body["lo"], body["hi"], body["step"], body.get("margin", 10))
                return self._json(200, {"ok": True, "points": points})
            elif path == "/api/scan/stop":
                SCANNER.stop()
            else:
                return self._json(404, {"error": "not found"})
            self._json(200, {"ok": True})
        except (ValueError, KeyError, TypeError) as e:
            self._json(400, {"error": str(e)})
        except Conflict as e:
            self._json(409, {"error": str(e)})
        except Exception as e:
            self._json(502, {"error": str(e)})


if __name__ == "__main__":
    def _term(*_):
        raise KeyboardInterrupt

    signal.signal(signal.SIGTERM, _term)
    print(f"FTDX10 web at http://{WEB_BIND}:{WEB_PORT}  (rigctld {RIG_HOST}:{RIG_PORT})")
    httpd = ThreadingHTTPServer((WEB_BIND, WEB_PORT), Handler)
    try:
        httpd.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        SCANNER.stop()  # restores the frequency if a scan was running
        AUDIO.close()
        httpd.server_close()

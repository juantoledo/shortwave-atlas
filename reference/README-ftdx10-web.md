# Remote FTDX10 (receive only): setup

Pieces:
1. `rigctld` (Hamlib): talks to the radio over the USB CAT port.
2. `ftdx10_web.py`: the web page (port 8080). It also captures the radio's audio
   (USB codec, via `ffmpeg`) and streams it to the page with low latency (~0.2 s).

For a quick manual start of both, use `./start.sh`.

## 1. Install Hamlib and serial port permissions
```
sudo apt install libhamlib-utils
sudo usermod -aG dialout jtoledoc      # log out and back in
```

## 2. Find the model and the port
```
rigctl -l | grep -i ftdx10
ls -l /dev/serial/by-id/
```
- If `grep` doesn't show the FTDX10, your Hamlib is too old: install version 4.5 or newer (build it from github.com/Hamlib/Hamlib).
- The FTDX10 creates **two** ports (Enhanced and Standard). They are usually `...-if00-port0` and `...-if01-port0`. Try one first and, if it doesn't respond, the other.
- In the radio menu find the **CAT RATE** option and note the speed (usually 38400). It must match `-s` below.

## 3. Test rigctld by hand
Replace `1042` with the number `rigctl -l` showed and adjust port and speed:
```
rigctld -m 1042 -r /dev/serial/by-id/usb-Silicon_Labs_CP2105_...-if00-port0 -s 38400 -T 127.0.0.1 -t 4532
```
In another terminal:
```
rigctl -m 2 -r 127.0.0.1:4532 f
```
It should return the radio's current frequency. If it responds, stop rigctld (Ctrl+C) and continue.

## 4. systemd services

`/etc/systemd/system/rigctld-ftdx10.service`
```
[Unit]
Description=rigctld FTDX10
After=network.target

[Service]
ExecStart=/usr/bin/rigctld -m 1042 -r /dev/serial/by-id/CAT_PORT -s 38400 -T 127.0.0.1 -t 4532
Restart=always
RestartSec=5
User=jtoledoc
Group=dialout

[Install]
WantedBy=multi-user.target
```

`/etc/systemd/system/ftdx10-web.service`
```
[Unit]
Description=FTDX10 web
After=network.target rigctld-ftdx10.service
Wants=rigctld-ftdx10.service

[Service]
ExecStart=/usr/bin/python3 /home/jtoledoc/ftdx10_web.py
Environment=WEB_PORT=8080
Environment=AUDIO_DEVICE=plughw:CARD=CODEC,DEV=0
Environment=AUDIO_RATE=16000
# Uncomment and change to require a user and password:
# Environment=WEB_AUTH=jtoledoc:A_LONG_PASSWORD
Restart=always
RestartSec=5
User=jtoledoc

[Install]
WantedBy=multi-user.target
```

Enable:
```
sudo systemctl daemon-reload
sudo systemctl enable --now rigctld-ftdx10 ftdx10-web
```

## 5. Use
Open `http://PC-IP:8080` (or the Tailscale IP). Press **Listen** (the status line shows the playback buffer) and tune:
frequency typed in MHz, +/- buttons with the chosen step, mouse wheel over the frequency, modes and bands.

**Power On/Off** switches the radio through CAT (`\set_powerstat`); Off asks for confirmation. Turning
on takes a few seconds. While the radio is off, status updates are slow (Hamlib waits for the radio
to answer) and the page shows "Radio is off".

**Bands** has two rows: ham bands and SW broadcast bands (120m–11m). A broadcast band button switches
to AM and a 5 kHz step.

**Scan** steps through the selected band using the step chosen in the Frequency card, reading the
S-meter at each point (about 0.17 s per point). Frequencies that stand out over the band's noise floor
(Sensitivity: 6/10/15 dB) appear in a clickable list as they are found. Audio is muted while scanning
and tuning is locked; after one pass (or Stop) the radio returns to your frequency. Clicking a result
stops the scan if needed and tunes it. The FTDX10 has a single receiver, so a scan can't run in the
background while you listen.

The **audio waterfall** runs while you listen. It is an FFT of the received audio, so it shows the
receiver passband (about 3 kHz in SSB/CW, choose 8 kHz span for AM), not the whole band: the FTDX10
doesn't send its scope data over CAT, so Hamlib can't provide a band waterfall for it. In CW the white
line marks your CW pitch; click a signal to retune so it sits on that pitch.

## Notes
- The web page can only read/change frequency and mode and read the S-meter. It has no PTT or anything related to transmitting.
- `rigctld` listens only on 127.0.0.1, so nobody on the network can talk to it directly.
- Don't open another CAT program (flrig, WSJT-X, etc.) on the same port while `rigctld` is using it.
- Audio is uncompressed 16-bit mono PCM: 16 kHz uses 256 kbit/s per listener (fine on LAN/Tailscale; `AUDIO_RATE=12000` uses 192 kbit/s and still covers SSB/CW). `ffmpeg` runs only while someone is listening.
- The audio device can only be opened by one program: don't run the old ffmpeg -> Icecast stream at the same time (it adds several seconds of delay anyway).
- If you access it over the internet, use Tailscale (or similar) and enable `WEB_AUTH`.

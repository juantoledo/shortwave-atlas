#!/usr/bin/env bash
# Rudimentary startup for exploring: rigctld + web (the web captures the audio itself).
# Ctrl+C stops everything.
cd "$(dirname "$0")"

trap 'kill $(jobs -p) 2>/dev/null' EXIT

# 1) Radio CAT control
rigctld -m 1042 -r /dev/serial/by-id/usb-Silicon_Labs_CP2105_Dual_USB_to_UART_Bridge_Controller_01A82072-if00-port0 \
  -s 9600 -T 127.0.0.1 -t 4532 &

# Wait until rigctld accepts connections (max ~10 s)
for _ in $(seq 20); do
  (exec 3<>/dev/tcp/127.0.0.1/4532) 2>/dev/null && break
  sleep 0.5
done

# 2) Old Icecast path (several seconds of delay). It can't run together with the
#    web audio: both would open the same ALSA device.
# ffmpeg -hide_banner -loglevel warning \
#   -f alsa -ac 2 -ar 48000 -i plughw:CARD=CODEC,DEV=0 \
#   -ac 1 -ar 24000 -c:a libmp3lame -b:a 64k \
#   -content_type audio/mpeg -f mp3 icecast://source:hackme@localhost:8000/ftdx10 &

# 3) Web + low-latency audio (foreground)
AUDIO_DEVICE=plughw:CARD=CODEC,DEV=0 AUDIO_RATE=16000 python3 ftdx10_web.py

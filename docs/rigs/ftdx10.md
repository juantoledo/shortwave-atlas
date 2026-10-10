# Yaesu FTDX10

Hamlib model **1042** (needs Hamlib 4.5+; this machine has 4.5.5).

## Connection
- USB: CP2105 dual UART. CAT is the first of its two ports (the other one is not CAT):
  - Linux: `/dev/serial/by-id/...CP2105...-if00-port0`
  - Windows: the **Enhanced COM Port** (Device Manager shows it as "Silicon Labs Dual CP2105 USB to
    UART Bridge: Enhanced COM Port (COMn)")
  - macOS: `/dev/cu.SLAB_USBtoUART` with the Silicon Labs driver, else `/dev/cu.usbserial-...`.
    To be confirmed on a Mac.
- Baud: 9600 here (radio menu **CAT RATE** must match `rig.baud`).
- Audio: the rig's USB codec (listed first, as "rig codec", in **⚙ → Audio**):
  - Linux: ALSA `plughw:CARD=CODEC,DEV=0`
  - Windows: "Microphone (USB AUDIO CODEC)", or "(2- USB AUDIO CODEC)" if Windows numbered it
  - macOS: "USB AUDIO CODEC"
- Connect: **⚙ → My radio**, model **1042 · Yaesu FTDX-10**, the `...CP2105...-if00-port0` port,
  **9600**, then **Connect**. To set it by hand instead, see `docs/swatlas.example.toml` (backend `spawn`).

## Known from the FTDX10 prototype
| Feature | Status |
|---|---|
| `f` / `F` frequency | works |
| `m` / `M` mode (USB LSB CW CWR AM FM) | works |
| `l STRENGTH` | works (approximate; fine for a bar) |
| `l CWPITCH` | works |
| `\get_powerstat` | works; returns `1`/`4` when on, takes ~6 s and fails when off |
| `\set_powerstat 0/1` | works; power-on takes several seconds (20 s timeout) |
| Band scope / waterfall over CAT | **not available**: the FTDX10 doesn't send scope data over CAT |

## To verify with Shortwave Atlas (plan step 6)
- [ ] connecting from the settings page works; the status shows the frequency
- [ ] spawn mode starts rigctld and the UI follows the VFO knob
- [ ] unplugging the USB cable shows "The serial port is gone"; plugging it back in reconnects by itself
- [ ] tuning from the UI, the dial and the station list
- [ ] mode changes; S-meter moves
- [ ] power off from the UI, off-state shows "Off", power on again
- [ ] rigctld restart after unplugging/replugging the USB cable
- [ ] remote audio + waterfall over Tailscale (buffer ~0.2 s)

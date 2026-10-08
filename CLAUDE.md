# SW Atlas

Rig (Hamlib `rigctld`) → Rust core → React UI (Tauri desktop, or browser via `swatlas-server`).
See README.md for layout and commands.

## Rules
- **Receive only.** Never add PTT or anything that transmits. `atlas_rig::hamlib::Cmd` is the
  closed whitelist of CAT commands; extend it deliberately.
- One API for both clients: add a `Call` variant in `crates/atlas-core/src/api.rs`, handle it in
  `Atlas::call` (`crates/atlas-server/src/app.rs`), and add a method in `ui/src/api/client.ts`.
  Do not add Tauri-only commands or WebSocket-only messages. The one exception is the rig audio
  PCM, a transport stream rather than a `Call`: `GET /api/audio` (browser) and the Tauri
  `audio_open`/`audio_close` channel (desktop) carry the same `AudioHub` blocks.
- Shared types are generated: `cargo test` writes `ui/src/types/generated/` (ts-rs). Commit them
  and never edit them by hand.
- Logic that doesn't touch IO goes in `atlas-core`, with unit tests. The UI only keeps animation
  math (`ui/src/geo/geo.ts`).
- Linux, Windows and macOS. Per-OS rules (device names, ffmpeg inputs, error strings, tool
  lookup) are pure functions in `atlas-core` that take an `atlas_core::platform::Os`, so all three
  variants are tested on any machine. IO code passes `Os::CURRENT`; no `cfg!` in the logic.
- Start helper programs only with `atlas_rig::process::command` (it finds sidecars and hides the
  Windows console). The Windows and macOS sidecars come from `assets/sidecars/build.py` (pinned
  versions; never commit the binaries).
- UI strings go in `ui/src/i18n/en.ts` and `es.ts` (same keys; a test enforces it).
- The globe texture is drawn at runtime (`ui/src/geo/texture.ts`). Its two rasters are generated
  by `assets/make_globe_rasters.py`; regenerate them rather than editing the JPEGs.
- `reference/` holds the prototypes, kept unchanged as a reference.

## Commands
- `cargo test` (core, rig and server; the real-rigctld test skips without `rigctld`)
- `cargo clippy --all-targets`
- Windows/macOS compile check (no SDKs needed): `cargo check -p atlas-core -p atlas-rig -p atlas-server
  --all-targets --target x86_64-pc-windows-msvc` (and `aarch64-apple-darwin`)
- `npm --prefix ui test`, `npm --prefix ui run typecheck`
- `npm run dev` (desktop), `npm run server` (headless + remote UI on :8080)
- Toolchain: Node 22 via nvm (`~/.nvm`), Rust via rustup (`~/.cargo/bin`).

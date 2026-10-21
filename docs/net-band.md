# Internet radio in Shortwave Atlas: the NET band

How [radio-browser.info](https://www.radio-browser.info) could come into Shortwave Atlas: the
legal position, the data, the decisions taken so far, and a first step that leaves today's
features unchanged.

Status (October 2026): steps 1 to 3 are implemented: browsing (§8, §11), listening with its own
history (§10), and tuning like a radio (§13). Everything after that is still design notes.

---

## 1. Summary
- **What it is.** radio-browser.info is a free, community-run directory of about 50 000 internet
  radio streams with a public JSON API. Its data is reportedly public domain.
- **Built around real radio.** The app is built around EiBi schedules, transmitter sites, a rig and
  DX statistics. Internet radio comes in as the **NET band**: it looks like a band to the listener
  and is a separate source inside the app.
- **Share the source, not the catalog.** NET stations get the same kind of identity as EiBi and
  Mine (`StationKey { source, key }`). They do not go into EiBi's frequency-indexed `Catalog`.
- **First step: browse only.** "NET" becomes a choice in the station list's band filter. Choosing
  it queries radio-browser live with the existing filters (text, country). Playback, a NET dial,
  favorites and EiBi↔NET links come later.
- **Today's experience must not change.** Stats, listening history, heat strip, filters, lenses,
  globe, tuning and scans all behave exactly as now. NET is off by default, and it's kept out of
  the rig and the listening history (§9).

---

## 2. radio-browser.info: what it offers

### The API
Source: [api.radio-browser.info](https://api.radio-browser.info/).
- **Server discovery.** Look up `all.api.radio-browser.info` in DNS (or the SRV record
  `_api._tcp.radio-browser.info`). Shuffle the list, use the first server, and on failure try the
  next. **Never hard-code a server.** Prefer HTTPS.
- **User-Agent.** Send a descriptive one, such as `swatlas/<version>`, which the update check
  already sends.
- **Clicks.** Call `/json/url/{stationuuid}` each time the user plays a station.
- **Identifiers.** Use `stationuuid`, `checkuuid` and `clickuuid`, and `countrycode`
  (ISO 3166-1 alpha-2). `id` and `country` are deprecated.
- The API page states no rate limits.

### Station fields
A live response has these 39 fields:

```
changeuuid, stationuuid, serveruuid, name, url, url_resolved, homepage, favicon, tags,
country, countrycode, iso_3166_2, state, language, languagecodes, votes, lastchangetime,
lastchangetime_iso8601, codec, bitrate, hls, lastcheckok, lastchecktime,
lastchecktime_iso8601, lastcheckoktime, lastcheckoktime_iso8601, lastlocalchecktime,
lastlocalchecktime_iso8601, clicktimestamp, clicktimestamp_iso8601, clickcount, clicktrend,
ssl_error, geo_lat, geo_long, geo_distance, has_extended_info
```

**There is no frequency field.**

### What the data looks like
From live queries:

| Observation | Example | Consequence |
|---|---|---|
| Real frequencies are often in the **name** | `La 100 - 99.9 FM - Grupo Clarín`, `90.1 MHz FM EL CHUBUT`, `Emisora del Este 98.9 FM y 98.1 FM` | A frequency can be parsed out of names (sample biased: searched for "fm") |
| Web-only spin-offs borrow a frequency | `FM Horizonte (revival 101.9)` | The parser needs a "web-only" outcome |
| **Coordinates are mostly null** | almost every station in the Argentine sample had `geo_lat`/`geo_long` null | Place comes from `countrycode`, `state` and `iso_3166_2` |
| Duplicates | `Rock & Pop 95.9` twice, `Nacional Clásica 96.7` twice | Expect several entries for one station |
| Languages are free text | `english/ españa/ russian /francia /italia,romanian` | `languagecodes` is more usable; EiBi language codes need a mapping |
| Codec often `UNKNOWN`, bitrate 0 | most CRI entries | Mostly HLS or unprobed; it affects what we can play |
| Big broadcasters are listed under other names | "voice of america" found nothing; CRI is listed as `CRI …`, `China Plus Radio` | Matching by name alone won't work (§5) |

---

## 3. Legal

The [FAQ](https://www.radio-browser.info/faq) loads its text with JavaScript, so it couldn't be read
automatically. **Confirm the points below against its wording.**

### Directory data: reportedly public domain
- The developer's landing page reportedly states *"Data license: public domain"* (quoted on the
  [moOde forum](https://moodeaudio.org/forum/showthread.php?pid=51576)). The
  [DOKK entry](https://dokk.org/radio-browser.info) says the same.
- The API page says it is free to use "in free and non free software", and that you may mirror all
  its data.
- **Consequence:** we may cache the data, store a snapshot, and use it commercially. Attribution
  isn't required, but we'll credit it anyway: *"Station directory: radio-browser.info"*.
- This is far simpler than radiomap (CC BY-NC-SA, authorship unconfirmed).

### Server code: AGPL-3.0
[LICENSE](https://raw.githubusercontent.com/segler-alex/radiobrowser-api-rust/stable/LICENSE).
The AGPL only matters if we copy the code; calling the API doesn't bring it in. Same rule as
radiomap: **use the data, never copy the code.**

### The streams themselves
- Public domain covers the **directory** (names, URLs, tags). It does not cover the **audio**,
  which belongs to each broadcaster, or station **logos**, which are trademarks.
- Opening a public stream URL puts us where a web browser or VLC is. Three things would change that:
  1. **Recording or saving streams.** Rule: **no recording of NET streams**, kept like the
     receive-only rule.
  2. **The headless server relaying a stream to other people.** `swatlas-server` exposed publicly
     would become a re-streaming service. Remote access has no password, so it warns whenever it
     listens beyond loopback (Settings → Remote access, and a log line): trusted networks and
     Tailscale only, never a public port.
  3. **App stores.** An [Apple developer forum thread](https://developer.apple.com/forums/thread/847520)
     describes an app built on radio-browser rejected three times under guideline 5.2.3. Apple
     wanted proof of rights to each stream, and a public-domain directory wasn't enough. This
     doesn't matter today (releases go out on GitHub), but it would for the Mac App Store or the
     Microsoft Store.
- **Takedowns.** Live queries, or a cache that expires, mean stations removed from the directory
  disappear from the app too. A frozen copy in the repo wouldn't. Add a local "hide this station"
  later.

### Privacy
NET sends queries to third-party servers, so it's **opt-in** (off by default), with a one-line
notice the first time it's turned on.

---

## 4. Options considered for the integration

| Option | What it is | Status |
|---|---|---|
| **A. Companion streams** | A stream is an attribute of an EiBi station: an "Also online ▶" block in the station panel. Uses: identify a weak signal by comparing it with the stream, listen online when the rig is off, show "off air but streaming". | **Later**, built on the links in §5 |
| **B. Second audio source (RIG / NET)** | A source switch in the audio dock, A/B comparison with the rig, a `via: air \| net` tag on sessions | **Later**, with playback |
| **C. NET band** | Internet radio browsable as a band of its own | **Chosen direction** |

The risk of C is turning the app into a general internet-radio player. Two measures keep it
grounded:
- the listener-friendly presentation (§7, §8)
- the links between NET and shortwave stations (§5)

---

## 5. Linking EiBi stations to NET streams

### What matches what
- EiBi has one row per frequency × time slot × language, with abbreviated names
  (`China Radio Int.`, `R.Algiers Int.`, `AIR Akashvani Ext.Sce`), ITU country codes and no website.
- radio-browser has one row per stream, with full, local or branded names, ISO country codes,
  free-text languages and a `homepage`.
- So the unit to match is **a broadcaster in one language**, not a frequency. "RRI English" links
  to the RRI English stream, and every EiBi row for RRI English follows from that.

The most frequent EiBi names show where a curated table pays off:

```
1614 Sound of Hope · 809 China Radio Int. · 239 BBC · 215 Trans World Radio
 210 CNR1 Jammer/Firedrake · 171 Voice of Korea · 149 China National Radio 1
 143 E11 Polish Spy Numbers · 124 Radio Exterior España · 118 Radio Romania Int. …
```

### Three layers
1. **Curated table** (our own file, e.g. `data/streams/links.csv`, kept like `site_overrides.csv`):
   ```
   station;itu;lang;match
   Radio Romania Int.;ROU;E;uuid:960a0c7e-…
   Radio Romania Int.;ROU;*;domain:rri.ro
   China Radio Int.;CHN;E;name:/china plus/i
   Voice of America;USA;*;domain:voanews.com
   CNR1 Jammer/Firedrake;CHN;*;none
   ```
   - A rule matches by `stationuuid`, homepage domain or name pattern. UUIDs are stable but
     stations get deleted, so a domain or pattern is the fallback.
   - `none` marks stations with no stream: jammers, number stations, utilities.
   - Around 50 broadcasters cover most of EiBi's rows.
2. **Scoring**, a pure function in `atlas-core` with tests. Signals:
   - **Name:** normalize, expanding EiBi abbreviations (`R.` → Radio, `Int.` → International,
     `Ext.Sce` → External Service), removing accents and punctuation, and generating acronyms
     (Radio Romania International ↔ RRI).
   - **Country:** ITU → ISO via `atlas_core::country`. A mismatch lowers the score rather than
     excluding the stream.
   - **Language:** EiBi code → `languagecodes`.
   - **Homepage domain:** the strongest signal outside the table.
   - **Health:** `lastcheckok` and a known codec.

   A good score shows as a **possible match**, never as confirmed.
3. **The user confirms.** "Same station? ✓ / ✗", stored per user like favorites; a ✗ is never
   suggested again. On NET, "Also on shortwave → choose station" links by hand. Confirmed links can
   be exported as rows for the curated table.

### How it would show
- **Linked:** "Also online ▶ RRI English · AAC+ 96k".
- **Possible:** shown dimmer, labelled "possible match", with the ✓ / ✗ buttons.
- **Language:** always show the stream's language next to the slot's. A stream may carry the
  domestic service or another language than the shortwave slot.

---

## 6. NET as a source

### What fits today's code
Identity across the app is `StationKey { source, key }`
(`crates/atlas-core/src/source.rs`). With `source = "radiobrowser"` and `key = stationuuid`, NET
fits without a new identity system:
- favorites
- the listening history (`Session.station: StationRef`)
- the Settings → Sources page (`SourceDesc`/`SourceInfo`: version, rows, current)
- the station list's `sources` filter (internally)
- the EiBi↔NET links of §5, which become a table between two keys

Mine could later hold streams too.

### What doesn't fit: `Catalog` is a frequency index
In `crates/atlas-core/src/stations.rs`:
- Rows are sorted by `freq_hz`. `lookup` and `near` binary-search them, and `band_of(freq_hz)`
  gives the band.
- `Station` requires `freq_hz`, `mode`, `slots` and `site`. A stream has none of these. It has a
  URL, codec, bitrate, tags and sometimes coordinates.
- "On air" comes from slots; for a stream it comes from `lastcheckok`.
- `StationRef` in listening sessions requires `freq_hz`.

### Decision: share the source, not the catalog
- **Shared:** `SourceDesc`/`SourceInfo`, `StationKey`, favorites, history, links, the country
  table.
- **NET only:** its own call and row type (`NetSearch` → `NetStation`/`NetPage`). EiBi's catalog
  and `Search` stay untouched.
- **Later change:** `StationRef` gets an optional frequency, or splits into an air variant and a
  net variant, once NET sessions exist.
- **Rejected for now:** a single `Station` with `reception: Air{…} | Net{…}` is cleaner in the long
  run, but it means a large refactor before any NET feature exists.

---

## 7. NET is not a band, but it's shown as one
A frequency does four jobs in the app. A NET stand-in has to do the same:
1. **Address:** where you tune to.
2. **Order:** the dial's axis, what counts as a neighbour, what one knob step means.
3. **Grouping:** bands.
4. **Memory:** the heat strip, scans and history are keyed by kHz.

Job 4 sets a hard rule: **a position must come from the station itself, not from its rank among
the others.** Otherwise every refresh shifts everything.

### Explored: geography as the frequency (not chosen)
- Bands are world regions (the existing `Region` in `country.rs`) and position is longitude, so
  turning the knob sweeps the globe.
- It's stable and fits the globe, but **a shortwave listener doesn't think in longitudes.**

### Preferred for later: the remote receiver
- Shortwave listeners already use KiwiSDR and WebSDR: **pick a receiver on a map, then tune its
  bands by real frequency.**
- NET would work the same way: pick a listening post (a country, a province), then tune its MW and
  FM bands at each station's **real broadcast frequency**, parsed from the name (§2).
- Readout: `NET · Buenos Aires ARG · FM 99.9 MHz · La 100`.

| Band | Contents | Position |
|---|---|---|
| MW | streams of AM stations | their kHz |
| FM | streams of FM stations (FM range by country: 87.5–108 MHz, Japan 76–95 MHz) | their MHz |
| SW | streams of EiBi broadcasters, through the links in §5 | their EiBi frequencies |
| WEB | web-only stations | no dial; memory channels by country |

**What it needs:**
- a frequency parser, pure and tested, with an overrides file and corrections by the user
- a table of province and city positions keyed by `iso_3166_2`, from a source whose licence we check
- first, **measuring how many stations per country have a usable frequency**

### Filters rather than axes
- **Genre:** tags are free text. A mapping file would reduce them to about ten genres.
- **Language:** maps to EiBi language codes.
- **Popularity:** only for sorting; it changes daily.

---

## 8. Step 1 (agreed): a NET choice in the band filter
The simplest start: `StationList.tsx`'s band filter is single-choice (`f.bands[0]`), with the metre
bands plus "out of band". **NET becomes one more choice.** Choosing it switches the list to live
results from radio-browser.

### The Source filter
The confusion of a Source dropdown offering EiBi, Mine and NET is avoided by a simple rule: **NET
appears in the UI only as a band, never as a source.**
- The Source dropdown keeps showing EiBi and Mine. Optionally relabel them "Schedule (EiBi)" and
  "Mine".
- "All bands" keeps meaning the radio bands; NET appears only when chosen.

### How each filter behaves on NET
| Filter | On NET |
|---|---|
| Search text | the station name (`name`) |
| Country | ITU → ISO (`countrycode`). The list comes from the **full** country table (`CodeTables`), not only the countries present in EiBi. Counts can come from radio-browser's country endpoint. |
| Language | Mapping EiBi codes to `languagecodes` is needed. Disabled in step 1. |
| Region | Not filterable on radio-browser (one country per query). Disabled in step 1. |
| Source | hidden |
| Lenses | All only. Heard doesn't apply. Favorites and Listened come later. |
| Paging | API `offset`/`limit` |
| Order | popularity (`clickcount`, descending) |

### What a NET row shows and does
- A "NET" badge and the codec/bitrate where the kHz would be; name, flag, language, tags.
- Clicking expands it inline (homepage, tags, last check). **It doesn't tune and doesn't change
  the Dossier.**
- A "Play" action arrives with playback (§10).

---

## 9. Keeping the existing experience unchanged

### Places where NET could leak into existing features
| Place | Risk | Guard |
|---|---|---|
| `pick()` in `ui/src/App.tsx:180` | tunes the rig and calls `hearingStation`, which credits the listening history | NET rows never go through `pick()` |
| Listening tracker (`Atlas::start`, fed by rig state) | NET time recorded as rig listening | step 1 never touches the rig. Later, while a stream plays, the rig session pauses or is marked |
| `useStationSearch` (`ui/src/hooks/useCore.ts:164`) refreshes every minute | would poll radio-browser, or run an empty `Search` with `bands:['net']` | NET has its own hook with no refresh. `useStationSearch` doesn't run while NET is selected |
| Filter state, also changed by `pickSite`, starting a scan, the Favorites button and lenses | a NET filter stuck under a globe click or a scan | those actions drop NET and return to the radio list |
| Filters NET doesn't use (language, region, source, site, scan) | lost when switching to NET | hidden and ignored on NET but kept in state, so switching back restores the exact list |
| `atlas_core::country` | changing EiBi codes | it only grows: an ISO → ITU table. Flag keys aren't unique (`us` = USA, ALS, CG7), so they can't double as ISO codes |
| Stats, heat strip, Hunt, Listened lens | NET data mixed into DX figures | nothing NET writes to `listening.jsonl` in step 1. Later, sessions carry the source and DX figures count only on-air listening |

### Guarantees
1. **Off by default.** While the setting is off there's no NET option and no network request, and
   the UI is identical to today.
2. **Separate path.** `Call::NetSearch` and `useNetSearch` are new. `Call::Search`, `Catalog`,
   `StationQuery`, `StationRow` and EiBi parsing are not modified.
3. **Rig and history untouched.** No `tune`, `set_freq` or `hearingStation` from NET, so no
   sessions and no stats changes.
4. **Filters preserved.** Switching back from NET restores the exact previous list.
5. **Tests prove it.** No existing test is edited. Every existing Rust and UI test must pass as is,
   and new tests cover the NET guarantees (§11).

---

## 10. Playback (implemented in step 2)
**As built:** the server decodes the stream in Rust and feeds the existing `AudioHub` path
(`/api/audio`, Tauri `audio_open`). The output is shared, like the rig's: starting a station
switches every window over, and tuning the rig, a scan or turning internet radio off switches back.

- **Decoding:** symphonia, pure Rust. MP3, AAC, Ogg Vorbis and FLAC play.
  - AAC+ (HE-AAC) plays its AAC core only (no SBR), at reduced high-frequency fidelity, roughly
    AM-broadcast quality.
  - Stations that can't play here are left out of the list: HLS, other codecs, and streams whose
    codec the directory doesn't know. Paging follows the directory's offsets, and an empty page
    is read past.
- **Pacing:** PCM is mono 16-bit at 32 kHz (`NET_RATE`). A `JitterBuffer` fills to its size
  before playing (and again after the stream runs dry), holds that plus 3 s (or the size again)
  at most, and hands out one block per tick, so network bursts don't reach the player.
- **Buffer size:** automatic by default: each stream starts at 2 s and goes up a step of
  `NET_BUFFER_STEPS_MS` (1, 2, 4, 8, 16 s) each time it runs dry, never down, so a station that
  keeps cutting out ends up with enough held to ride over it. The listener can pick a fixed size
  instead (`Call::SetNetBuffer`, saved as `[net] buffer_ms`; unset is automatic). It applies to
  the stream playing at once.
- **The funnel:** on the NET faceplate the buffer is drawn in the whole dial's place as a funnel
  on its side (`NetFunnel.tsx`, geometry in `funnel.ts`): the stream pours in at the mouth and
  leaves through the spout, on a log scale from 0.5 s to 32 s so each size is one equal section.
  The audio held fills it from the spout, a gate marks the size it fills to (`buffer_target_ms`),
  a dashed line where the oldest starts to go (`buffer_max_ms`). The sections' labels and an Auto
  button pick the size; while Auto is on, the size it has grown to is lit.
- **States:** `Connecting` (fetching and filling up), `Playing`, `Buffering` (it ran dry and is
  filling up again), `Failed`. The faceplate shows a chip while it connects or fills
  (`Buffering 40%`, of the size it fills to), and the UI asks every 0.5 s meanwhile.
- **Safety:** the UI sends only a station uuid. The server reads the station's details, counts the
  click (`/json/url/{uuid}`), and opens the URL only if `stream_url_ok` allows it: http(s), no
  local or private addresses.
- **Failures:** the station shows the reason, and listeners are refused for 10 s, then it's tried
  again.
- **History:** NET sessions (30 s to count, 20 s reconnect gap) go to `net-listening.jsonl`. The
  rig's sessions pause while a stream plays. Stats has its own NET card, and `ClearListening`
  clears both files.

The original notes, kept for the reasoning:
- **Same everywhere:** one transport for desktop, browser and remote clients, unchanged
  `audio/player.ts`, the waterfall working, and A/B with the rig on one clock.
- **Avoids webview limits:** playing `<audio src=http://…>` directly hits cross-origin audio
  reaching WebAudio as silence (so no waterfall), mixed-content blocking under HTTPS, and codec
  support that varies by OS (WebKitGTK on Linux).
- **Main risk: codecs.** A pure-Rust decoder (symphonia) handles MP3 and AAC-LC. HE-AAC (`aacp`,
  common) and HLS would need an ffmpeg sidecar (through `atlas_rig::process::command` and
  `assets/sidecars/build.py`) or would have to be filtered out. **Settle it with a spike first.**
- **Project rules:** CLAUDE.md would gain a line that stream audio is a second allowed transport
  stream (or the same one through `AudioHub`). Receive-only isn't affected. **No recording.**
- **Click counting:** report plays to `/json/url/{uuid}`.

---

## 11. Step 1 implementation outline

### Core (`crates/atlas-core`): pure and tested
- New `src/net.rs`:
  - `NetQuery { q, country, offset, limit }` → radio-browser query parameters (`name`,
    `countrycode`, `hidebroken=true`, `order=clickcount`, `reverse=true`, `offset`, `limit`).
  - `NetStation` (ts-rs exported) with `uuid, name, itu, flag, langs, tags, codec, bitrate,
    homepage, ok`. JSON parsing skips and counts bad rows.
  - `NetPage`. The API has no total, so offer "more" while a full page comes back.
  - `NET_SOURCE = "radiobrowser"`.
- `src/country.rs`: `itu_of_iso` and `iso_of_itu`, from an explicit table.
- `src/api.rs`: `Call::NetSearch(NetQuery)`, `Call::NetCountries` (countries with an ISO code, with
  their names, because `CatalogMeta` only names countries present in EiBi), `Call::SetNet { enabled }`,
  and `Info.net`.

### Server (`crates/atlas-server`)
- Move `fetch` and its shared `reqwest` client from `update.rs` into `http.rs`, under the same cargo
  feature: one client and one User-Agent.
- New `net.rs` with a `NetService`:
  - server discovery with retry on the next server
  - a few minutes' in-memory cache per query
  - errors mapped to `ErrorKind`
- `app.rs`: `NetSearch` (refused while NET is off) and `SetNet`, saved to `swatlas.toml` as
  `[net] enabled`, like `SetUpdatePrefs`.

### UI (`ui/src`)
- `api/client.ts`: `netSearch` and `setNet`.
- `hooks/useNetSearch.ts`: debounced, paged, no periodic refresh.
- `components/StationList.tsx`: the NET band option when `info.net` is on, NET rows, hiding the
  filters NET can't use, countries from `CodeTables`.
- `App.tsx`: choose between the normal and the NET search by band, and drop NET in `pickSite`, at
  scan start, and on Favorites.
- `components/settings/SourcesSection.tsx`: the opt-in toggle and the credit line.
- `i18n/en.ts` and `es.ts`: the new keys.
- `CLAUDE.md`: "NET is a source shown only as a band; its rows come from `NetSearch`, never from
  the catalog."

### Verification
- `cargo test`, with existing tests unchanged and green. New tests:
  - query building, and parsing a saved radio-browser JSON response
  - every mapped ISO code resolves to a real ITU row
  - `NetSearch` refused while NET is off
  - `SetNet` saved to the config file
  - after NET searches, `Listening`, `Stats` and `ListenedFreqs` are unchanged
- `cargo clippy --all-targets`, plus the Windows and macOS `cargo check` commands from CLAUDE.md.
- `npm --prefix ui test` and `typecheck`. New UI tests:
  - no NET option while `info.net` is false
  - clicking a NET row calls neither `api.rig` nor `api.hearingStation`
  - leaving NET restores the previous filters
  - the i18n key parity test
- Manual, in `npm run dev` and `npm run server`:
  1. With NET off, the app is the same as today.
  2. Turn NET on and choose Argentina: the list fills and paging works.
  3. Go back to 49m: the previous filters and list come back.
  4. With the sim rig tuned, browsing NET leaves the frequency, the Dossier, the heat strip and the
     Stats totals unchanged.

---

## 12. Roadmap and open questions
| Phase | Content |
|---|---|
| 1 | NET band in the filter, browse only (§8, §11) |
| 2 | **Done.** Playback through `AudioHub` (symphonia: MP3, AAC, Vorbis, FLAC; HE-AAC core only; no HLS), click counting, a separate NET listening history and Stats card |
| 3 | **Done.** Tuning like a radio (§13): NET favorites as memory channels, the NET station panel and faceplate, the map |
| 3b | NET favorites on the map; full HE-AAC and HLS if wanted (an ffmpeg sidecar) |
| 4 | EiBi↔NET links (curated table, scoring, user confirmation); "Also online" in the station panel; A/B with the rig |
| 5 | Remote-receiver dial: frequency parser, listening posts, MW/FM/SW/WEB bands on NET |

**Open questions:**
- Confirm the FAQ wording on the data licence and terms (§3).
- Language filter on NET: mapping EiBi codes to `languagecodes`.
- How many stations per country have a usable frequency in their name (needed before phase 5).
- Should Mine hold user-added streams?
- Where the province and city coordinates for listening posts come from, and under what licence.

---

## Sources
- [radio-browser API page](https://api.radio-browser.info/)
- [radio-browser FAQ](https://www.radio-browser.info/faq) (not readable automatically, to confirm)
- [radiobrowser-api-rust LICENSE (AGPL-3.0)](https://raw.githubusercontent.com/segler-alex/radiobrowser-api-rust/stable/LICENSE)
- [moOde forum, quoting "Data license: public domain"](https://moodeaudio.org/forum/showthread.php?pid=51576)
- [DOKK entry on radio-browser.info](https://dokk.org/radio-browser.info)
- [Apple developer forum: guideline 5.2.3 case](https://developer.apple.com/forums/thread/847520)
- Live API queries to `de1.api.radio-browser.info` (October 2026): RRI, CRI, VOA and Argentine FM
  searches
- Repo: `crates/atlas-core/src/{source,stations,country}.rs`, `crates/atlas-server/src/{app,update}.rs`,
  `ui/src/{App.tsx,hooks/useCore.ts,components/StationList.tsx}`, `data/eibi/source/sked-a26.csv`

---

## 13. Step 3: tuning like a radio (implemented)
- **Tune, then listen.**
  - A NET row click tunes the receiver to the station (`Call::NetTune`). Nothing is fetched until
    someone listens.
  - The audio panel's global ▶ Listen plays the receiver's source, the rig or the station. While
    listening, tuning another station switches at once.
  - There are no per-station Listen buttons.
- **The station panel (`NetDossier`)** shows:
  - the state (tuned, connecting, playing, failed)
  - country, languages, tags and the stream
  - distance and bearing "as the crow flies", and the Sun there
  - the directory's last check, and your time in the last 30 days
  - a website link
  - **Back to rig**
- **Favorites** are kept in `net-favorites.json`, apart from the radio favorites.
  - They are the band's **memory channels**, in the order added.
  - The Favorites lens on NET lists them, and the TopBar ★ opens them on NET.
  - Adding one takes the tuned station's details, or asks the directory without counting a click.
- **The faceplate:**
  - the station's name in the readout, unit NET, ★
  - stream chips (codec and bitrate, country)
  - the band head: ‹ NET › steps memory channels, and the dropdown goes back to a radio band
  - the dial: numbered memory channels
  - a **Buf** meter for the jitter buffer (0–5 s)
  - the span switch: + NET (a radio span goes back to the rig)
  - the knob and the tuning keys step channels
  - "Rig waiting on 9 610 kHz AM · ⟲ Back to rig"
- **The map:**
  - The camera flies to the tuned station, and a pink NET pin marks it, at its coordinates or
    approximately in its country. There is **no route arc**: a stream has no radio path.
  - The transmitter sites dim while it is tuned.
  - On the NET band, the sites give way to one tower per country with internet stations (counts
    from `/json/countries`, cached). Clicking one filters the list to that country.


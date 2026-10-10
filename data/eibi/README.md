# EiBi shortwave schedule

Shortwave Atlas's station data: the [EiBi](http://www.eibispace.de/dx/) schedule by Eike Bierwirth,
bundled into every build by `crates/atlas-server/build.rs` and parsed at startup by
`atlas_core::eibi` (about 15 ms).

| File | What it is |
|---|---|
| `source/sked-<season>.csv` | EiBi's schedule for the season (`a26` = summer 2026, `b26` = winter 2026/27), unchanged |
| `source/README.TXT` | EiBi's code tables: languages, countries, target areas, transmitter sites with coordinates, unchanged |
| `utility_patterns.txt` | Station names that are not broadcasts (aviation, coast stations, weather fax, numbers stations...), one case-insensitive regex per line |
| `site_overrides.csv` | Coordinates for sites EiBi lacks or gets wrong: `itu,code,lat,lon,name,note` (`code` empty = the country's default site) |
| `parity.json` | radiomap's published counts for the same CSV, which `cargo test` checks the import against |

The two files in `source/` are byte-for-byte copies (`.gitattributes` keeps their CRLF line endings
and Latin-1 bytes), so their SHA-256 matches the upstream files. Don't edit them: corrections go in
`site_overrides.csv` or `utility_patterns.txt`.

## What the import keeps

Rows are checked in this order, and the first rule that matches skips the row:

1. **invalid**: frequency, time or persistence code that does not parse
2. **band**: outside 1711-30000 kHz (long and medium wave)
3. **inactive**: persistence 8
4. **utility**: persistence 90 or more, a utility language (`-CW`, `-HF`, `-TS`, `-TY`...;
   `-MX`, music, is kept), or a name in `utility_patterns.txt`
5. **not a broadcast**: Days says `alt`, `harm`, `imod`, `spur`, `LSB` or `USB`
6. **no site**: no coordinates for the transmitter (see below)

The transmitter comes from the Remarks column (empty = the station's country, `k` = site `k` at
home, `/BUL-s` = site `s` in Bulgaria) and is found in this order: `site_overrides.csv`, the site in
README.TXT, the country's default (uncoded) site, then any site of that country with coordinates,
shown as "approximate" and named after the country.

Each kept row becomes a schedule slot: UTC start and end, days (daily, weekdays, `1.Sa`, `Last7`,
`15Sep`, irregular), language, target area, winter or summer only (persistence 4 and 5), valid
dates (persistence 6) and the last logged month (`[MMYY]`). The UI lists one row per frequency,
station and site, with all its slots.

Atlas keeps the same rows as [radiomap](https://github.com/juantoledo/radiomap)'s `shortwave.js`,
except the stations in `parity.json`'s `atlas_keeps` (broadcasts that radiomap's patterns catch as
utilities). `cargo test` fails when the counts drift apart.

## Updating

`.github/workflows/sync-eibi.yml` runs every Tuesday. It copies the two EiBi files from radiomap's
`data/shortwave/source` (radiomap fetches them from eibispace.de every Monday), rewrites
`parity.json`, runs the tests and opens a PR on `bot/eibi-sync`. Merging it lets release-please
cut a release: `feat` for a new season, `fix` for a refresh.

By hand:
```
git clone --depth 1 https://github.com/juantoledo/radiomap /tmp/radiomap
python3 assets/sync_eibi.py /tmp/radiomap
cargo test -p atlas-core eibi
```

When the parity test fails, the import and radiomap disagree: a new utility station (add a pattern
here), a new EiBi code (add it to `atlas_core::country`), or a format change in EiBi's files.

A running app can also read newer files without a new build: point `eibi_dir` (or
`SWATLAS_EIBI_DIR`) at a folder laid out like this one.

## Licensing

EiBi's README.TXT, section A: "All my frequency lists are free of cost, and any person is absolutely
free to download, use, copy, or distribute these files or to use them within third-party software."
The rest of this folder is part of Shortwave Atlas (MIT).

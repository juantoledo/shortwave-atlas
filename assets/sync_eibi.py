"""Copy the EiBi schedule from a radiomap checkout into Shortwave Atlas's data/eibi.

usage, from the Shortwave Atlas repo root (standard library only):
  git clone --depth 1 https://github.com/juantoledo/radiomap /tmp/radiomap
  python3 assets/sync_eibi.py /tmp/radiomap

radiomap (data/shortwave/source) keeps EiBi's sked-<season>.csv and README.TXT up to date.
This copies those two files byte for byte into data/eibi/source (EiBi's files are free to
redistribute; nothing else of radiomap is copied) and rewrites data/eibi/parity.json with
radiomap's published counts for the same CSV, which `cargo test` checks Atlas against.

Exits 1 if radiomap is mid-update (its generated data/shortwave.js is for another CSV).
On GitHub Actions it writes changed=, season_changed=, file= and radiomap_sha= to
$GITHUB_OUTPUT, and the report to report.md.
"""
import hashlib
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

ATLAS = Path(__file__).resolve().parent.parent
DEST = ATLAS / "data" / "eibi"


def fail(msg: str) -> None:
    print(f"sync_eibi: {msg}", file=sys.stderr)
    sys.exit(1)


def radiomap_meta(js: Path) -> dict:
    """The JSON object in data/shortwave.js (`const SHORTWAVE = {...};`)."""
    text = js.read_text(encoding="utf-8")
    start, end = text.find("{", text.find("const SHORTWAVE")), text.rfind("}")
    if start < 0 or end < start:
        fail(f"{js}: no `const SHORTWAVE = {{...}}`")
    return json.loads(text[start : end + 1])


def main() -> None:
    if len(sys.argv) != 2:
        fail("usage: sync_eibi.py <radiomap checkout>")
    radiomap = Path(sys.argv[1])
    src = radiomap / "data" / "shortwave" / "source"
    csvs = sorted(src.glob("sked-*.csv"))
    if len(csvs) != 1:
        fail(f"{src}: expected exactly one sked-*.csv, found {[c.name for c in csvs]}")
    csv, readme = csvs[0], src / "README.TXT"
    if not readme.is_file():
        fail(f"{readme}: missing")

    data = radiomap_meta(radiomap / "data" / "shortwave.js")
    meta = data["meta"]
    sha = hashlib.sha256(csv.read_bytes()).hexdigest()
    if meta.get("sourceFile") != csv.name or meta.get("sha256") != sha:
        fail(f"radiomap's shortwave.js is for {meta.get('sourceFile')} {meta.get('sha256')}, not {csv.name} {sha}: try again later")

    try:
        commit = subprocess.run(["git", "-C", str(radiomap), "rev-parse", "HEAD"], capture_output=True, text=True, check=True).stdout.strip()
    except (OSError, subprocess.CalledProcessError):
        commit = None

    parity_path = DEST / "parity.json"
    old = json.loads(parity_path.read_text(encoding="utf-8")) if parity_path.exists() else {}
    skipped = meta["skipped"]
    parity = {
        "note": old.get("note", "radiomap's published counts for this CSV; written by assets/sync_eibi.py."),
        "radiomap_commit": commit,
        "source_file": csv.name,
        "sha256": sha,
        "rows": meta["rows"],
        "skipped": {
            "invalid": skipped["invalid"],
            "band": skipped["band"],
            "inactive": skipped["inactive"],
            "utility": skipped["utility"],
            "not_broadcast": skipped["notBroadcast"],
            "no_site": skipped["noSite"],
        },
        "sites": len(data["sites"]),
        "precise_sites": sum(1 for s in data["sites"] if s[5] == "site"),
        "stations": len(data["stations"]),
        "day_patterns": len(data["days"]),
        "atlas_keeps": old.get("atlas_keeps", []),
    }

    dest_src = DEST / "source"
    dest_src.mkdir(parents=True, exist_ok=True)
    old_csvs = sorted(dest_src.glob("sked-*.csv"))
    season_changed = [c.name for c in old_csvs] != [csv.name]
    changed = season_changed
    for old_csv in old_csvs:
        if old_csv.name != csv.name:
            old_csv.unlink()
    for f in (csv, readme):
        target = dest_src / f.name
        if not target.exists() or target.read_bytes() != f.read_bytes():
            shutil.copyfile(f, target)
            changed = True
    # the commit alone does not count as a change
    same = {k: v for k, v in old.items() if k != "radiomap_commit"} == {k: v for k, v in parity.items() if k != "radiomap_commit"}
    if not same:
        changed = True
    if changed:
        parity_path.write_text(json.dumps(parity, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")

    report = "\n".join(
        [
            f"EiBi `{csv.name}` (sha256 `{sha[:12]}...`) from juantoledo/radiomap@{(commit or 'unknown')[:12]}.",
            "",
            f"- radiomap keeps {meta['rows']} of the rows; skipped: "
            + ", ".join(f"{k} {v}" for k, v in parity["skipped"].items()),
            f"- {parity['sites']} sites ({parity['precise_sites']} precise), {parity['stations']} stations, {parity['day_patterns']} day patterns",
            f"- new season: {'yes' if season_changed else 'no'}",
            "",
            "`cargo test` checks Atlas's import against these counts (data/eibi/parity.json).",
        ]
    )
    print(report if changed else f"{csv.name}: no change")
    if os.environ.get("GITHUB_OUTPUT"):
        with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as out:
            out.write(f"changed={'true' if changed else 'false'}\n")
            out.write(f"season_changed={'true' if season_changed else 'false'}\n")
            out.write(f"file={csv.name}\n")
            out.write(f"radiomap_sha={commit or ''}\n")
        (ATLAS / "report.md").write_text(report + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()

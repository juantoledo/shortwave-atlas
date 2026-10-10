"""Build the helper program the Windows and macOS installers ship (a Tauri sidecar).

usage: python3 assets/sidecars/build.py <target> [--out src-tauri/binaries] [--work .sidecars]

  x86_64-pc-windows-msvc   run anywhere (Hamlib's own Windows build is downloaded)
  aarch64-apple-darwin     run on macOS (Xcode command line tools)
  x86_64-apple-darwin      run on macOS (cross-compiles on Apple Silicon)
  universal-apple-darwin   both macOS ones, merged with lipo

Linux packages use the system's rigctld; nothing is bundled there.

Writes, as Tauri's `externalBin` expects:
  <out>/rigctld-<target>[.exe]    Hamlib's rigctld. Windows: the official w64 build, plus
                                  its DLLs in <out>/windows/. macOS: built static from source.
  <out>/licenses/hamlib/          its license texts (bundled, see THIRD_PARTY_NOTICES.md)

The rig audio is captured in-process (cpal): there is no audio helper to build.

Sources are pinned by version and SHA-256; downloads are cached in <work>.
"""
import argparse
import hashlib
import os
import platform
import shutil
import subprocess
import sys
import tarfile
import urllib.request
import zipfile
from pathlib import Path

HAMLIB = "4.7.2"
HAMLIB_SRC = (f"https://github.com/Hamlib/Hamlib/releases/download/{HAMLIB}/hamlib-{HAMLIB}.tar.gz",
              "ae1fcf2dbc80ea0786ea8f047b09399c3f7737d1930442f61a031708ed33e88f")
HAMLIB_W64 = (f"https://github.com/Hamlib/Hamlib/releases/download/{HAMLIB}/hamlib-w64-{HAMLIB}.zip",
              "8553bc6c5c6032e8debf99c017e98f58fed7e07e7c25d04815dc3e8bbe3304c7")
# Hamlib: just the library and rigctld, no bindings or optional libraries, static.
HAMLIB_CONFIGURE = [
    "--disable-shared", "--enable-static", "--without-readline", "--without-libusb",
    "--without-cxx-binding", "--without-xml-support", "--without-indi", "--disable-winradio",
]


def run(cmd, cwd=None, env=None):
    print("+", " ".join(str(c) for c in cmd), flush=True)
    subprocess.run([str(c) for c in cmd], cwd=cwd, env=env, check=True)


def fetch(work: Path, url: str, sha: str) -> Path:
    dest = work / url.rsplit("/", 1)[1]
    if not dest.exists() or hashlib.sha256(dest.read_bytes()).hexdigest() != sha:
        print("downloading", url, flush=True)
        with urllib.request.urlopen(url) as r:
            dest.write_bytes(r.read())
    got = hashlib.sha256(dest.read_bytes()).hexdigest()
    if got != sha:
        sys.exit(f"{dest.name}: SHA-256 {got}, expected {sha}")
    return dest


def unpack(archive: Path, into: Path) -> Path:
    """Extract a source tarball once; returns its top directory."""
    with tarfile.open(archive) as t:
        top = t.getnames()[0].split("/")[0]
        if not (into / top).exists():
            t.extractall(into, filter="data")
    return into / top


def fresh(d: Path) -> Path:
    shutil.rmtree(d, ignore_errors=True)
    d.mkdir(parents=True)
    return d


def jobs() -> str:
    return str(os.cpu_count() or 2)


def windows(work: Path, out: Path):
    target = "x86_64-pc-windows-msvc"  # the name Tauri looks for
    # rigctld: the Hamlib project's own Windows build, with the DLLs it needs
    z = fetch(work, *HAMLIB_W64)
    dlls = fresh(out / "windows")
    lic = fresh(out / "licenses" / "hamlib")
    with zipfile.ZipFile(z) as zf:
        for n in zf.namelist():
            base = n.rsplit("/", 1)[-1]
            if n.endswith("/bin/rigctld.exe"):
                (out / f"rigctld-{target}.exe").write_bytes(zf.read(n))
            elif "/bin/" in n and base.endswith(".dll"):
                (dlls / base).write_bytes(zf.read(n))
            elif base in ("COPYING.txt", "COPYING.LIB.txt", "LICENSE.txt"):
                (lic / base).write_bytes(zf.read(n))


def mac_arch(work: Path, out: Path, arch: str) -> Path:
    """rigctld for one macOS architecture (`arm64` or `x86_64`)."""
    flags = f"-arch {arch} -mmacosx-version-min={MACOS_MIN}"
    native = platform.machine() == arch
    triple = {"arm64": "aarch64-apple-darwin", "x86_64": "x86_64-apple-darwin"}[arch]

    src = unpack(fetch(work, *HAMLIB_SRC), work)
    build = fresh(work / f"hamlib-build-{arch}")
    env = dict(os.environ, CFLAGS=f"-O2 {flags}", CXXFLAGS=f"-O2 {flags}", LDFLAGS=flags)
    host = [] if native else [f"--host={arch.replace('arm64', 'aarch64')}-apple-darwin"]
    run([src / "configure", *HAMLIB_CONFIGURE, *host], cwd=build, env=env)
    run(["make", "-j", jobs()], cwd=build / "src", env=env)
    run(["make", "-j", jobs(), "rigctld"], cwd=build / "tests", env=env)
    lic = fresh(out / "licenses" / "hamlib")
    for f in ("COPYING", "COPYING.LIB", "LICENSE"):
        if (src / f).exists():
            shutil.copy(src / f, lic / f"{f}.txt")

    rig = out / f"rigctld-{triple}"
    shutil.copy(build / "tests" / "rigctld", rig)
    run(["strip", rig])  # static, with every rig backend: ~28 MB -> ~12 MB
    return rig


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("target")
    ap.add_argument("--out", default="src-tauri/binaries")
    ap.add_argument("--work", default=".sidecars")
    a = ap.parse_args()
    out, work = Path(a.out).resolve(), Path(a.work).resolve()
    out.mkdir(parents=True, exist_ok=True)
    work.mkdir(parents=True, exist_ok=True)

    if a.target == "x86_64-pc-windows-msvc":
        windows(work, out)
    elif a.target in ("aarch64-apple-darwin", "x86_64-apple-darwin"):
        mac_arch(work, out, "arm64" if a.target.startswith("aarch64") else "x86_64")
    elif a.target == "universal-apple-darwin":
        arm, x86 = mac_arch(work, out, "arm64"), mac_arch(work, out, "x86_64")
        run(["lipo", "-create", arm, x86, "-output", out / "rigctld-universal-apple-darwin"])
    else:
        sys.exit(f"unknown target {a.target}")
    print("done:", ", ".join(sorted(p.name for p in out.iterdir())))


if __name__ == "__main__":
    main()

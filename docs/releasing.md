# Releasing SW Atlas

Nothing reaches users until a person publishes a release. The path is:

```
commits on main ──► release PR ──merge──► tag + draft release ──build──► draft with installers
  (feat:, fix:)     (version,             (release-please)       (release.yml)   │ install it, try it
                     CHANGELOG)                                                   ▼
                                     users' apps ◄── updates/stable.json ◄── publish (promote.yml)
```

## Commit messages decide the version
Commits on `main` follow [Conventional Commits](https://www.conventionalcommits.org/). CI lints
them (`.commitlintrc.json`).

| Message | Next version | In the changelog |
|---|---|---|
| `fix: rigctld restart on Windows` | 0.2.0 → 0.2.1 | yes |
| `feat: update available banner` | 0.2.0 → 0.3.0 | yes |
| `feat!: new config format` (or a `BREAKING CHANGE:` line) | 0.x: next minor; 1.x+: next major | yes |
| `chore:`, `docs:`, `ci:`, `test:`, `refactor:`, `build:` | none on its own | no |

A scope is optional: `fix(windows): stop rigctld on exit`.

## Release a version
1. Push to `main` as usual. **Release PR** (`release-please.yml`) keeps a pull request called
   `chore(main): release X.Y.Z` up to date: the version in `Cargo.toml` and both `package.json`,
   the lockfiles, and the new `CHANGELOG.md` entry. Edit the PR's CHANGELOG text if you like.
2. Merge the PR when you want to release. That creates the tag `vX.Y.Z`, a **draft** release with
   the changelog as notes, and starts **Release** (`release.yml`):
   - checks: the tag matches the version, `Cargo.lock` is current, `cargo deny`, `npm audit`;
   - builds the Windows (NSIS), macOS (universal dmg) and Linux (deb, AppImage) installers, plus
     the updater bundles signed with the updater key and `latest.json`;
   - builds `swatlas-server` for each OS and runs `--version`;
   - installs and starts the desktop builds on clean runners;
   - checks `latest.json` lists all four platforms, signed; adds CycloneDX SBOMs, `SHA256SUMS`
     and build provenance attestations for every file.
3. Open the draft on GitHub, install it, try it.
4. **Publish** the release. **Promote** (`promote.yml`) copies its `latest.json` (with the
   published notes) to `updates/stable.json` and `updates/beta.json` on the `gh-pages` branch.
   Apps find it within 6 hours, or at their next start.

To rebuild a draft (for example after a flaky runner), run **Release** by hand with the tag.

## Release candidates (beta channel)
Push a commit with a `Release-As` footer, for example:

```
git commit --allow-empty -m "chore: release 0.3.0-rc.1" -m "Release-As: 0.3.0-rc.1"
```

The release PR then proposes `0.3.0-rc.1`. Merging it builds a draft marked as a **prerelease**.
Publishing it updates only `updates/beta.json`, so only apps on the Beta channel see it.
When it is good, push `Release-As: 0.3.0` the same way for the final release.

To test an upgrade end to end: install rc.1 (Settings → Updates → Beta), publish rc.2, and
check that rc.1 offers it, installs it and comes back as rc.2 with its settings. A copy can also
be pointed at any manifest with `SWATLAS_UPDATE_URL=https://.../latest.json`.

## Rollback
Run **Promote** by hand with the last good tag. `stable.json` points back to it, so nobody else
gets the bad version. Apps that already installed it stay on it until a fixed, higher version
ships (apps never downgrade). Then fix forward with a new `fix:` release.

## The updater signing key
Tauri's updater only installs files signed with the key whose public half is in
`src-tauri/tauri.conf.json` (`plugins.updater.pubkey`).

- Private key: `~/.tauri/swatlas-updater.key`, password in `~/.tauri/swatlas-updater.password`
  (on the machine that generated it). In GitHub: the secrets `TAURI_SIGNING_PRIVATE_KEY` (the
  key file's content) and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
- **Back both up offline** (a password manager is fine). If the key is lost, installed apps can
  never update again: everyone would have to reinstall by hand once.
- If it leaks: generate a new pair, ship the new public key in a release signed with the old
  key, then switch the secrets.

## One-time setup (already done once; for a fork or a new repository)
1. The repository is public (the apps download from it without a token).
2. Settings → Secrets and variables → Actions: `TAURI_SIGNING_PRIVATE_KEY`,
   `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
3. Settings → Actions → General: *Read and write permissions* and *Allow GitHub Actions to
   create and approve pull requests* (for the release PR).
4. After the first publish created `gh-pages`: Settings → Pages → *Deploy from a branch*,
   `gh-pages`, `/ (root)`. Check `https://juantoledo.github.io/shortwave-atlas/updates/stable.json`.

## Known limits
- The installers are not code-signed for Windows or macOS (SmartScreen and Gatekeeper warn on
  the first install). The updater's own signature protects updates. If OS signing is added, it
  must happen before the updater signs the files: Authenticode and Apple signing change the
  bytes the `.sig` covers.
- Pull requests opened by GitHub Actions (the release PR) don't trigger CI. CI runs on `main`
  after the merge, alongside the release build.
- `requireSignedVersion` (the updater refusing a signed old build announced as a newer one) is
  off: the Tauri CLI used here doesn't record the version in its signatures yet. Turn it on in
  `tauri.conf.json` once it does.

# Gigflow

A desktop app that helps DJs prep setlists: import any sound file, auto-detect BPM/key/energy,
build a setlist with harmonic-mixing suggestions, save a transition library, and export to USB or
Rekordbox XML.

## Installing on a Mac (Apple Silicon)

1. Open the `.dmg` and drag **Gigflow** into Applications.
2. On first launch, macOS will say *"Gigflow is damaged and can't be opened."* This is expected —
   the app isn't code-signed with an Apple Developer ID yet, so Gatekeeper blocks it instead of
   showing the usual "unidentified developer" prompt. Fix it once with:

   ```bash
   xattr -cr /Applications/Gigflow.app
   ```

   Open Terminal (Spotlight → "Terminal"), paste that line, press Enter. Gigflow will open
   normally from then on.

This build is Apple Silicon (M-series) only. Intel Macs need a separate build.

## Installing on Windows

Run the `.exe` (NSIS installer) or `.msi` — either works; the `.msi` is better for IT-managed
machines. Windows SmartScreen may warn "Windows protected your PC" since the installer isn't
signed with a code-signing certificate yet — click **More info → Run anyway**.

Windows builds come from GitHub Actions (`.github/workflows/build-windows.yml`), since a Windows
installer can't be cross-compiled from a Mac. It runs automatically on every push to `main`;
grab the result from the workflow run's **Artifacts** section, or via:

```bash
gh run list --repo isaac-goldberg/gigflow
gh run download <run-id> --repo isaac-goldberg/gigflow
```

## If Gigflow crashes

Check `~/Library/Application Support/Gigflow/crash.log` (macOS) — every panic is logged there with
a timestamp and the exact code location, even in the packaged app where there's no visible
terminal for stderr. Paste that into a bug report along with which file/action triggered it.

## Development

```bash
npm install
npm run tauri dev
```

## Building a release installer

Full step-by-step instructions for macOS and Windows 11 are in [BUILDING.md](BUILDING.md). In short:

```bash
npm run tauri build
```

Produces `src-tauri/target/release/bundle/macos/Gigflow.app` and a `.dmg` in
`src-tauri/target/release/bundle/dmg/`.

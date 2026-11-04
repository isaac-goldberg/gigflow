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

This build is Apple Silicon (M-series) only. Intel Macs and Windows need a separate build.

## Development

```bash
npm install
npm run tauri dev
```

## Building a release installer

```bash
npm run tauri build
```

Produces `src-tauri/target/release/bundle/macos/Gigflow.app` and a `.dmg` in
`src-tauri/target/release/bundle/dmg/`.

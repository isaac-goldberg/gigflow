# Building Gigflow yourself

Gigflow is a [Tauri 2](https://tauri.app) app: a Rust backend (`src-tauri/`) and a React + Vite
frontend (`src/`). `npm run tauri build` compiles both and packages an installer for the
operating system you run it on.

**A Mac can't build the Windows installer, and a Windows PC can't build the Mac app.** Each one has
to be built on its own OS. The easy way to get a Windows build without a Windows PC is the GitHub
Actions workflow (see [Option B](#option-b-github-actions-no-windows-pc-needed)).

---

## macOS (Apple Silicon)

### One-time setup

1. **Xcode command line tools** (the C compiler/linker Rust needs):
   ```bash
   xcode-select --install
   ```
2. **Rust**:
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```
   Restart the terminal afterwards, then check `rustc --version`.
3. **Node.js 20 or newer** — from [nodejs.org](https://nodejs.org) or `brew install node`.
   Check with `node --version`.

### Build

From the project root:

```bash
npm ci
npm run tauri build
```

The first build takes several minutes (it compiles every Rust dependency); later builds are much
faster. If you only changed the frontend or a little Rust, expect a minute or two.

### Where the output is

| What | Path |
|---|---|
| App | `src-tauri/target/release/bundle/macos/Gigflow.app` |
| Installer | `src-tauri/target/release/bundle/dmg/Gigflow_0.1.0_aarch64.dmg` |

(The version in the file name comes from `version` in `src-tauri/tauri.conf.json`.)

### Running a build that isn't code-signed

The app isn't signed with an Apple Developer ID, so macOS says *"Gigflow is damaged and can't be
opened"* the first time. Clear the quarantine flag once:

```bash
xattr -cr /Applications/Gigflow.app
```

(Apps you build and run on your own Mac, straight from `bundle/macos/`, usually open without this.
It matters for copies you download or send to other people.)

### Quick dev loop instead of a full build

```bash
npm install
npm run tauri dev
```

This opens the app with hot-reloading for frontend changes. If it says *Port 1420 is already in
use*, an earlier dev session is still running; close it first.

---

## Windows 11

### Option A: build on a Windows 11 PC

#### One-time setup

1. **Microsoft C++ Build Tools** — install
   [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) and tick
   the **"Desktop development with C++"** workload (this includes the Windows SDK).
2. **WebView2** — already included with Windows 11, nothing to do.
3. **Rust** — download `rustup-init.exe` from [rustup.rs](https://rustup.rs) and run it with the
   default options (the `x86_64-pc-windows-msvc` toolchain). Reopen the terminal afterwards.
4. **Node.js 20 or newer** — from [nodejs.org](https://nodejs.org).

#### Build

In PowerShell, from the project root:

```powershell
npm ci
npm run tauri build
```

#### Where the output is

| What | Path |
|---|---|
| NSIS installer (`.exe`) | `src-tauri\target\release\bundle\nsis\Gigflow_0.1.0_x64-setup.exe` |
| MSI installer | `src-tauri\target\release\bundle\msi\Gigflow_0.1.0_x64_en-US.msi` |

Either installer works. The `.msi` suits IT-managed machines. Windows SmartScreen will warn
*"Windows protected your PC"* because the installer isn't signed with a code-signing certificate:
click **More info → Run anyway**.

### Option B: GitHub Actions (no Windows PC needed)

The workflow in `.github/workflows/build-windows.yml` builds on a Windows runner in GitHub's cloud.
It runs on every push to `main`, and can also be started by hand.

1. Push to `main`, or trigger it manually:
   ```bash
   gh workflow run build-windows.yml --repo isaac-goldberg/gigflow
   ```
2. Watch it (usually 10–20 minutes):
   ```bash
   gh run list --repo isaac-goldberg/gigflow
   gh run watch <run-id> --repo isaac-goldberg/gigflow
   ```
3. Download the installers:
   ```bash
   gh run download <run-id> --repo isaac-goldberg/gigflow
   ```
   You get a `gigflow-windows` folder containing the `.msi` and the `.exe`. You can also download
   the artifact from the run's page on github.com.

---

## Things worth knowing

- **Version number**: change `version` in `src-tauri/tauri.conf.json` (and keep it in step with
  `src-tauri/Cargo.toml` and `package.json`) before a release build; it ends up in the installer's
  file name.
- **Run the tests first** (Rust backend; fast):
  ```bash
  cd src-tauri && cargo test --lib
  ```
- **Stale build errors**: if a Rust build fails at the *linking* step with odd missing symbols from
  a crate (this happened once with `anyhow`) and the code is fine, clear the app's own build cache
  and retry:
  ```bash
  cd src-tauri && cargo clean -p gigflow
  ```
- **AI genre model (optional feature)**: it needs the `llama-server` program from llama.cpp on the
  machine running Gigflow (macOS: `brew install llama.cpp`) plus a ~2.5 GB model downloaded from
  inside the app. This is a runtime requirement, not a build requirement. Everything else, including
  the transition mixing, works without it.
- **Release builds are slow on purpose**: `src-tauri/Cargo.toml` turns on link-time optimization for
  `[profile.release]`, which makes the audio analysis and mix engine run much faster at the cost of
  a longer compile.
- **Intel Macs**: a build made on Apple Silicon only runs on Apple Silicon. For an Intel build, run
  the same steps on an Intel Mac (or add the `x86_64-apple-darwin` Rust target and build with
  `npm run tauri build -- --target x86_64-apple-darwin`).

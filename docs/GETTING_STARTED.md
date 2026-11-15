# Getting started with BoothSoft

BoothSoft v0.16.0 is a **developer preview**, not a validated installer release. This guide describes building from source. See [BUILD_STATUS.md](../BUILD_STATUS.md) for the validation boundary.

## Requirements

- Current stable Rust and Cargo (Rust 2021 edition).
- Git (if cloning a repository rather than unpacking the source ZIP).
- A supported Windows, macOS, or Linux desktop session with access to graphical output.
- Native platform compiler/toolchain and development libraries for `eframe` and `nokhwa`.

### Windows

Install Rust with the MSVC toolchain and Microsoft C++ Build Tools. Device access uses Media Foundation; installed printer queues use Windows print services. For packaging an MSI, install a .NET SDK and WiX Toolset v5 as documented in [device and installer guide](DEVICES_AND_INSTALLERS.md).

### macOS

Install the Xcode Command Line Tools and stable Rust. Authorize camera access for the launching process when the OS asks. System printer queues require CUPS tools for the current printer adapter. Packaging a distributable `.app` also requires testing camera entitlement/usage descriptions, signing and notarization.

### Linux

A graphical desktop session (X11 or Wayland), V4L2 camera support, GTK-related build dependencies, and CUPS command-line utilities are needed for the existing backends. Consult the exact packages installed in [the CI workflow](../.github/workflows/native.yml) for Ubuntu 24.04. Confirm your user has permission to access the selected `/dev/video*` device.

## Run in demo mode first

From the project root:

```bash
cargo run --release -- --demo --windowed
```

This starts the native window without a physical camera and is the appropriate first smoke test. To try your own theme, install it under `BoothSoft/themes/<id>/` and run:

```bash
cargo run --release -- --demo --windowed --theme <id>
```

The bundled themes live under `themes/` beside the executable for distribution, or under the project root during development.

## Diagnose hardware

```bash
cargo run --bin boothsoft-doctor -- --json
```

The tool lists devices exposed by the OS. It does not prove their capture quality, media support, printer readiness or paper completion. Use Maintenance (`Ctrl+Shift+D`) to select a device and verify an actual photograph or printed sheet.

## Work on themes

```bash
cargo run --bin boothsoft-theme-check -- themes
```

Copy `themes/default` to a new safe lowercase folder ID. Edit only `config.toml`, `assets/*.png`, and `AGENTS.md`. Review the [theme specification](THEMING.md). You can also import the included [`warm-studio.zip`](../examples/themes/warm-studio.zip) from the app's administration UI.

## Quality checks

```bash
cargo fmt --all --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets -- -D warnings -D dead_code
cargo build --release
```

No successful run of these checks has been confirmed in this source package. A clean CI run and a device test matrix are required before describing the build as portable or release-ready.

## Common issues

**No camera appears:** check OS privacy permissions, current user access and whether the device is exposed as a webcam. Raw Canon DSLR tethering is outside the present adapter.

**Printer is listed but nothing comes out:** ensure a compatible driver/queue and correct paper/media settings, then inspect the OS spooler. Submission acceptance does not imply physical completion.

**Theme import fails:** ensure the archive includes a bounded, valid `config.toml` plus required PNGs under `assets/`. Theme names must use lowercase ASCII, digits, `-` and `_`, and archives must not contain symlinks or traversal paths.

**High CPU/RAM use:** record the OS, camera format, driver, resolution and workload; follow the [performance test procedure](PERFORMANCE.md) and report before/after measurements rather than assumptions.

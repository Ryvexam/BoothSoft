## [Unreleased]

### Added
- Camera self-repair: a camera that stops answering (cable, battery, power) is reconnected in the background (every 2 s, then 5 s); the pause screen has no button any more and goes back to the live view by itself.
- Disk guard: warning under 5 GB free, no new session under 500 MB (calm message for guests); stored events listed with their size in Data & export, deletable with confirmation.
- Event export as one ZIP (Data & export › Stored events): photos, prints, guest book videos and manifests, written in the background with a progress bar; photos and videos stored without recompression, the ZIP only appears once complete.
- Guest book switch on its own maintenance page; version shown next to "Maintenance".

## [0.17.0] - 2026-11-15

### Added
- Interface in English and French, following the computer's language (English otherwise); can be fixed in maintenance. Themes can give their texts per language (`[ui.en]`, `[ui.fr]`). Party frames now say "PARTY TIME!".
- Video guest book (unlocked in maintenance): microphone test with a live level meter, 3 s countdown, 2–30 s video message with sound, playback, keep or record again, thank-you screen. AVI (MJPEG + PCM 48 kHz) written without re-encoding (`src/avi.rs`), crash repair of `.part` files, `cpal` audio (`src/audio.rs`), maintenance page to play, delete and export messages.
- Portrait 10×15 prints: orientation Landscape / Portrait / guests' choice, Party portrait frame, frame PNG import with 3 previous frames kept, photo placed in the frame's transparent window (detected automatically), live crop guide on that window.
- Clear return to the live view after a photo: visible countdown on the review and thank-you screens (review auto-return 30 s by default, migrated from 90 s).
- Day log, crash guard (`boothsoft-guard`) with safe mode, nightly restart, unsaved-changes dialog in maintenance.

### Changed
- License: Apache-2.0 → **CPAL-1.0** (OSI-approved). Visible credit « BoothSoft, créé par Dôm'Booth — dombooth.com » required in every copy and derived work (shown at launch, on the sleep screen and in maintenance). SPDX headers in every source file.
- Release profile: `opt-level = 3`, fat LTO (−4 % executable, same CPU; ADR-0003).
- Print frames are kept as file + photo window instead of an 8.6 MB bitmap per orientation (−8.6 to −17 MB RSS).
- Prints keep their proportions: portrait prints are turned a quarter turn on landscape paper (Windows GDI), `lp -o fit-to-page` elsewhere.

### Fixed
- Windows build: print job functions moved to `windows_sys::Win32::Storage::Xps` in windows-sys 0.61 (the Windows target did not compile).

## [0.16.0] - 2026-11-09

## Documentation refresh (0.16.0 source unchanged)

- Redesigned the GitHub README with a feature matrix, guest flow, quick start, hardware status, theme configuration, architecture and honest release caveats.
- Added a first-run guide, documentation index, community standards, and GitHub bug/feature/hardware report and PR templates.
- Improved security disclosure guidance and separated private-fork provisioning from open-source release gates.
- Kept the Apache-2.0 license, Rust source and Cargo package version unchanged.


### Changed
- Centralized persistent JSON writes with unique, exclusive-create temporary files and file `sync_all` before atomic replacement.
- Bounded disk reads for settings, print ledger and job history before allocating or deserializing.
- Enforced image decoder limits on theme frames and shared pixel/format budgets for printing and contact sheets.
- Removed a redundant full-frame RGB-to-BGR allocation on Windows GDI by converting the owned buffer in place with safe row padding.
- Updated MSI/macOS version identifiers and added MSI major-upgrade handling.
- Extended regression tests, architecture, agent instructions, performance guide and ADR-0016.

### Validation
- Source-level checks only; native Cargo/Clippy, Windows MSI and hardware workflows remain unverified in this environment.

## [0.15.0] - 2026-11-09

### Correctness and resource efficiency
- Source-crop photos before resizing into a frame: avoid giant intermediate allocations when portrait and landscape aspect ratios differ sharply.
- Release the live camera texture when showing the composed still or opening maintenance.
- Close the JPEG encoder/file before replacing an encoded output; clean temporary files on failure (important for Windows).
- Persist session retakes before removing JPEGs, rolling back in-memory changes when the manifest save fails.
- Use monotonic per-process session IDs and exclusive directory creation to avoid overwriting an existing session.
- Extend deterministic image benchmarks and regression tests for extreme crops, failed JPEG replacement, failed retakes and rapid session creation.

### Validation
- Structural, TOML, asset and archive checks are local. Rust compilation, tests, Clippy and hardware measurements remain unverified without a Rust toolchain and native devices.

## [0.14.0] - 2026-11-09

### Refactoring
- Split the executable into CLI bootstrap, guest kiosk logic, and a dedicated administration module. Introduced `src/lib.rs` shared by the main application and the three developer binaries.
- Removed all `#[allow(dead_code)]` and duplicated `#[path]` source imports, plus unused session-image and quota helper functions. Strengthened CI with `cargo check --all-targets` and dead-code denial.

### Correctness and resource usage
- Roll back in-memory print job transitions if writing to disk fails; preserve Submitted when cancellation cannot be committed.
- Bound the admin contact-sheet export to one full-resolution photo at a time and eliminate an unnecessary copy for small review textures.
- Skip an immediate second camera decode after a captured still; await camera events or timeout instead of polling every 200 ms during capture.
- Enforce an exact upper bound of 5% extra paper copies by rounding DOWN (floor), superseding ADR-0007's ceiling rule; updated regression tests.

### Verification limits
- Runtime and performance tests were not executed in this sandbox because Rust/Cargo and network access were unavailable. This remains a developer preview, not a validated production release.

## [0.13.0] - 2026-11-09

### Correctness and performance
- Fixed a compile-blocking full-bleed image crop (undefined UV variables); centralized the UV calculations with unit tests for both image orientations and equal ratios.
- Reduced temporary image memory: full-resolution JPEG composition can consume the captured RGB buffer instead of unconditionally cloning it. When no frame is configured, a second JPEG encode is skipped and the original encoded JPEG is copied to the framed path.
- Print quota edits are now staged in the admin interface and persisted only on Save. Disallow lowering an event quota below completed and pending prints. Administrator bonus unlock is persisted immediately; failed writes roll back in-memory state.
- If persisting a new or changed admin PIN fails, the in-memory credential is restored rather than diverging from disk.
- Added tests for UV crops, ownership-preserving image composition, zero-frame print JPEG equality, quota changes, and bonus persistence.

- Added `boothsoft-doctor`, a separate console-only binary for local JSON/text camera and printer enumeration; does not initialize a graphical window.

### Limitations
- Source changes were statically inspected, but cannot be compiled/tested in this environment (no Cargo/toolchain and no network access).
- Windows MSI is still produced by CI, not included here. Printer drivers, tethered DSLRs, device behavior and resource use require actual cross-platform hardware validation.

## 0.11.0 — Native device selection and installer authoring

- Enumerate native webcams and installed printer queues on demand from administration, without periodic UI polling.
- Replace manual camera index and printer name entry with device lists and a refresh control.
- Add WiX v5 MSI authoring and Windows CI packaging for the executable and all installed themes.
- Remove the unreleased back-office sync contract from the public OSS core; that functionality belongs in a private fork.
- Document the OS-specific driver requirements, installer workflow, and outstanding hardware validation.

# Changelog

## 0.10.0 — 2026-11-09 (developer preview)

### Added
- Single-flight, bounded Rust photo processor for off-thread JPEG encoding and frame composition.
- A platform-specific spooler worker: Windows GDI / macOS and Linux CUPS `lp`; printing disabled by default.
- Durable `Submitted` job state and administrator reconciliation controls; spooler acceptance never counts as paper output.
- Real printing and chosen printer name in admin settings, subject to a preconfigured event quota.
- `--demo`, `--windowed`, `--help` CLI flags for accessible, camera-free UI testing.
- New ADRs for background photo processing, spooler semantics and fail-closed settings.

### Changed
- Corrected Nokhwa native backend features and selected a conservative 720p/15 FPS stream when available.
- Capture and frame composition run outside the UI thread; review uses an already-rendered, reduced-size preview.
- Supported multi-photo capture flow advances until the configured number of shots is reached.
- Print jobs remain quota-reserved until verified, even after OS spooler acceptance.

### Security
- Corrupted settings no longer silently reset administrator credentials.
- Existing local session manifests are size-bounded and checked for unsafe IDs, filenames and symlinked directories.

### Known limitations
- No local Rust toolchain was available to compile this iteration. Windows/macOS/Linux hardware testing required.
- Canon-specific DSLR tethering, DNP job-complete telemetry and authenticated single-event network sync are pending.


## [0.12.0] - 2026-11-09

### Added
- Administrator-only native ZIP file picker and in-app installation of user themes.
- ZIP safety restrictions: path sanitization, symlink and special-file rejection, extraction budgets, duplicate detection, validated staging and no overwrite.
- User-local themes discovered alongside bundled system themes without requiring permission to modify the app installation.
- Documentation, ADR and Rust importer tests.

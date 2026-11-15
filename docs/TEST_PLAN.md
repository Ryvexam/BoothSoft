# Cross-platform validation plan

## Automated checks

```bash
cargo fmt --all --check
cargo test --all-targets
cargo clippy --all-targets -- -D warnings -D dead_code
cargo build --release
cargo run --release --bin boothsoft-bench -- 30
```

Required cases: preview-slot overwrite, camera stop/resume, filter equality/range, frame alpha composition, strict theme identifiers and asset paths, migrated settings, no-quota denial, print job idempotency after a partial crash.

## Manual acceptance matrix

| Scenario | Windows | macOS | Linux |
| --- | --- | --- | --- |
| Install and open `default` theme | Pending | Pending | Pending |
| Hot-switch to an installed theme, restart and persist | Pending | Pending | Pending |
| Live USB webcam, unplug/replug, permissions | Pending | Pending | Pending |
| Camera remains full-screen; no promotional UI | Pending | Pending | Pending |
| 7-second countdown and real JPEG export | Pending | Pending | Pending |
| Pause preview while reviewing; resume after retake | Pending | Pending | Pending |
| Stress: 100 photos and 30-minute idle | Pending | Pending | Pending |
| Reject symlink, traversal and oversized PNG theme | Pending | Pending | Pending |
| Confirmed print job prevents duplicate count | Pending | Pending | Pending |
| Offline boot with no network access | Pending | Pending | Pending |

`Pending` is deliberate: the source was not compiled or hardware-tested here. Printer and Canon EOS tethering tests must wait until native adapters exist.

## Release gate

Do **not** publish a production-ready release until all target builds pass CI, hardware capture has been validated, print quota enforcement is backed by an actual printer adapter, offline mode has been tested, and RSS/CPU/GPU measurements are recorded on at least one representative kiosk device.


## v0.10 operational acceptance additions

| Test | Pass condition |
| --- | --- |
| Demo `--demo --windowed` | Full-bleed synthetic camera, 7-second timer, framed photo, retake and finish work without peripherals |
| Four photos per session | After each non-final photo, next action returns to camera; only final action completes session |
| Off-thread full-resolution image work | GUI remains interactive and the worker has at most one queued capture |
| Print disabled by default | No spooler call when `print_enabled=false` |
| Print with no reservation quota | Request is refused before printer worker scheduling |
| Quota limit 100 | Exactly 100 reserved or confirmed; 101st denied without admin grant |
| 5% allowance unlocked | After admin unlock, 101–105 permitted; 106 denied |
| OS print accepted, paper jam follows | Status remains `Submitted`, capacity remains reserved until explicit manual reconciliation |
| Restart with pending jobs | Print jobs load without double-increment or automatic quota release |
| Invalid settings JSON | Startup fails instead of wiping PIN |
| Crafted `session.json` traversal | Unsafe file name/session folder rejected without opening attacker-selected path |
| Unavailable printer | Job stays outstanding; admin notified; does not falsely count a completed print |

**Hardware-specific tests remain mandatory** for DNP and Canon EOS 2000D (see `docs/RELEASE_CHECKLIST.md`).

## Native device enumeration and MSI

- Windows/macOS/Linux: enumerate two attached cameras and two installed printer queues, select each via Maintenance, save and relaunch.
- Refresh after unplugging a device; confirm missing device causes a safe error, not a crash.
- Verify the device scan is triggered only when opening Maintenance or by a refresh click; no per-frame device polling.
- Install the Windows MSI, verify shortcut, `themes/default`, in-app selection, and clean uninstall.
- Add `themes/custom` under the installed themes folder with suitable permissions, refresh and apply without restarting.


## ZIP theme import acceptance tests

- With a logged-in local admin, import a flat ZIP with `config.toml`, `assets/logo.png`, `assets/frame.png`; select Apply, relaunch and verify persistent selection.
- Import a ZIP wrapped in exactly one theme directory; verify the wrapper folder becomes the theme ID.
- Without a valid admin PIN, confirm there is no guest path to import themes.
- Reject `../`, Windows drive names, absolute paths, `\`, duplicate names differing only by case, ZIP symlinks, executable files, invalid TOML, invalid PNGs, excessive entries, excessive sizes and compressed ZIP bombs.
- Reject attempts to replace bundled theme IDs and an existing imported theme. Verify a failed import leaves no partially installed visible theme.
- Test with Windows installed under Program Files (non-admin user), macOS and Linux: imported themes persist in user data, and bundled themes remain readable.
- Monitor CPU/RAM during import, camera preview and idle administration; ensure import does not block the camera rendering or leave unbounded decoded image buffers.

## v0.14 regression checks

- `cargo check --all-targets` ensures the application, shared library, and three diagnostic binaries compile using the same source modules.
- CI rejects `allow(dead_code)` in `src/`; delete dead functions and stale dependencies instead of hiding them.
- Verify staged print job transitions roll back from Submitted or Cancelled if persistence fails, without releasing quota.
- Confirm strict 5% math: 19 included grants no whole bonus; 20 grants one, 21 grants one, and 100 grants five.
- Export two or four JPEGs into a contact sheet without decoding all originals simultaneously; inspect output dimensions and compare peak RSS.
- Verify still capture does not trigger a redundant immediate preview decode and the capture timeout remains functional without 200ms UI polling.

## v0.15 regression suite

- Test center-crop of 16x4096 and 4096x16 RGB sources into a 600x600 output; no giant full-image resize should be required.
- Confirm JPEG output and absence of `.jpg.tmp` files after success; force replacement failure and confirm the temporary file is removed after the encoder closes.
- Create two sessions in rapid succession; both IDs and manifest files must remain distinct.
- Force a retake manifest write/rename failure; both the in-memory shot and original JPEG must remain intact.
- Use GPU debugging tools to check that switching from live preview to photo Review/maintenance releases the unused preview texture without resource leaks after 100 repeated captures.
- Re-run the pre-existing theme import, quota fail-closed, camera and print acceptance tests. Do not treat static inspection as evidence of behavior on real hardware.

## v0.16 regression cases

1. Run all unit tests and verify temporary JSON replacement leaves exactly the current file with no sibling temporary files.
2. Supply a persistent JSON larger than its configured budget. Startup must fail closed; it must not reset PIN or printer quota.
3. Import JPEG and non-JPEG files through `read_print_jpeg`; non-JPEG content must be rejected even if the filename ends in `.jpg`.
4. Windows: validate odd-width DIB row padding, BGR channel order and no double full-resolution allocation in the adapter.
5. With a real Canon webcam/UVC mode and Windows GDI or CUPS printer, test live preview, filter changes, 7-second shutter, repeated retakes, print submission, crash recovery and administrative reconciliation.
6. Test MSI upgrade from the previous version; theme folders must remain correctly installed and user themes must survive upgrades.

**Required checks not performed in the source-only build environment:** `cargo fmt --all --check`, `cargo test --all-targets`, `cargo clippy --all-targets -- -D warnings -D dead_code`, cross-OS binary builds, MSI installation, driver printing and resource profiling.

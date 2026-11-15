# Release acceptance — BoothSoft

This is a **blocking** checklist for a stable commercial-quality release, not marketing copy. Source changes or successful `cargo check` alone are insufficient.

## Build and quality gates

- [ ] Windows `cargo fmt --check`, Clippy `-D warnings`, tests and release build pass.
- [ ] macOS `cargo fmt --check`, Clippy, tests and release build pass.
- [ ] Linux `cargo fmt --check`, Clippy, tests and release build pass.
- [ ] License, NOTICE/attribution, dependency licenses and security advisory process reviewed.
- [ ] Binary packages include both bundled themes; macOS camera usage string is signed and valid.
- [ ] Release tag signed, reproducible artifact hashes, no secrets or personal customer photos.

## Kiosk acceptance

- [ ] UI is camera-first, touch controls are reachable at 21.5 inches, no marketing copy.
- [ ] Seven-second countdown timing verified under CPU contention.
- [ ] Frames and logo switch during running event; invalid theme safely rejected.
- [ ] Photo capture with live camera, driver disconnect/reconnect and low-disk failures tested.
- [ ] Three-hour continuous operation plus at least 300 captures without memory growth.
- [ ] CPU/RAM/GPU numbers published for each supported test computer.
- [ ] Guest photos never leave the device unless the operator explicitly enables a future photo-sharing feature.

## Print acceptance

- [ ] Windows GDI print works with the chosen DNP printer and required paper size, orientation and driver settings.
- [ ] CUPS print works on both Linux and macOS with an installed supported printer/driver.
- [ ] Job pending/submitted/confirmed states survive application shutdown and power loss.
- [ ] Repeated clicks or repeated job IDs never consume more quota or print twice.
- [ ] A missing quota or failed disk persistence prevents printing.
- [ ] The +5% allowance requires an authenticated admin action and cannot be granted by a theme.
- [ ] OS kiosk user cannot open other programs and print outside BoothSoft (printer ACL/policies).
- [ ] Printer unplugged, empty paper, jam, Windows spooler restart and CUPS restart tested with safe recovery.
- [ ] The UI does not claim physical printing success from spooler acceptance alone.

## Camera & device support

- [ ] Windows webcam via Media Foundation, Linux webcam via V4L2 and macOS webcam via AVFoundation tested.
- [ ] Canon EOS 2000D native tethering adapter implemented and verified (not yet in source).
- [ ] DSLR battery, camera sleep, camera USB disconnect, reconnect and full-resolution still capture tested.

## Open-source release scope and data handling

- [ ] No private back-office code, production API credentials or event-network clients included in the OSS release.
- [ ] Windows MSI install/uninstall and theme discovery tested on a clean machine.
- [ ] Event/session data from previous bookings is not exposed to later guests; privacy retention and deletion documented.

### Optional private fork: event provisioning (not an OSS release gate)

- [ ] A fork-specific endpoint enforces **read/download only for one assigned event**.
- [ ] No global administrator API credentials embedded in rented kiosks.
- [ ] Signed/verified manifests, expiry, asset hashes and offline caching are validated in that fork.

Do **not** mark BoothSoft stable until all applicable critical acceptance gates pass.

## Distribution notes

- On macOS, run `bash scripts/package-macos.sh` after a release build to create an **unsigned** `.app` bundle with `NSCameraUsageDescription` and installed themes. Production distribution additionally requires signing, camera permission validation, and notarization.
- On Windows, distribute the executable with its adjacent `themes/` directory; sign the executable and test Win32 printing against the exact printer driver.
- On Linux, package required GTK/V4L2 dependencies and the adjacent `themes/` directory; use kiosk user permissions and CUPS ACLs.
- Generate and commit `Cargo.lock` from a verified Cargo resolution before publishing a stable application binary. `Cargo.lock` is not present in this offline source preview.

# ADR-0011: Install themes alongside the native binary

**Status:** Proposed; MSI build not yet verified on a Windows runner.

## Context

BoothSoft theme discovery reads `<executable directory>/themes/<id>/config.toml`. The installer must preserve that layout so newly installed valid theme folders can be selected dynamically.

## Decision

Use a WiX v5 MSI recipe with fixed product upgrade identity. Package the release executable and recursively harvest all `dist/themes/**`. Install under Program Files and provide a Start Menu shortcut. Save kiosk state under the OS-local app data directory, not Program Files. The installed theme folders are read-only for standard users; administrators install new themes.

## Consequences

Third-party themes must be deployed with appropriate OS permissions. The MSI is a Windows CI artifact, not a cross-platform binary; macOS and Linux use separate packaging. Test upgrades, uninstall behavior and new-theme refresh on Windows.

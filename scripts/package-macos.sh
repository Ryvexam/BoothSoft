#!/usr/bin/env bash
# SPDX-License-Identifier: CPAL-1.0
# Copyright (c) 2026 Dôm'Booth and the BoothSoft contributors. See LICENSE (Exhibits A and B) and NOTICE.
set -euo pipefail

# Run on macOS after `cargo build --release`. A signing identity is required
# for real distribution; this script intentionally does not sign or notarize.
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="$ROOT/dist/BoothSoft.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS/themes" "$APP/Contents/Resources"
cp "$ROOT/target/release/boothsoft" "$APP/Contents/MacOS/boothsoft"
cp -R "$ROOT/themes/." "$APP/Contents/MacOS/themes/"
cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>BoothSoft</string>
  <key>CFBundleDisplayName</key><string>BoothSoft</string>
  <key>CFBundleIdentifier</key><string>org.boothsoft.app</string>
  <key>CFBundleExecutable</key><string>boothsoft</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.16.0</string>
  <key>NSCameraUsageDescription</key><string>BoothSoft needs camera access to take photobooth pictures during events.</string>
</dict>
</plist>
PLIST
printf 'Unsigned application bundle: %s\n' "$APP"

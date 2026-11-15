# Cameras, printers and Windows MSI

## Device discovery

Open Maintenance with `Ctrl+Shift+D`, unlock it, and select **Refresh devices**. Discovery runs on a separate thread only when requested. The camera combo lists webcams reported by the OS camera backend. The printer combo lists OS-installed printer queues; selecting one does **not** install a driver or guarantee that a physical printer is online.

- Windows: Media Foundation (`nokhwa`); printer enumeration using `EnumPrintersW` level 4; submission through GDI.
- macOS: AVFoundation (`nokhwa`); CUPS `lpstat -p`; submission through CUPS `lp`.
- Linux: V4L2 (`nokhwa`); CUPS `lpstat -p`; submission through CUPS `lp`.

Compatibility depends on camera drivers/permissions and the printer driver. Canon EOS 2000D native USB tethering is not implemented. A Canon camera only appears if the OS exposes it as a compatible webcam/UVC device. DNP print completion cannot be inferred from successful spooler acceptance.

## Build on Windows

Requirements: stable Rust, Visual Studio C++ Build Tools, .NET SDK and WiX Toolset v5. Build the executable and arrange the payload:

```powershell
cargo test --all-targets
cargo build --release
New-Item -ItemType Directory -Force dist | Out-Null
Copy-Item target/release/boothsoft.exe dist/
Copy-Item -Recurse themes dist/themes
Copy-Item README.md, LICENSE dist/
dotnet tool install --global wix --version 5.0.2
$payload = (Resolve-Path dist).Path
wix build installers/windows/BoothSoft.wxs -arch x64 -d "Payload=$payload" -o dist/BoothSoft-0.16.0-x64.msi
```

The GitHub Actions workflow also builds the MSI after all tests/builds pass on Windows. At this stage the WiX recipe is authoring-only and has **not** produced a verified installable MSI in this environment.

## Installed themes

```text
C:\Program Files\BoothSoft\
  boothsoft.exe
  themes\
    default\config.toml
    default\assets\logo.png
    default\assets\frame.png
    default\assets\frame-portrait.png
```

Themes are dynamically discovered by folder ID on admin open and Refresh. Adding `themes/my_theme/config.toml` with validated assets makes it available without rebuilding the Rust executable. Under Program Files, installing themes normally requires administrative rights. The admin-only screen persists a chosen theme ID in the OS-local settings store.

## Hardware test checklist

1. Fresh Windows MSI install, launch via Start menu and open Maintenance.
2. List two cameras, select each, test capture, unplug/replug and refresh.
3. List installed printers, select one and enable printing under a nonzero event quota.
4. Print a framed JPEG; verify paper size, rotation, margins, and colors on the physical device.
5. Recover from printer-out-of-paper, disconnects, and crashes without giving away quota.
6. Add a valid custom theme folder as an administrator; refresh, apply and restart.
7. Verify MSI upgrades and uninstall do not erase user data stored under LocalAppData.
8. Measure baseline/preview/print CPU, RSS and GPU usage on each target OS.

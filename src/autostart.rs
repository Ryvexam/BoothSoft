// SPDX-License-Identifier: CPAL-1.0
// Copyright (c) 2026 Dôm'Booth and the BoothSoft contributors. See LICENSE (Exhibits A and B) and NOTICE.
//! Launch BoothSoft when the user session opens (kiosk boot: power on → live view).
//!
//! Per-user registration only, no administrator rights needed:
//! * Windows: `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`
//! * macOS: `~/Library/LaunchAgents/org.boothsoft.kiosk.plist`
//! * Linux: `~/.config/autostart/boothsoft.desktop`
//!
//! The registered command is the running executable with no arguments: theme,
//! full screen and the event schedule all come from the saved settings.
use std::io;
use std::path::PathBuf;

/// Program to start: `boothsoft-guard` next to the running executable when it
/// exists (relaunches BoothSoft after a crash), else the executable itself.
fn launch_target() -> io::Result<PathBuf> {
    let exe = std::env::current_exe()?;
    let guard = exe.with_file_name(format!("boothsoft-guard{}", std::env::consts::EXE_SUFFIX));
    Ok(if guard.is_file() { guard } else { exe })
}

const NAME: &str = "BoothSoft";

#[cfg(target_os = "windows")]
mod imp {
    use super::NAME;
    use std::{io, iter, ptr};
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
        HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_SZ,
    };

    const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

    fn wide(value: &str) -> Vec<u16> { value.encode_utf16().chain(iter::once(0)).collect() }

    fn open(access: u32) -> io::Result<HKEY> {
        let mut key: HKEY = ptr::null_mut();
        let path = wide(RUN);
        // SAFETY: valid NUL-terminated path, out-pointer to a local handle.
        let status = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, path.as_ptr(), 0, access, &mut key) };
        if status == ERROR_SUCCESS { Ok(key) } else { Err(io::Error::from_raw_os_error(status as i32)) }
    }

    pub fn is_enabled() -> bool {
        let Ok(key) = open(KEY_QUERY_VALUE) else { return false; };
        let name = wide(NAME);
        // SAFETY: size query only (null data buffer); the key is closed below.
        let status = unsafe { RegQueryValueExW(key, name.as_ptr(), ptr::null(), ptr::null_mut(), ptr::null_mut(), ptr::null_mut()) };
        unsafe { RegCloseKey(key) };
        status == ERROR_SUCCESS
    }

    pub fn set(enabled: bool) -> io::Result<()> {
        let key = open(KEY_SET_VALUE)?;
        let name = wide(NAME);
        let status = if enabled {
            let exe = super::launch_target()?;
            let command = wide(&format!("\"{}\"", exe.display()));
            // SAFETY: REG_SZ data is the NUL-terminated UTF-16 buffer, size in bytes.
            unsafe { RegSetValueExW(key, name.as_ptr(), 0, REG_SZ, command.as_ptr().cast(), (command.len() * 2) as u32) }
        } else {
            // SAFETY: deletes our own value; a missing value is fine.
            let status = unsafe { RegDeleteValueW(key, name.as_ptr()) };
            if status == windows_sys::Win32::Foundation::ERROR_FILE_NOT_FOUND { ERROR_SUCCESS } else { status }
        };
        unsafe { RegCloseKey(key) };
        if status == ERROR_SUCCESS { Ok(()) } else { Err(io::Error::from_raw_os_error(status as i32)) }
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::{entry_path, NAME};
    use std::{fs, io};

    pub fn is_enabled() -> bool { entry_path().is_some_and(|p| p.is_file()) }

    pub fn set(enabled: bool) -> io::Result<()> {
        let path = entry_path().ok_or_else(|| io::Error::other("No home directory"))?;
        if !enabled { return remove(&path); }
        let exe = super::launch_target()?;
        let dir = std::env::current_dir()?;
        let plist = format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>org.boothsoft.kiosk</string>
<key>ProgramArguments</key><array><string>{}</string></array>
<key>WorkingDirectory</key><string>{}</string>
<key>RunAtLoad</key><true/>
<key>ProcessType</key><string>Interactive</string>
</dict></plist>
<!-- {NAME} -->
"#, xml(&exe.display().to_string()), xml(&dir.display().to_string()));
        if let Some(parent) = path.parent() { fs::create_dir_all(parent)?; }
        fs::write(path, plist)
    }

    fn xml(value: &str) -> String { value.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;") }

    fn remove(path: &std::path::Path) -> io::Result<()> {
        match fs::remove_file(path) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        }
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
mod imp {
    use super::{entry_path, NAME};
    use std::{fs, io};

    pub fn is_enabled() -> bool { entry_path().is_some_and(|p| p.is_file()) }

    pub fn set(enabled: bool) -> io::Result<()> {
        let path = entry_path().ok_or_else(|| io::Error::other("No config directory"))?;
        if !enabled {
            return match fs::remove_file(&path) {
                Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
                _ => Ok(()),
            };
        }
        let exe = super::launch_target()?;
        let dir = std::env::current_dir()?;
        let entry = format!("[Desktop Entry]\nType=Application\nName={NAME}\nExec=\"{}\"\nPath={}\nX-GNOME-Autostart-enabled=true\n",
            exe.display(), dir.display());
        if let Some(parent) = path.parent() { fs::create_dir_all(parent)?; }
        fs::write(path, entry)
    }
}

#[cfg(target_os = "macos")]
fn entry_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join("Library/LaunchAgents/org.boothsoft.kiosk.plist"))
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn entry_path() -> Option<PathBuf> {
    dirs::config_dir().map(|config| config.join("autostart/boothsoft.desktop"))
}

/// Whether BoothSoft is registered to start with the user session.
pub fn is_enabled() -> bool { imp::is_enabled() }

/// Register (or remove) the start-up entry for the running executable.
pub fn set(enabled: bool) -> io::Result<()> { imp::set(enabled) }

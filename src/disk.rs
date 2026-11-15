// SPDX-License-Identifier: CPAL-1.0
// Copyright (c) 2026 Dôm'Booth and the BoothSoft contributors. See LICENSE (Exhibits A and B) and NOTICE.
//! Free disk space and folder sizes (ROADMAP B4 / Z5).
//!
//! The booth checks the free space at start-up and before every session (one
//! system call, a few microseconds). Under [`LOW`] maintenance shows a warning;
//! under [`FULL`] no new session starts and guests see a calm pause message, so
//! a photo is never half-written on a full disk.

use std::path::Path;

/// Maintenance warning below 5 GB free.
pub const LOW: u64 = 5 * 1024 * 1024 * 1024;
/// New sessions refused below 500 MB free.
pub const FULL: u64 = 500 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Space { Ok, Low, Full }

impl Space {
    pub fn of(free: u64) -> Self {
        if free < FULL { Space::Full } else if free < LOW { Space::Low } else { Space::Ok }
    }
}

/// Bytes available to this user on the disk holding `path`.
#[cfg(unix)]
pub fn free_bytes(path: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let c_path = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    // SAFETY: valid NUL-terminated path and a zeroed out-structure owned by this frame.
    let mut stats: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c_path.as_ptr(), &mut stats) } != 0 { return None; }
    #[allow(clippy::unnecessary_cast)] // field types differ between Linux and macOS
    Some(stats.f_bavail as u64 * stats.f_frsize as u64)
}

#[cfg(windows)]
pub fn free_bytes(path: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut available = 0u64;
    // SAFETY: NUL-terminated wide path; the other out-pointers may be null.
    let ok = unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut available, std::ptr::null_mut(), std::ptr::null_mut()) };
    (ok != 0).then_some(available)
}

#[cfg(not(any(unix, windows)))]
pub fn free_bytes(_path: &Path) -> Option<u64> { None }

/// Total size of the files under `path` (symbolic links not followed).
pub fn dir_size(path: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else { return 0 };
    entries.filter_map(Result::ok).map(|entry| match entry.file_type() {
        Ok(kind) if kind.is_dir() => dir_size(&entry.path()),
        Ok(kind) if kind.is_file() => entry.metadata().map_or(0, |m| m.len()),
        _ => 0,
    }).sum()
}

/// « 4,2 Go » / "4.2 GB".
pub fn human(bytes: u64) -> String {
    let gb = bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    if gb >= 1.0 { crate::tr!(fr: format!("{:.1} Go", gb).replace('.', ","), en: format!("{gb:.1} GB")) }
    else {
        let mb = (bytes as f64 / (1024.0 * 1024.0)).round();
        crate::tr!(fr: format!("{mb} Mo"), en: format!("{mb} MB"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thresholds() {
        assert_eq!(Space::of(100 << 30), Space::Ok);
        assert_eq!(Space::of(2 << 30), Space::Low);
        assert_eq!(Space::of(100 << 20), Space::Full);
    }

    #[test]
    fn measures_the_disk_and_folders() {
        let dir = std::env::temp_dir().join(format!("boothsoft-disk-test-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("a/b")).unwrap();
        std::fs::write(dir.join("a/one"), [0u8; 1000]).unwrap();
        std::fs::write(dir.join("a/b/two"), [0u8; 24]).unwrap();
        assert_eq!(dir_size(&dir), 1024);
        assert!(free_bytes(&dir).is_some_and(|free| free > 0));
        std::fs::remove_dir_all(dir).unwrap();
    }
}

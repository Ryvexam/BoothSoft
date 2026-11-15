// SPDX-License-Identifier: CPAL-1.0
// Copyright (c) 2026 Dôm'Booth and the BoothSoft contributors. See LICENSE (Exhibits A and B) and NOTICE.
//! Bounded file reads and crash-conscious same-directory replacements.
//!
//! The caller controls the trusted data directory. This module does not create
//! missing parents, follow caller-controlled paths, or silently recover from
//! corrupted persistent state. A successful write syncs the new file before
//! rename; power-loss durability of the directory entry is OS/filesystem dependent.
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

static WRITE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Never allocate from a file length controlled by external state.
/// The extra byte detects growth between metadata and actual reading.
pub fn read_limited(path: &Path, max_bytes: u64) -> io::Result<Vec<u8>> {
    let file = File::open(path)?;
    let meta = file.metadata()?;
    if !meta.is_file() || meta.len() > max_bytes {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Persistent file exceeds size limit"));
    }
    let mut data = Vec::new();
    file.take(max_bytes + 1).read_to_end(&mut data)?;
    if data.len() as u64 > max_bytes {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Persistent file grew beyond size limit"));
    }
    Ok(data)
}

/// Write a complete replacement into a fresh sibling file, sync it, and rename.
/// Never truncate an existing live JSON file or reuse a predictable `.tmp` path.
pub fn replace(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let name = path.file_name().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "Missing destination filename")
    })?;
    let sequence = WRITE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temp_name = format!(".{}.{}.{sequence}.tmp", name.to_string_lossy(), std::process::id());
    let temp = path.with_file_name(temp_name);
    let result = (|| {
        let mut file = OpenOptions::new().write(true).create_new(true).open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file); // Windows cannot rename a file while it is still open.
        fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn fixture() -> std::path::PathBuf {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("boothsoft-atomic-{}-{stamp}", std::process::id()));
        fs::create_dir(&root).unwrap();
        root
    }

    #[test]
    fn replacement_is_complete_and_has_no_stale_temp_file() {
        let root = fixture();
        let file = root.join("state.json");
        replace(&file, b"{\"value\":1}").unwrap();
        replace(&file, b"{\"value\":2}").unwrap();
        assert_eq!(read_limited(&file, 256).unwrap(), b"{\"value\":2}");
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_replacement_never_overwrites_original() {
        let root = fixture();
        let file = root.join("state.json");
        fs::write(&file, b"old").unwrap();
        let missing_parent = root.join("missing").join("state.json");
        assert!(replace(&missing_parent, b"new").is_err());
        assert_eq!(fs::read(file).unwrap(), b"old");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn bounded_reader_rejects_oversize_and_accepts_exact_limit() {
        let root = fixture();
        let file = root.join("state.json");
        fs::write(&file, b"123456").unwrap();
        assert_eq!(read_limited(&file, 6).unwrap(), b"123456");
        assert_eq!(read_limited(&file, 5).unwrap_err().kind(), io::ErrorKind::InvalidData);
        fs::remove_dir_all(root).unwrap();
    }
}

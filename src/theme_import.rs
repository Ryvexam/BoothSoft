// SPDX-License-Identifier: CPAL-1.0
// Copyright (c) 2026 Dôm'Booth and the BoothSoft contributors. See LICENSE (Exhibits A and B) and NOTICE.
//! Offline, bounded ZIP theme installation. This module is only called from
//! maintenance, on a dedicated worker; never from the camera/render loop.
//! No automatic overwrite and no extraction into the application install dir.
use crate::theme_config::{self, MAX_ASSET_BYTES, MAX_MANIFEST_BYTES};
use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use zip::ZipArchive;

const MAX_ARCHIVE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_EXTRACTED_BYTES: u64 = 32 * 1024 * 1024;
const MAX_ENTRIES: usize = 64;
const MAX_DOC_BYTES: u64 = 16 * 1024;
const MAX_ASSETS: usize = 32;

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

/// Reject unsupported names before we read or create any filesystem entry.
/// Do not normalize malicious paths into apparently safe ones.
fn split_name(name: &str) -> io::Result<Vec<&str>> {
    if name.len() > 200 || name.is_empty() || name.starts_with('/') || name.contains('\\')
        || name.contains(':') || name.contains('%') || name.contains('\0') {
        return Err(invalid("Unsafe ZIP entry path"));
    }
    let trimmed = name.strip_suffix('/').unwrap_or(name);
    let parts: Vec<_> = trimmed.split('/').collect();
    if parts.is_empty() || parts.len() > 6 || parts.iter().any(|s| !valid_component(s)) {
        return Err(invalid("Unsafe ZIP path component"));
    }
    Ok(parts)
}

fn valid_component(value: &str) -> bool {
    if value.is_empty() || value.len() > 80 || value == "." || value == ".."
        || value.starts_with('.') || value.ends_with('.') || value.ends_with(' ')
        || !value.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit()
            || matches!(b, b'_' | b'-' | b'.')) && value != "AGENTS.md" {
        return false;
    }
    let prefix = value.split('.').next().unwrap_or("");
    !matches!(prefix, "con" | "prn" | "aux" | "nul"
        | "com1" | "com2" | "com3" | "com4" | "com5" | "com6" | "com7" | "com8" | "com9"
        | "lpt1" | "lpt2" | "lpt3" | "lpt4" | "lpt5" | "lpt6" | "lpt7" | "lpt8" | "lpt9")
}

fn allowed_file(name: &str) -> bool {
    if name == "config.toml" || name == "AGENTS.md" { return true; }
    let Some(rest) = name.strip_prefix("assets/") else { return false; };
    rest.ends_with(".png") && !rest.is_empty() && rest.split('/').all(valid_component)
}

fn allowed_directory(name: &str) -> bool {
    name == "assets" || name.starts_with("assets/")
        && name["assets/".len()..].split('/').all(valid_component)
}

fn candidate_id(path: &Path, archive_names: &[String]) -> io::Result<(String, bool)> {
    let flat = archive_names.iter().any(|name| name == "config.toml");
    let id = if flat {
        path.file_stem().and_then(|x| x.to_str()).ok_or_else(|| invalid("Invalid ZIP filename"))?.to_owned()
    } else {
        let first = archive_names.iter()
            .find_map(|name| name.split_once('/').map(|(folder, _)| folder))
            .ok_or_else(|| invalid("Missing theme folder in ZIP"))?;
        if !archive_names.iter().all(|name| name == first || name.starts_with(&format!("{first}/"))) {
            return Err(invalid("ZIP must contain exactly one theme folder"));
        }
        first.to_owned()
    };
    if !theme_config::valid_theme_id(&id) {
        return Err(invalid("Theme ID must be a safe lowercase folder/ZIP name"));
    }
    Ok((id, !flat))
}

fn archive_entries(archive: &mut ZipArchive<File>, path: &Path) -> io::Result<(String, Vec<Option<PathBuf>>)> {
    if archive.is_empty() || archive.len() > MAX_ENTRIES {
        return Err(invalid("Too many or no ZIP entries"));
    }
    let mut raw_names = Vec::with_capacity(archive.len());
    for index in 0..archive.len() {
        let file = archive.by_index(index).map_err(io::Error::other)?;
        let name = file.name().trim_end_matches('/').to_owned();
        split_name(&name)?;
        if let Some(mode) = file.unix_mode() {
            let file_type = mode & 0o170000;
            if !matches!(file_type, 0 | 0o100000 | 0o040000)
                || (file_type == 0o040000) != file.is_dir() && file_type != 0 {
                return Err(invalid("ZIP contains a link or special file"));
            }
        }
        raw_names.push(name);
    }
    let (id, wrapped) = candidate_id(path, &raw_names)?;
    let mut paths = Vec::with_capacity(archive.len());
    let mut seen = HashSet::new();
    let mut total_bytes = 0u64;
    let mut asset_count = 0;
    let mut has_manifest = false;
    for (index, raw) in raw_names.iter().enumerate() {
        let file = archive.by_index(index).map_err(io::Error::other)?;
        let stripped = if wrapped {
            raw.strip_prefix(&format!("{id}/")).unwrap_or("")
        } else { raw.as_str() };
        if stripped.is_empty() && file.is_dir() {
            paths.push(None); // wrapper directory itself
            continue;
        }
        // A case-insensitive key rejects duplicate paths on NTFS/HFS as well.
        if !seen.insert(stripped.to_ascii_lowercase()) {
            return Err(invalid("Duplicate ZIP entry name"));
        }
        split_name(stripped)?;
        if file.is_dir() {
            if !allowed_directory(stripped) { return Err(invalid("Unexpected directory in theme ZIP")); }
            paths.push(Some(PathBuf::from(stripped)));
            continue;
        }
        if !allowed_file(stripped) { return Err(invalid("ZIP contains an unsupported file")); }
        let max_size = match stripped {
            "config.toml" => { has_manifest = true; MAX_MANIFEST_BYTES },
            "AGENTS.md" => MAX_DOC_BYTES,
            _ => { asset_count += 1; MAX_ASSET_BYTES },
        };
        if asset_count > MAX_ASSETS || file.size() == 0 || file.size() > max_size {
            return Err(invalid("ZIP entry exceeds its allowed size"));
        }
        total_bytes = total_bytes.checked_add(file.size()).ok_or_else(|| invalid("Invalid ZIP size"))?;
        if total_bytes > MAX_EXTRACTED_BYTES { return Err(invalid("ZIP exceeds extraction budget")); }
        paths.push(Some(PathBuf::from(stripped)));
    }
    if !has_manifest || asset_count == 0 { return Err(invalid("ZIP requires config.toml and PNG assets")); }
    Ok((id, paths))
}

/// Import one strict ZIP from disk. The resulting ID is not applied automatically;
/// administrator must explicitly select and apply the newly installed theme.
pub fn import_zip(installed_root: &Path, archive_path: &Path) -> io::Result<String> {
    let user_root = theme_config::user_themes_root()?;
    import_zip_into(installed_root, &user_root, archive_path)
}

fn import_zip_into(installed_root: &Path, user_root: &Path, archive_path: &Path) -> io::Result<String> {
    if !archive_path.extension().and_then(|x| x.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("zip")) {
        return Err(invalid("Select a .zip file"));
    }
    let file = File::open(archive_path)?;
    let size = file.metadata()?.len();
    if size == 0 || size > MAX_ARCHIVE_BYTES { return Err(invalid("ZIP exceeds 32 MiB")); }
    let mut archive = ZipArchive::new(file).map_err(io::Error::other)?;
    let (id, paths) = archive_entries(&mut archive, archive_path)?;

    let user_root = user_root.canonicalize()?;
    let destination = user_root.join(&id);
    if fs::symlink_metadata(installed_root.join(&id)).is_ok() || fs::symlink_metadata(&destination).is_ok() {
        return Err(invalid("Theme ID already installed; choose a different name"));
    }

    // Private staging directory; never extract directly to a visible theme.
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_err(io::Error::other)?.as_nanos();
    let staging = user_root.join(format!("staging-{:x}-{:x}", std::process::id(), stamp % 1_000_000_000_000_000));
    fs::create_dir(&staging)?;
    let result = (|| {
        let mut actual_total = 0u64;
        for (index, relative) in paths.iter().enumerate() {
            let Some(relative) = relative else { continue; };
            let mut entry = archive.by_index(index).map_err(io::Error::other)?;
            let target = staging.join(relative);
            if entry.is_dir() { fs::create_dir_all(&target)?; continue; }
            if let Some(parent) = target.parent() { fs::create_dir_all(parent)?; }
            let max_bytes = match relative.to_str() {
                Some("config.toml") => MAX_MANIFEST_BYTES,
                Some("AGENTS.md") => MAX_DOC_BYTES,
                _ => MAX_ASSET_BYTES,
            };
            let expected = entry.size();
            let mut output = OpenOptions::new().write(true).create_new(true).open(target)?;
            let written = io::copy(&mut entry.by_ref().take(max_bytes + 1), &mut output)?;
            output.flush()?;
            if written != expected || written > max_bytes {
                return Err(invalid("ZIP decompression size mismatch"));
            }
            actual_total = actual_total.checked_add(written).ok_or_else(|| invalid("ZIP extraction overflow"))?;
            if actual_total > MAX_EXTRACTED_BYTES { return Err(invalid("ZIP extraction budget exceeded")); }
        }
        theme_config::load(&user_root, staging.file_name().and_then(|name| name.to_str())
            .ok_or_else(|| invalid("Invalid temporary theme name"))?)?;
        // Never replace an existing installed theme, even after validation.
        if fs::symlink_metadata(installed_root.join(&id)).is_ok() || fs::symlink_metadata(&destination).is_ok() {
            return Err(invalid("Theme was installed concurrently"));
        }
        fs::rename(&staging, &destination)?;
        Ok(id)
    })();
    if result.is_err() { let _ = fs::remove_dir_all(&staging); }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use zip::{write::SimpleFileOptions, ZipWriter};

    fn fixture(name: &str) -> PathBuf {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).expect("time").as_nanos();
        let folder = std::env::temp_dir().join(format!("boothsoft-import-{}-{stamp}", std::process::id()));
        fs::create_dir(&folder).expect("create fixture");
        folder.join(name)
    }

    fn zip_with_entries(path: &Path, entries: &[(&str, &[u8])]) {
        let mut zip = ZipWriter::new(File::create(path).expect("zip output"));
        for (name, data) in entries {
            zip.start_file(*name, SimpleFileOptions::default()).expect("start entry");
            zip.write_all(data).expect("zip entry");
        }
        zip.finish().expect("finish zip");
    }

    #[test]
    fn rejects_traversal_and_unexpected_files() {
        for name in ["../config.toml", "/config.toml", "assets/../../evil.png", "assets/EVIL.png",
                     "assets/\\evil.png", "assets/con.png", "assets/a.exe", "assets/a.png:stream"] {
            assert!(split_name(name).is_err() || !allowed_file(name), "accepted: {name}");
        }
    }

    #[test]
    fn rejects_unsupported_file_before_extraction() {
        let file = fixture("sample.zip");
        zip_with_entries(&file, &[("config.toml", b"valid content"), ("assets/app.exe", b"binary")]);
        let mut archive = ZipArchive::new(File::open(&file).expect("open")).expect("read zip");
        assert!(archive_entries(&mut archive, &file).is_err());
        let _ = fs::remove_dir_all(file.parent().expect("parent"));
    }

    #[test]
    fn invalid_manifest_does_not_install_or_leave_staging() {
        let file = fixture("invalid-theme.zip");
        let logo = fs::read(Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("themes/default/assets/logo.png")).expect("logo");
        zip_with_entries(&file, &[("config.toml", b"invalid = [toml"), ("assets/logo.png", &logo)]);
        let installed = file.parent().expect("parent").join("installed");
        let user = file.parent().expect("parent").join("user");
        fs::create_dir(&installed).expect("installed");
        fs::create_dir(&user).expect("user");
        assert!(import_zip_into(&installed, &user, &file).is_err());
        assert_eq!(fs::read_dir(&user).expect("read user themes").count(), 0);
        let _ = fs::remove_dir_all(file.parent().expect("parent"));
    }

    #[test]
    fn blocks_theme_shadowing() {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("themes/default");
        let file = fixture("default.zip");
        let config = fs::read(source.join("config.toml")).expect("config");
        let logo = fs::read(source.join("assets/logo.png")).expect("logo");
        let frame = fs::read(source.join("assets/frame.png")).expect("frame");
        zip_with_entries(&file, &[("config.toml", &config), ("assets/logo.png", &logo), ("assets/frame.png", &frame)]);
        let user = file.parent().expect("parent").join("user");
        fs::create_dir(&user).expect("user");
        assert!(import_zip_into(source.parent().expect("parent"), &user, &file).is_err());
        let _ = fs::remove_dir_all(file.parent().expect("parent"));
    }

    #[test]
    fn validates_flat_and_wrapped_identifiers() {
        let flat = vec!["config.toml".to_owned(), "assets/frame.png".to_owned()];
        assert_eq!(candidate_id(Path::new("warm-studio.zip"), &flat).expect("flat"), ("warm-studio".to_owned(), false));
        let wrapped = vec!["warm-studio/config.toml".to_owned(), "warm-studio/assets/logo.png".to_owned()];
        assert_eq!(candidate_id(Path::new("ignored.zip"), &wrapped).expect("wrapped"), ("warm-studio".to_owned(), true));
    }

    #[test]
    fn valid_import_is_installed_and_reimport_refused() {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("themes/default");
        let sample = fixture("custom-theme.zip");
        let config = fs::read(source.join("config.toml")).expect("config");
        let logo = fs::read(source.join("assets/logo.png")).expect("logo");
        let frame = fs::read(source.join("assets/frame.png")).expect("frame");
        let background = fs::read(source.join("assets/background.png")).expect("background");
        let portrait = fs::read(source.join("assets/frame-portrait.png")).expect("portrait frame");
        zip_with_entries(&sample, &[("config.toml", &config), ("assets/logo.png", &logo),
            ("assets/frame.png", &frame), ("assets/frame-portrait.png", &portrait), ("assets/background.png", &background)]);
        let installed = sample.parent().expect("parent").join("installed");
        let user = sample.parent().expect("parent").join("user");
        fs::create_dir(&installed).expect("installed");
        fs::create_dir(&user).expect("user");
        assert_eq!(import_zip_into(&installed, &user, &sample).expect("installed"), "custom-theme");
        assert!(theme_config::load(&user, "custom-theme").is_ok());
        assert!(import_zip_into(&installed, &user, &sample).is_err());
        let _ = fs::remove_dir_all(sample.parent().expect("parent"));
    }
}

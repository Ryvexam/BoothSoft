// SPDX-License-Identifier: CPAL-1.0
// Copyright (c) 2026 Dôm'Booth and the BoothSoft contributors. See LICENSE (Exhibits A and B) and NOTICE.
//! A theme is a validated directory, never an arbitrary filesystem path.
//! No remote access, shell interpolation, environment expansion or dynamic code execution.
use serde::Deserialize;
use std::{fs, io, path::{Component, Path, PathBuf}};

pub const MAX_MANIFEST_BYTES: u64 = 32 * 1024;
pub const MAX_ASSET_BYTES: u64 = 16 * 1024 * 1024;
const MAX_ASSET_DIMENSION: u32 = 4096;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema_version: u32,
    pub name: String,
    pub colors: Colors,
    pub assets: Assets,
    pub ui: Labels,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Colors {
    pub background: String,
    pub foreground: String,
    pub primary: String,
    pub accent: String,
    pub surface: String,
    pub muted: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Assets {
    pub logo: String,
    pub frame: String,
    /// Optional portrait print frame (1200×1800). Without it, portrait prints
    /// have no frame (the photo covers the whole print).
    #[serde(default)]
    pub frame_portrait: Option<String>,
    /// Optional full-screen welcome artwork, pre-rendered so the UI draws one textured quad.
    #[serde(default)]
    pub background: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Labels {
    pub shutter_label: String,
    pub retake_label: String,
    pub finish_label: String,
    #[serde(default)]
    pub print_label: Option<String>,
    #[serde(default)]
    pub start_label: Option<String>,
    #[serde(default)]
    pub welcome_eyebrow: Option<String>,
    #[serde(default)]
    pub welcome_title: Option<String>,
    #[serde(default)]
    pub welcome_subtitle: Option<String>,
    #[serde(default)]
    pub thanks_title: Option<String>,
    #[serde(default)]
    pub thanks_subtitle: Option<String>,
}

impl Labels {
    pub fn print(&self) -> &str { self.print_label.as_deref().unwrap_or("Imprimer") }
    pub fn start(&self) -> &str { self.start_label.as_deref().unwrap_or("Commencer") }
    pub fn welcome_eyebrow(&self) -> &str { self.welcome_eyebrow.as_deref().unwrap_or("Photobooth") }
    pub fn welcome_title(&self) -> &str { self.welcome_title.as_deref().unwrap_or("Souriez,\nvous êtes\nmagnifiques") }
    pub fn welcome_subtitle(&self) -> &str { self.welcome_subtitle.as_deref().unwrap_or("Touchez l’écran pour prendre votre photo.") }
    pub fn thanks_title(&self) -> &str { self.thanks_title.as_deref().unwrap_or("Merci !") }
    pub fn thanks_subtitle(&self) -> &str { self.thanks_subtitle.as_deref().unwrap_or("À tout de suite pour la prochaine photo.") }
}

#[derive(Debug, Clone)]
pub struct LoadedTheme {
    pub id: String,
    pub config: Config,
    pub logo: PathBuf,
    pub frame: PathBuf,
    pub frame_portrait: Option<PathBuf>,
    pub background: Option<PathBuf>,
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

/// Theme IDs are also directory names. Reject Unicode confusables and OS-reserved names.
pub fn valid_theme_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 48
        && value.bytes().any(|byte| byte.is_ascii_alphanumeric())
        && !matches!(value, "." | ".." | "con" | "prn" | "aux" | "nul" | "com1" | "com2" | "com3" | "com4" | "com5" | "com6" | "com7" | "com8" | "com9" | "lpt1" | "lpt2" | "lpt3" | "lpt4" | "lpt5" | "lpt6" | "lpt7" | "lpt8" | "lpt9")
        && !value.starts_with('-')
        && value.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

fn validate_text(value: &str, max_len: usize) -> io::Result<()> {
    if value.is_empty() || value.chars().count() > max_len || value.chars().any(char::is_control) {
        return Err(invalid("Invalid theme display text"));
    }
    Ok(())
}

pub fn valid_color(value: &str) -> bool {
    value.len() == 7 && value.as_bytes()[0] == b'#'
        && value.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
}

/// Path is confined to the theme and cannot traverse symlinks or NTFS reparse points.
fn safe_asset_path(root: &Path, relative: &str) -> io::Result<PathBuf> {
    if relative.is_empty() || relative.len() > 150 || relative.chars().any(|character| matches!(character, '\\' | ':' | '%' | '\0')) {
        return Err(invalid("Invalid asset path"));
    }
    if relative.split('/').any(|part| part.is_empty() || part == "." || part == "..") {
        return Err(invalid("Asset paths must not contain empty or dot components"));
    }
    let path = Path::new(relative);
    if !path.components().all(|component| matches!(component, Component::Normal(_))) {
        return Err(invalid("Asset paths must be relative and cannot contain traversal"));
    }
    for component in path.components() {
        let Component::Normal(name) = component else { return Err(invalid("Invalid asset component")); };
        let Some(name) = name.to_str() else { return Err(invalid("Asset filename must be UTF-8")); };
        if name.is_empty() || name.starts_with('.') || name.ends_with('.') || name.ends_with(' ')
            || !name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'-')) {
            return Err(invalid("Asset filenames must be lowercase ASCII"));
        }
    }
    if path.extension().and_then(|ext| ext.to_str()) != Some("png") {
        return Err(invalid("Only PNG assets are supported"));
    }
    let mut current = root.to_path_buf();
    for component in path.components() {
        current.push(component);
        let metadata = fs::symlink_metadata(&current)?;
        if metadata.file_type().is_symlink() {
            return Err(invalid("Symlinks are forbidden in theme assets"));
        }
    }
    let canonical = current.canonicalize()?;
    if !canonical.starts_with(root) {
        return Err(invalid("Asset escapes theme root"));
    }
    let metadata = fs::metadata(&canonical)?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_ASSET_BYTES {
        return Err(invalid("Asset missing, empty or too large"));
    }
    // Decode header without allocating a full image; reject unexpected file types and giant images.
    let reader = image::ImageReader::open(&canonical)?.with_guessed_format().map_err(io::Error::other)?;
    if reader.format() != Some(image::ImageFormat::Png) {
        return Err(invalid("Asset is not a PNG"));
    }
    let (width, height) = reader.into_dimensions().map_err(io::Error::other)?;
    if width == 0 || height == 0 || width > MAX_ASSET_DIMENSION || height > MAX_ASSET_DIMENSION {
        return Err(invalid("PNG dimensions exceed allowed bounds"));
    }
    Ok(canonical)
}

pub fn load(themes_root: &Path, theme_id: &str) -> io::Result<LoadedTheme> {
    if !valid_theme_id(theme_id) { return Err(invalid("Invalid theme identifier")); }
    let root = themes_root.canonicalize()?;
    let directory = root.join(theme_id);
    if fs::symlink_metadata(&directory)?.file_type().is_symlink() {
        return Err(invalid("Theme directory cannot be a symlink"));
    }
    let directory = directory.canonicalize()?;
    if !directory.starts_with(&root) || !directory.is_dir() {
        return Err(invalid("Theme directory is not contained in themes root"));
    }
    let manifest = directory.join("config.toml");
    if fs::symlink_metadata(&manifest)?.file_type().is_symlink() {
        return Err(invalid("Theme manifest cannot be a symlink"));
    }
    if fs::metadata(&manifest)?.len() > MAX_MANIFEST_BYTES {
        return Err(invalid("Theme manifest too large"));
    }
    let data = fs::read_to_string(&manifest)?;
    let config: Config = toml::from_str(&data).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    if config.schema_version != 1 { return Err(invalid("Unsupported theme schema version")); }
    validate_text(&config.name, 80)?;
    for label in [&config.ui.shutter_label, &config.ui.retake_label, &config.ui.finish_label] {
        validate_text(label, 40)?;
    }
    for label in [&config.ui.print_label, &config.ui.start_label, &config.ui.welcome_eyebrow].into_iter().flatten() {
        validate_text(label, 40)?;
    }
    // Long guest-facing texts may contain explicit line breaks, but no other control characters.
    for text in [&config.ui.welcome_title, &config.ui.welcome_subtitle,
                 &config.ui.thanks_title, &config.ui.thanks_subtitle].into_iter().flatten() {
        validate_text(&text.replace('\n', " "), 140)?;
    }
    for color in [&config.colors.background, &config.colors.foreground, &config.colors.primary,
                  &config.colors.accent, &config.colors.surface, &config.colors.muted] {
        if !valid_color(color) { return Err(invalid("Colors must use #RRGGBB")); }
    }
    let logo = safe_asset_path(&directory, &config.assets.logo)?;
    let frame = safe_asset_path(&directory, &config.assets.frame)?;
    let background = config.assets.background.as_deref()
        .map(|relative| safe_asset_path(&directory, relative)).transpose()?;
    let frame_portrait = config.assets.frame_portrait.as_deref()
        .map(|relative| safe_asset_path(&directory, relative)).transpose()?;
    Ok(LoadedTheme { id: theme_id.into(), config, logo, frame, frame_portrait, background })
}


/// A bounded catalogue of independently validated, installed theme directories.
/// No image is decoded while scanning: this is safe to refresh from administration.
#[derive(Default)]
pub struct ThemeCatalog {
    pub installed: Vec<LoadedTheme>,
    pub rejected: usize,
}

pub fn discover(themes_root: &Path) -> io::Result<ThemeCatalog> {
    const MAX_ENTRIES: usize = 256;
    const MAX_THEMES: usize = 64;
    let mut result = ThemeCatalog::default();
    let root = themes_root.canonicalize()?;
    for item in fs::read_dir(&root)?.take(MAX_ENTRIES) {
        let Ok(item) = item else {
            result.rejected += 1;
            continue;
        };
        let Some(id) = item.file_name().to_str().map(str::to_owned) else {
            result.rejected += 1;
            continue;
        };
        // Ignore unrelated files such as the themes README.
        if !valid_theme_id(&id) { continue; }
        let Ok(metadata) = fs::symlink_metadata(item.path()) else {
            result.rejected += 1;
            continue;
        };
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            result.rejected += 1;
            continue;
        }
        if result.installed.len() >= MAX_THEMES {
            result.rejected += 1;
            continue;
        }
        match load(&root, &id) {
            Ok(theme) => result.installed.push(theme),
            Err(_) => result.rejected += 1,
        }
    }
    result.installed.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(result)
}

/// Installed layout: `<install>/themes/<id>`. During development, use the project cwd.
pub fn installed_themes_root() -> io::Result<PathBuf> {
    let executable = std::env::current_exe()?;
    let adjacent = executable.parent().ok_or_else(|| invalid("No executable directory"))?.join("themes");
    if adjacent.is_dir() { return Ok(adjacent); }
    let development = std::env::current_dir()?.join("themes");
    if development.is_dir() { return Ok(development); }
    Err(io::Error::new(io::ErrorKind::NotFound, "Missing installed themes directory"))
}

/// Third-party themes live in the current user's data directory, not the MSI
/// installation directory (which is normally read-only under Program Files).
pub fn user_themes_root() -> io::Result<PathBuf> {
    let data = dirs::data_local_dir().ok_or_else(|| invalid("No user data directory"))?;
    let root = data.join("BoothSoft").join("themes");
    fs::create_dir_all(&root)?;
    root.canonicalize()
}

/// Bundled themes take precedence: an imported theme cannot shadow `default`
/// or other themes shipped with an installed application.
pub fn load_any(installed_root: &Path, theme_id: &str) -> io::Result<LoadedTheme> {
    if !valid_theme_id(theme_id) {
        return Err(invalid("Invalid theme identifier"));
    }
    if fs::symlink_metadata(installed_root.join(theme_id)).is_ok() {
        return load(installed_root, theme_id);
    }
    load(&user_themes_root()?, theme_id)
}

/// Merge bundled and user-installed themes; duplicates and invalid entries are
/// hidden. Catalogue refresh occurs only on an admin action, never per frame.
pub fn discover_all(installed_root: &Path) -> io::Result<ThemeCatalog> {
    use std::collections::HashSet;
    let mut merged = discover(installed_root)?;
    let mut ids: HashSet<String> = merged.installed.iter().map(|t| t.id.clone()).collect();
    if let Ok(user_root) = user_themes_root() {
        let user = discover(&user_root)?;
        merged.rejected += user.rejected;
        for theme in user.installed {
            if ids.insert(theme.id.clone()) && merged.installed.len() < 64 {
                merged.installed.push(theme);
            } else {
                merged.rejected += 1;
            }
        }
    }
    merged.installed.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(merged)
}


#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identifiers_reject_traversal_and_confusables() {
        for value in ["../private", "foo/bar", "Foo", "café", "x\\y", "--", "", "a.b", "C:", "con", "com1"] {
            assert!(!valid_theme_id(value), "accepted: {value}");
        }
        assert!(valid_theme_id("my-brand"));
        assert!(valid_theme_id("my_theme-2"));
    }
    #[test]
    fn theme_identifiers_do_not_accept_punctuation_only() {
        for id in ["_", "__", "-", "--", "---", ".", "a/../b", "con"] {
            assert!(!valid_theme_id(id), "should reject {id}");
        }
    }
    #[cfg(unix)]
    #[test]
    fn symlinked_theme_directory_is_not_loaded() {
        use std::os::unix::fs::symlink;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).expect("clock")
            .as_nanos();
        let temp = std::env::temp_dir().join(format!("boothsoft-theme-test-{}-{stamp}", std::process::id()));
        fs::create_dir(&temp).expect("create isolated theme root");
        let original = Path::new(env!("CARGO_MANIFEST_DIR")).join("themes/default");
        symlink(&original, temp.join("external")).expect("create test symlink");
        assert!(load(&temp, "external").is_err());
        let catalogue = discover(&temp).expect("scan");
        assert!(catalogue.installed.is_empty());
        fs::remove_dir_all(temp).expect("remove fixture");
    }
    #[test]
    fn asset_paths_reject_traversal_and_urls() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("themes/default");
        let root = root.canonicalize().unwrap();
        for path in ["../private.png", "/etc/passwd.png", "assets/../../secret.png",
            "https://example.com/asset.png", "assets\\logo.png", "assets/%2e%2e.png",
            "assets/evil.svg", "assets/.hidden.png"] {
            assert!(safe_asset_path(&root, path).is_err(), "accepted {path}");
        }
        assert!(safe_asset_path(&root, "assets/logo.png").is_ok());
    }

    #[test]
    fn colors_are_strict() {
        assert!(valid_color("#1a2B3c"));
        for value in ["red", "#123", "#1234567", "#GG0000", " #123456"] {
            assert!(!valid_color(value));
        }
    }
    #[test]
    fn paths_reject_traversal_and_remote_references() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("themes/default");
        for path in ["../other/assets/logo.png", "/etc/passwd", "C:\\secret.png",
                     "https://example.com/logo.png", "assets/../../secret.png",
                     "assets/./logo.png", "assets/LOGO.png", "assets\\logo.png"] {
            assert!(safe_asset_path(&root, path).is_err(), "accepted: {path}");
        }
        assert!(safe_asset_path(&root, "assets/logo.png").is_ok());
    }
    #[test]
    fn catalogue_finds_only_valid_themes() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("themes");
        let catalogue = discover(&root).expect("scan must succeed");
        // A clean clone ships only `default` ("Party"); a developer may have private
        // themes next to it (ignored by Git). Every theme found must be valid.
        assert!(catalogue.installed.iter().any(|theme| theme.id == "default"));
        assert_eq!(catalogue.rejected, 0);
    }
    #[test]
    fn bundled_themes_validate() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("themes");
        let theme = load(&root, "default").expect("Bundled theme must validate");
        assert_eq!(theme.config.name, "Party");
    }
}

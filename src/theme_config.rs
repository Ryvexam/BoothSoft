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

/// Guest-facing texts of a theme, all optional (application defaults in the
/// current language). `[ui]` is written in `language` (absent: valid in every
/// language); `[ui.en]`, `[ui.fr]` override it per language.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Labels {
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub shutter_label: Option<String>,
    #[serde(default)]
    pub retake_label: Option<String>,
    #[serde(default)]
    pub finish_label: Option<String>,
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
    #[serde(default)]
    pub en: Option<LabelSet>,
    #[serde(default)]
    pub fr: Option<LabelSet>,
}

/// Texts of one language section (`[ui.en]`, `[ui.fr]`).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LabelSet {
    #[serde(default)]
    pub shutter_label: Option<String>,
    #[serde(default)]
    pub retake_label: Option<String>,
    #[serde(default)]
    pub finish_label: Option<String>,
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

macro_rules! label {
    ($(#[$doc:meta])* $name:ident, $field:ident, fr: $fr:expr, en: $en:expr) => {
        $(#[$doc])*
        pub fn $name(&self) -> &str {
            self.resolve(|set| set.$field.as_deref(), self.$field.as_deref(), $fr, $en)
        }
    };
}

impl Labels {
    /// `[ui.<lang>]`, then `[ui]` when it is written in the current language
    /// (or declares none), then the application's default text.
    fn resolve<'a>(&'a self, section: impl Fn(&'a LabelSet) -> Option<&'a str>, base: Option<&'a str>,
                   fr: &'static str, en: &'static str) -> &'a str {
        use crate::i18n::Lang;
        let lang = crate::i18n::lang();
        let set = match lang { Lang::En => self.en.as_ref(), Lang::Fr => self.fr.as_ref() };
        if let Some(text) = set.and_then(section) { return text; }
        let base_matches = self.language.as_deref().is_none_or(|l| Lang::from_locale(l) == Some(lang));
        if let Some(text) = base.filter(|_| base_matches) { return text; }
        match lang { Lang::Fr => fr, Lang::En => en }
    }

    label!(shutter, shutter_label, fr: "Prendre la photo", en: "Take the photo");
    label!(retake, retake_label, fr: "Reprendre", en: "Retake");
    label!(finish, finish_label, fr: "Terminer", en: "Done");
    label!(print, print_label, fr: "Imprimer", en: "Print");
    label!(start, start_label, fr: "Commencer", en: "Start");
    label!(welcome_eyebrow, welcome_eyebrow, fr: "Photobooth", en: "Photobooth");
    label!(welcome_title, welcome_title, fr: "Souriez,\nvous êtes\nmagnifiques", en: "Smile,\nyou look\namazing");
    label!(welcome_subtitle, welcome_subtitle, fr: "Touchez l’écran pour prendre votre photo.", en: "Touch the screen to take your photo.");
    label!(thanks_title, thanks_title, fr: "Merci !", en: "Thank you!");
    label!(thanks_subtitle, thanks_subtitle, fr: "À tout de suite pour la prochaine photo.", en: "See you soon for the next photo.");
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
    let ui = &config.ui;
    if let Some(language) = ui.language.as_deref() {
        if crate::i18n::Lang::from_locale(language).is_none() { return Err(invalid("Unknown [ui] language")); }
    }
    let sets = [Some(LabelSet {
        shutter_label: ui.shutter_label.clone(), retake_label: ui.retake_label.clone(), finish_label: ui.finish_label.clone(),
        print_label: ui.print_label.clone(), start_label: ui.start_label.clone(), welcome_eyebrow: ui.welcome_eyebrow.clone(),
        welcome_title: ui.welcome_title.clone(), welcome_subtitle: ui.welcome_subtitle.clone(),
        thanks_title: ui.thanks_title.clone(), thanks_subtitle: ui.thanks_subtitle.clone(),
    }), ui.en.clone(), ui.fr.clone()];
    for set in sets.iter().flatten() {
        for label in [&set.shutter_label, &set.retake_label, &set.finish_label, &set.print_label, &set.start_label, &set.welcome_eyebrow]
            .into_iter().flatten() {
            validate_text(label, 40)?;
        }
        // Long guest-facing texts may contain explicit line breaks, but no other control characters.
        for text in [&set.welcome_title, &set.welcome_subtitle, &set.thanks_title, &set.thanks_subtitle].into_iter().flatten() {
            validate_text(&text.replace('\n', " "), 140)?;
        }
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

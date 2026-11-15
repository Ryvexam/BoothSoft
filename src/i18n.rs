// SPDX-License-Identifier: CPAL-1.0
// Copyright (c) 2026 Dôm'Booth and the BoothSoft contributors. See LICENSE (Exhibits A and B) and NOTICE.
//! Interface languages (`docs/specs/LANGUAGES.md`).
//!
//! English is the default and the fallback; French is complete. With the
//! "automatic" setting the booth follows the computer's language when it is
//! one of ours, else English.
//!
//! Every visible text is written with [`tr!`](crate::tr), which takes **every**
//! language: forgetting a translation does not compile, and nothing is looked
//! up at run time (a `match` on one byte). Log lines stay in English.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang { En, Fr }

impl Lang {
    pub const ALL: [Lang; 2] = [Lang::En, Lang::Fr];
    /// The language's name in that language.
    pub fn native_name(self) -> &'static str { match self { Lang::En => "English", Lang::Fr => "Français" } }
    pub fn code(self) -> &'static str { match self { Lang::En => "en", Lang::Fr => "fr" } }

    /// "fr", "fr_FR.UTF-8", "fr-CA", "French_France.1252"… → `Fr`.
    pub fn from_locale(locale: &str) -> Option<Lang> {
        let lower = locale.trim().to_ascii_lowercase();
        let head: String = lower.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
        match head.as_str() {
            "fr" | "french" => Some(Lang::Fr),
            "en" | "english" => Some(Lang::En),
            _ => None,
        }
    }
}

/// Maintenance setting.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LanguageSetting {
    /// The computer's language when available, else English.
    #[default]
    Auto,
    En,
    Fr,
}

impl LanguageSetting {
    pub fn resolve(self) -> Lang {
        match self {
            LanguageSetting::En => Lang::En,
            LanguageSetting::Fr => Lang::Fr,
            LanguageSetting::Auto => system().unwrap_or(Lang::En),
        }
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

/// Current interface language (one atomic load).
#[inline]
pub fn lang() -> Lang { if CURRENT.load(Ordering::Relaxed) == 1 { Lang::Fr } else { Lang::En } }

pub fn set(lang: Lang) { CURRENT.store(match lang { Lang::En => 0, Lang::Fr => 1 }, Ordering::Relaxed); }

/// The computer's language, if it is one BoothSoft speaks (read once, then cached).
pub fn system() -> Option<Lang> {
    static SYSTEM: std::sync::OnceLock<Option<Lang>> = std::sync::OnceLock::new();
    *SYSTEM.get_or_init(|| {
        // Explicit override for tests and kiosks: BOOTHSOFT_LANG=fr.
        if let Some(lang) = std::env::var("BOOTHSOFT_LANG").ok().as_deref().and_then(Lang::from_locale) { return Some(lang); }
        system_locales().iter().find_map(|l| Lang::from_locale(l))
    })
}

#[cfg(windows)]
fn system_locales() -> Vec<String> {
    use windows_sys::Win32::Globalization::GetUserDefaultLocaleName;
    let mut buffer = [0u16; 85];
    // SAFETY: the buffer and its length (LOCALE_NAME_MAX_LENGTH) are valid for the call.
    let len = unsafe { GetUserDefaultLocaleName(buffer.as_mut_ptr(), buffer.len() as i32) };
    if len <= 1 { return Vec::new(); }
    vec![String::from_utf16_lossy(&buffer[..(len - 1) as usize])]
}

#[cfg(target_os = "macos")]
fn system_locales() -> Vec<String> {
    // Apps opened from the Finder get no LANG: ask the user defaults.
    let mut found = unix_env_locales();
    if let Ok(out) = std::process::Command::new("defaults").args(["read", "-g", "AppleLanguages"]).output() {
        // ( "fr-FR", "en-FR" ) → first entry first.
        let text = String::from_utf8_lossy(&out.stdout);
        found.extend(text.split(|c: char| c == '"' || c == ',' || c == '(' || c == ')' || c.is_whitespace())
            .filter(|s| !s.is_empty()).map(str::to_owned));
    }
    found
}

#[cfg(all(unix, not(target_os = "macos")))]
fn system_locales() -> Vec<String> { unix_env_locales() }

#[cfg(not(any(windows, unix)))]
fn system_locales() -> Vec<String> { Vec::new() }

#[cfg(unix)]
fn unix_env_locales() -> Vec<String> {
    // POSIX order: LC_ALL, then LC_MESSAGES, then LANG; LANGUAGE is a list ("fr:en").
    let mut found = Vec::new();
    if let Ok(list) = std::env::var("LANGUAGE") { found.extend(list.split(':').filter(|s| !s.is_empty()).map(str::to_owned)); }
    for key in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(value) = std::env::var(key) {
            if !value.is_empty() && value != "C" && value != "POSIX" && !value.starts_with("C.") { found.push(value); break; }
        }
    }
    found
}

/// One text in every language: `tr!(fr: "Imprimer", en: "Print")`. Each
/// argument is any expression (a literal, or a `format!`), evaluated only for
/// the current language.
#[macro_export]
macro_rules! tr {
    (fr: $fr:expr, en: $en:expr $(,)?) => {
        match $crate::i18n::lang() { $crate::i18n::Lang::Fr => $fr, $crate::i18n::Lang::En => $en }
    };
}

/// Short weekday and month names for the maintenance date picker.
pub fn weekday_short(day: usize) -> &'static str {
    const FR: [&str; 7] = ["lun.", "mar.", "mer.", "jeu.", "ven.", "sam.", "dim."];
    const EN: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    match lang() { Lang::Fr => FR[day % 7], Lang::En => EN[day % 7] }
}

pub fn month_short(month0: usize) -> &'static str {
    const FR: [&str; 12] = ["janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.", "déc."];
    const EN: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    match lang() { Lang::Fr => FR[month0 % 12], Lang::En => EN[month0 % 12] }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locales_are_recognised() {
        for (locale, lang) in [("fr_FR.UTF-8", Some(Lang::Fr)), ("fr-CA", Some(Lang::Fr)), ("French_France.1252", Some(Lang::Fr)),
                               ("en_US.UTF-8", Some(Lang::En)), ("en-GB", Some(Lang::En)), ("de_DE.UTF-8", None), ("C", None), ("", None)] {
            assert_eq!(Lang::from_locale(locale), lang, "{locale}");
        }
    }

    #[test]
    fn explicit_settings_win() {
        assert_eq!(LanguageSetting::Fr.resolve(), Lang::Fr);
        assert_eq!(LanguageSetting::En.resolve(), Lang::En);
    }

    #[test]
    fn tr_follows_the_current_language() {
        set(Lang::Fr);
        assert_eq!(tr!(fr: "Imprimer", en: "Print"), "Imprimer");
        set(Lang::En);
        let n = 3;
        assert_eq!(tr!(fr: format!("{n} tirages"), en: format!("{n} prints")), "3 prints");
    }
}

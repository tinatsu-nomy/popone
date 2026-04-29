//! Internationalization helpers built on top of `rust-i18n`.
//!
//! Translation files live under `popone/locales/{ja,en,zh}.yml` and are embedded
//! into the binary at compile time by the `i18n!` macro invoked in `lib.rs`.
//! This module exposes locale detection and runtime switching utilities so that
//! the rest of the crate does not depend directly on `rust_i18n`'s API surface.

/// Locales bundled into the binary. Update [`locales/`] alongside this list.
pub const SUPPORTED_LOCALES: &[&str] = &["ja", "en", "zh"];

/// Default locale used when OS detection fails or the detected locale is
/// not bundled. Matches the `fallback` argument of the `i18n!` macro.
pub const FALLBACK_LOCALE: &str = "en";

/// Normalize an arbitrary locale tag (e.g. `"ja-JP"`, `"zh_Hans_CN"`) to one
/// of [`SUPPORTED_LOCALES`], falling back to [`FALLBACK_LOCALE`].
pub fn normalize_locale(raw: &str) -> &'static str {
    let primary = raw
        .split(['-', '_'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    match primary.as_str() {
        "ja" => "ja",
        "zh" => "zh",
        "en" => "en",
        _ => FALLBACK_LOCALE,
    }
}

/// Detect the user's preferred locale from the operating system.
pub fn detect_default_locale() -> &'static str {
    let raw = sys_locale::get_locale().unwrap_or_default();
    normalize_locale(&raw)
}

/// Apply the active locale to the global `rust-i18n` state. Call once
/// at startup, before any user-visible string is generated.
///
/// Resolution order:
///   1. `POPONE_LOCALE` environment variable (e.g. `ja`, `en`, `zh`).
///      Useful for forcing a locale during development or scripted runs
///      regardless of the OS setting.
///   2. OS-detected locale via [`detect_default_locale`].
pub fn init_default_locale() {
    let resolved = std::env::var("POPONE_LOCALE")
        .ok()
        .map(|s| normalize_locale(&s))
        .unwrap_or_else(detect_default_locale);
    rust_i18n::set_locale(resolved);
}

/// Override the active locale at runtime. Unsupported tags fall back to
/// [`FALLBACK_LOCALE`] rather than leaving the previous locale untouched.
pub fn set_locale(requested: &str) {
    rust_i18n::set_locale(normalize_locale(requested));
}

/// Currently active locale tag (one of [`SUPPORTED_LOCALES`]).
pub fn current_locale() -> String {
    rust_i18n::locale().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_known_locales() {
        assert_eq!(normalize_locale("ja"), "ja");
        assert_eq!(normalize_locale("ja-JP"), "ja");
        assert_eq!(normalize_locale("en_US"), "en");
        assert_eq!(normalize_locale("zh-Hans-CN"), "zh");
        assert_eq!(normalize_locale("ZH"), "zh");
    }

    #[test]
    fn normalize_unknown_locale_falls_back() {
        assert_eq!(normalize_locale("fr-FR"), FALLBACK_LOCALE);
        assert_eq!(normalize_locale(""), FALLBACK_LOCALE);
    }
}

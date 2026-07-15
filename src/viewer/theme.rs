//! Application appearance theme (System / Light / Dark / Custom).
//!
//! Registers `Visuals` for both egui themes via `ctx.set_visuals_of` and
//! switches the follow mode with `egui::ThemePreference`. "System" tracks the
//! OS light/dark setting at runtime (winit `ThemeChanged`). "Custom" keeps the
//! legacy behavior: the hex colors from `[theme]` in popone.toml layered on
//! top of the dark base.
//!
//! Note: overlays drawn on the 3D viewport (FPS, camera info, drop overlay)
//! are rendered over the `bg_brightness`-controlled clear color, which is
//! independent from the egui theme, so they are intentionally not themed.

use eframe::egui;
use egui::Color32;

use super::app::persistence::{ThemeConfig, ThemeMode};

/// Dark theme panel background color (#1D1D1D).
pub const DARK_PANEL_BG: Color32 = Color32::from_rgb(0x1D, 0x1D, 0x1D);
/// Dark theme border color (#333333).
pub const DARK_BORDER: Color32 = Color32::from_rgb(0x33, 0x33, 0x33);
/// Dark theme accent color (#4A90D9).
pub const DARK_ACCENT: Color32 = Color32::from_rgb(0x4A, 0x90, 0xD9);
/// Dark theme text color (#D0D0D0).
pub const DARK_TEXT: Color32 = Color32::from_gray(0xD0);
/// Dark theme widget background color (#252525).
pub const DARK_WIDGET_BG: Color32 = Color32::from_rgb(0x25, 0x25, 0x25);

/// Resolved theme colors for one effective theme (dark or light).
///
/// `ViewerApp` refreshes its copy every frame from `ctx.theme()`, so manual
/// panel frames and hardcoded-ish text colors follow System switches too.
#[derive(Debug, Clone, Copy)]
pub struct ThemePalette {
    /// Effective darkness (Custom counts as dark).
    pub dark: bool,
    /// Panel / window background.
    pub panel_bg: Color32,
    /// Border / separator stroke color.
    pub border: Color32,
    /// Accent (hover / selection) color.
    pub accent: Color32,
    /// Normal text color.
    pub text: Color32,
    /// Widget (button at rest) background.
    pub widget_bg: Color32,
    /// Active (mouse-down) background.
    pub active: Color32,
    /// Open-widget (expanded ComboBox) / inactive-tab background.
    pub open_bg: Color32,
    /// Extreme background (TextEdit interior).
    pub extreme_bg: Color32,
}

impl ThemePalette {
    /// The default dark palette (v0 design; identical to the pre-v0.5.17 look).
    pub fn dark_default() -> Self {
        Self {
            dark: true,
            panel_bg: DARK_PANEL_BG,
            border: DARK_BORDER,
            accent: DARK_ACCENT,
            text: DARK_TEXT,
            widget_bg: DARK_WIDGET_BG,
            active: Color32::from_rgb(0x2A, 0x5A, 0x8A),
            open_bg: Color32::from_rgb(0x2A, 0x2A, 0x2A),
            extreme_bg: Color32::from_rgb(0x15, 0x15, 0x15),
        }
    }

    /// The light palette. Neutral grays mirror the dark palette inverted
    /// (0x1D -> 0xE2 etc.); the accent blue is shared with dark.
    pub fn light_default() -> Self {
        Self {
            dark: false,
            panel_bg: Color32::from_rgb(0xE2, 0xE2, 0xE2),
            border: Color32::from_rgb(0xBB, 0xBB, 0xBB),
            accent: DARK_ACCENT,
            text: Color32::from_gray(0x2F),
            widget_bg: Color32::from_rgb(0xDA, 0xDA, 0xDA),
            active: Color32::from_rgb(0xA9, 0xC7, 0xE8),
            open_bg: Color32::from_rgb(0xD5, 0xD5, 0xD5),
            extreme_bg: Color32::from_rgb(0xF4, 0xF4, 0xF4),
        }
    }

    /// The custom palette: dark base overridden by the `[theme]` hex colors.
    pub fn custom(cfg: &ThemeConfig) -> Self {
        let d = Self::dark_default();
        Self {
            dark: true,
            panel_bg: theme_color(&cfg.panel_bg, d.panel_bg),
            border: theme_color(&cfg.border, d.border),
            accent: theme_color(&cfg.accent, d.accent),
            text: theme_color(&cfg.text, d.text),
            widget_bg: theme_color(&cfg.widget_bg, d.widget_bg),
            active: theme_color(&cfg.active, d.active),
            open_bg: d.open_bg,
            extreme_bg: d.extreme_bg,
        }
    }

    /// Strong (emphasized) text: white on dark, near-black on light.
    pub fn strong_text(&self) -> Color32 {
        strong_text(self.dark)
    }

    /// Gray text tuned per theme: the dark theme uses the given level as-is,
    /// the light theme uses its inverse (0x60 -> 0x9F etc.).
    pub fn gray_text(&self, level: u8) -> Color32 {
        gray_text(self.dark, level)
    }

    /// Colored text (green/yellow/blue status labels etc.): the dark-theme
    /// color as-is on dark, darkened on light so it stays readable.
    pub fn accent_text(&self, dark_variant: Color32) -> Color32 {
        accent_text(self.dark, dark_variant)
    }

    /// Build the egui `Visuals` for this palette.
    pub fn visuals(&self) -> egui::Visuals {
        let mut visuals = if self.dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };

        // Panel / window background.
        visuals.panel_fill = self.panel_bg;
        visuals.window_fill = self.panel_bg;

        // Border.
        let border_stroke = egui::Stroke::new(1.0, self.border);
        visuals.window_stroke = border_stroke;

        // Common widget text color.
        let fg = egui::Stroke::new(1.0, self.text);

        // noninteractive (labels, separators, etc.).
        visuals.widgets.noninteractive.bg_stroke = border_stroke;
        visuals.widgets.noninteractive.fg_stroke = fg;

        // inactive (button at rest).
        visuals.widgets.inactive.bg_fill = self.widget_bg;
        visuals.widgets.inactive.weak_bg_fill = self.widget_bg;
        visuals.widgets.inactive.bg_stroke = border_stroke;
        visuals.widgets.inactive.fg_stroke = fg;

        // hovered: accent color (white text stays readable on the accent blue).
        visuals.widgets.hovered.bg_fill = self.accent;
        visuals.widgets.hovered.weak_bg_fill = self.accent;
        visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, self.accent);
        visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, Color32::WHITE);

        // active (while pressed).
        visuals.widgets.active.bg_fill = self.active;
        visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, self.active);
        visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, self.strong_text());

        // open (expanded ComboBox, etc.).
        visuals.widgets.open.bg_fill = self.open_bg;
        visuals.widgets.open.bg_stroke = border_stroke;
        visuals.widgets.open.fg_stroke = fg;

        // Selection / accent.
        visuals.selection.bg_fill = self.accent;
        visuals.selection.stroke = egui::Stroke::new(1.0, Color32::WHITE);

        // Extreme background (e.g., interior of `TextEdit`).
        visuals.extreme_bg_color = self.extreme_bg;

        visuals
    }
}

/// Convert a hex string in the config to `Color32` (with a default).
fn theme_color(opt: &Option<String>, default: Color32) -> Color32 {
    opt.as_ref()
        .and_then(|s| ThemeConfig::parse_hex(s))
        .map(|(r, g, b)| Color32::from_rgb(r, g, b))
        .unwrap_or(default)
}

/// Strong (emphasized) text color for the given darkness.
pub fn strong_text(dark: bool) -> Color32 {
    if dark {
        Color32::WHITE
    } else {
        Color32::from_gray(0x10)
    }
}

/// Theme-aware gray text (see `ThemePalette::gray_text`).
pub fn gray_text(dark: bool, level: u8) -> Color32 {
    if dark {
        Color32::from_gray(level)
    } else {
        Color32::from_gray(0xFF - level)
    }
}

/// Theme-aware colored text (see `ThemePalette::accent_text`).
pub fn accent_text(dark: bool, dark_variant: Color32) -> Color32 {
    if dark {
        dark_variant
    } else {
        // Darken so mid/high-brightness status colors stay readable on light.
        let f = 0.55_f32;
        Color32::from_rgb(
            (dark_variant.r() as f32 * f) as u8,
            (dark_variant.g() as f32 * f) as u8,
            (dark_variant.b() as f32 * f) as u8,
        )
    }
}

/// Format a `Color32` back into the config's 6-digit hex form.
pub fn color_hex(c: Color32) -> String {
    format!("{:02X}{:02X}{:02X}", c.r(), c.g(), c.b())
}

/// Fill unset custom colors with the dark defaults (so the color pickers in
/// the GUI show the effective colors when switching to Custom).
pub fn fill_custom_defaults(cfg: &mut ThemeConfig) {
    let d = ThemePalette::dark_default();
    let fill = |slot: &mut Option<String>, v: Color32| {
        if slot.is_none() {
            *slot = Some(color_hex(v));
        }
    };
    fill(&mut cfg.panel_bg, d.panel_bg);
    fill(&mut cfg.border, d.border);
    fill(&mut cfg.accent, d.accent);
    fill(&mut cfg.text, d.text);
    fill(&mut cfg.widget_bg, d.widget_bg);
    fill(&mut cfg.active, d.active);
}

/// Apply the theme in `cfg` to the egui context (both dark/light `Visuals`,
/// the follow-mode preference and shared spacing tweaks). Returns the palette
/// effective right now.
pub fn apply(ctx: &egui::Context, cfg: &ThemeConfig) -> ThemePalette {
    let mode = cfg.effective_mode();
    let dark_palette = match mode {
        ThemeMode::Custom => ThemePalette::custom(cfg),
        _ => ThemePalette::dark_default(),
    };
    ctx.set_visuals_of(egui::Theme::Dark, dark_palette.visuals());
    ctx.set_visuals_of(egui::Theme::Light, ThemePalette::light_default().visuals());

    // Make the scrollbar thinner (shared by both themes).
    ctx.all_styles_mut(|style| style.spacing.scroll.bar_width = 6.0);

    ctx.set_theme(match mode {
        ThemeMode::System => egui::ThemePreference::System,
        ThemeMode::Light => egui::ThemePreference::Light,
        ThemeMode::Dark | ThemeMode::Custom => egui::ThemePreference::Dark,
    });

    effective_palette(ctx, cfg)
}

/// The palette matching the theme currently in effect (`ctx.theme()`).
/// With `ThemeMode::System` this can change between frames when the OS
/// setting flips, so the caller refreshes it every frame.
pub fn effective_palette(ctx: &egui::Context, cfg: &ThemeConfig) -> ThemePalette {
    match ctx.theme() {
        egui::Theme::Light => ThemePalette::light_default(),
        egui::Theme::Dark => match cfg.effective_mode() {
            ThemeMode::Custom => ThemePalette::custom(cfg),
            _ => ThemePalette::dark_default(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fill_custom_defaults_matches_dark_palette() {
        // Switching to Custom with no colors set must reproduce the dark look
        // exactly (the pickers start from the effective colors).
        let mut cfg = ThemeConfig::default();
        fill_custom_defaults(&mut cfg);
        assert_eq!(cfg.panel_bg.as_deref(), Some("1D1D1D"));
        let custom = ThemePalette::custom(&cfg);
        let dark = ThemePalette::dark_default();
        assert_eq!(custom.panel_bg, dark.panel_bg);
        assert_eq!(custom.border, dark.border);
        assert_eq!(custom.accent, dark.accent);
        assert_eq!(custom.text, dark.text);
        assert_eq!(custom.widget_bg, dark.widget_bg);
        assert_eq!(custom.active, dark.active);
    }

    #[test]
    fn test_fill_custom_defaults_keeps_existing_colors() {
        let mut cfg = ThemeConfig {
            panel_bg: Some("202020".to_string()),
            ..Default::default()
        };
        fill_custom_defaults(&mut cfg);
        assert_eq!(cfg.panel_bg.as_deref(), Some("202020"));
        assert_eq!(cfg.border.as_deref(), Some("333333"));
    }

    #[test]
    fn test_color_hex_roundtrip() {
        let c = Color32::from_rgb(0x4A, 0x90, 0xD9);
        let hex = color_hex(c);
        assert_eq!(hex, "4A90D9");
        assert_eq!(ThemeConfig::parse_hex(&hex), Some((0x4A, 0x90, 0xD9)));
    }

    #[test]
    fn test_theme_aware_text_helpers() {
        // Dark passes values through; light inverts / darkens for readability.
        assert_eq!(gray_text(true, 0x60), Color32::from_gray(0x60));
        assert_eq!(gray_text(false, 0x60), Color32::from_gray(0x9F));
        assert_eq!(strong_text(true), Color32::WHITE);
        let red = Color32::from_rgb(0xFF, 0x60, 0x60);
        assert_eq!(accent_text(true, red), red);
        let light_red = accent_text(false, red);
        assert!(light_red.r() < red.r() && light_red.g() < red.g());
    }
}

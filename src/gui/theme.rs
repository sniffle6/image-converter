use std::{
    fs,
    path::{Path, PathBuf},
    str::FromStr,
};

use eframe::egui::{self, Color32, FontFamily, FontId, Stroke, TextStyle};
use image_converter::RgbColor;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    Light,
    #[default]
    Dark,
    Glass,
    Custom,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HexColor(pub [u8; 3]);

impl HexColor {
    pub fn egui(self) -> Color32 {
        Color32::from_rgb(self.0[0], self.0[1], self.0[2])
    }
}

impl std::fmt::Display for HexColor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{:02X}{:02X}{:02X}", self.0[0], self.0[1], self.0[2])
    }
}

impl FromStr for HexColor {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let parsed = value.parse::<RgbColor>()?;
        Ok(Self(parsed.components()))
    }
}

impl From<RgbColor> for HexColor {
    fn from(value: RgbColor) -> Self {
        Self(value.components())
    }
}

impl From<HexColor> for RgbColor {
    fn from(value: HexColor) -> Self {
        Self::new(value.0[0], value.0[1], value.0[2])
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ThemeTokens {
    pub canvas: HexColor,
    pub background: HexColor,
    pub panel: HexColor,
    pub control: HexColor,
    pub field: HexColor,
    pub row_alt: HexColor,
    pub border: HexColor,
    pub text: HexColor,
    pub muted: HexColor,
    pub accent: HexColor,
    pub on_accent: HexColor,
    pub danger: HexColor,
    pub title: HexColor,
    pub title_text: HexColor,
    pub title_muted: HexColor,
    pub title_rule: HexColor,
    pub title_control: HexColor,
}

const fn hex(r: u8, g: u8, b: u8) -> HexColor {
    HexColor([r, g, b])
}

impl ThemeTokens {
    pub fn light() -> Self {
        Self {
            canvas: hex(0xe9, 0xe7, 0xe2),
            background: hex(0xf6, 0xf5, 0xf2),
            panel: hex(0xfc, 0xfb, 0xf9),
            control: hex(0xec, 0xeb, 0xe6),
            field: hex(0xff, 0xff, 0xff),
            row_alt: hex(0xff, 0xff, 0xff),
            border: hex(0xdc, 0xd9, 0xd3),
            text: hex(0x26, 0x28, 0x2a),
            muted: hex(0x83, 0x81, 0x7b),
            accent: hex(0x0f, 0x9d, 0x84),
            on_accent: hex(0xff, 0xff, 0xff),
            danger: hex(0xb0, 0x4b, 0x2f),
            title: hex(0x26, 0x28, 0x2a),
            title_text: hex(0xf6, 0xf5, 0xf2),
            title_muted: hex(0xa3, 0xa1, 0x9b),
            title_rule: hex(0x43, 0x46, 0x4a),
            title_control: hex(0x3a, 0x3d, 0x41),
        }
    }

    pub fn dark() -> Self {
        Self {
            canvas: hex(0x10, 0x12, 0x15),
            background: hex(0x19, 0x1b, 0x1f),
            panel: hex(0x1f, 0x22, 0x26),
            control: hex(0x24, 0x27, 0x2c),
            field: hex(0x24, 0x27, 0x2c),
            row_alt: hex(0x24, 0x27, 0x2c),
            border: hex(0x33, 0x37, 0x3d),
            text: hex(0xe8, 0xea, 0xed),
            muted: hex(0x8b, 0x91, 0x9a),
            accent: hex(0x2f, 0xc9, 0xa2),
            on_accent: hex(0x10, 0x24, 0x1c),
            danger: hex(0xf0, 0x87, 0x6a),
            title: hex(0x11, 0x13, 0x16),
            title_text: hex(0xe8, 0xea, 0xed),
            title_muted: hex(0x8b, 0x91, 0x9a),
            title_rule: hex(0x33, 0x37, 0x3d),
            title_control: hex(0x24, 0x27, 0x2c),
        }
    }

    pub fn glass() -> Self {
        let mut tokens = Self::dark();
        tokens.canvas = hex(0x1b, 0x2a, 0x3a);
        tokens.background = hex(0x16, 0x1c, 0x22);
        tokens.panel = hex(0x25, 0x2d, 0x35);
        tokens.control = hex(0x31, 0x39, 0x42);
        tokens.field = hex(0x2b, 0x34, 0x3d);
        tokens.row_alt = hex(0x29, 0x32, 0x3a);
        tokens.border = hex(0x56, 0x60, 0x69);
        tokens.text = hex(0xf2, 0xf5, 0xf7);
        tokens.muted = hex(0x9e, 0xa8, 0xb0);
        tokens.danger = hex(0xff, 0x9d, 0x80);
        tokens.title = hex(0x0a, 0x0e, 0x12);
        tokens.title_text = hex(0xf2, 0xf5, 0xf7);
        tokens.title_muted = hex(0x9a, 0xa2, 0xaa);
        tokens.title_rule = hex(0x55, 0x60, 0x69);
        tokens.title_control = hex(0x31, 0x39, 0x42);
        tokens
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct Preferences {
    pub active_theme: ThemeMode,
    pub custom_tokens: ThemeTokens,
    pub jpeg_background: HexColor,
    pub glass_translucency: u8,
    pub glass_blur: u8,
    pub solid_when_inactive: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            active_theme: ThemeMode::Dark,
            custom_tokens: ThemeTokens::dark(),
            jpeg_background: HexColor::from(RgbColor::WHITE),
            glass_translucency: 72,
            glass_blur: 18,
            solid_when_inactive: false,
        }
    }
}

impl Preferences {
    pub fn tokens(&self) -> ThemeTokens {
        match self.active_theme {
            ThemeMode::Light => ThemeTokens::light(),
            ThemeMode::Dark => ThemeTokens::dark(),
            ThemeMode::Glass => ThemeTokens::glass(),
            ThemeMode::Custom => self.custom_tokens.clone(),
        }
    }

    pub fn load() -> Self {
        Self::load_from(&settings_path())
    }

    pub fn load_from(path: &Path) -> Self {
        fs::read_to_string(path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> Result<(), String> {
        self.save_to(&settings_path())
    }

    pub fn save_to(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        fs::write(path, json).map_err(|e| e.to_string())
    }

    pub fn apply(&self, context: &egui::Context) {
        let t = self.tokens();
        let theme = if self.active_theme == ThemeMode::Light {
            egui::Theme::Light
        } else {
            egui::Theme::Dark
        };
        context.set_theme(if theme == egui::Theme::Light {
            egui::ThemePreference::Light
        } else {
            egui::ThemePreference::Dark
        });
        let mut style = (*context.style_of(theme)).clone();
        let mut visuals = if theme == egui::Theme::Light {
            egui::Visuals::light()
        } else {
            egui::Visuals::dark()
        };
        visuals.panel_fill = t.background.egui();
        visuals.window_fill = t.panel.egui();
        visuals.faint_bg_color = t.row_alt.egui();
        visuals.extreme_bg_color = t.field.egui();
        visuals.override_text_color = Some(t.text.egui());
        visuals.selection.bg_fill = t.accent.egui();
        visuals.selection.stroke = Stroke::new(1.0, t.on_accent.egui());
        visuals.hyperlink_color = t.accent.egui();
        visuals.widgets.inactive.bg_fill = t.control.egui();
        visuals.widgets.inactive.weak_bg_fill = t.control.egui();
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, t.border.egui());
        visuals.widgets.hovered.bg_fill = mix(t.control.egui(), t.accent.egui(), 0.16);
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, t.accent.egui());
        visuals.widgets.active.bg_fill = t.accent.egui();
        visuals.widgets.active.fg_stroke = Stroke::new(1.0, t.on_accent.egui());
        visuals.widgets.open.bg_fill = t.control.egui();
        visuals.window_stroke = Stroke::new(1.0, t.border.egui());
        style.visuals = visuals;
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(10.0, 6.0);
        style.visuals.widgets.inactive.corner_radius = 6.0.into();
        style.visuals.widgets.hovered.corner_radius = 6.0.into();
        style.visuals.widgets.active.corner_radius = 6.0.into();
        style.text_styles.insert(
            TextStyle::Heading,
            FontId::new(18.0, FontFamily::Proportional),
        );
        style
            .text_styles
            .insert(TextStyle::Body, FontId::new(13.0, FontFamily::Proportional));
        style.text_styles.insert(
            TextStyle::Monospace,
            FontId::new(11.5, FontFamily::Monospace),
        );
        style.text_styles.insert(
            TextStyle::Button,
            FontId::new(12.5, FontFamily::Proportional),
        );
        context.set_style_of(theme, style);
    }
}

fn mix(a: Color32, b: Color32, amount: f32) -> Color32 {
    let blend = |x, y| (f32::from(x) * (1.0 - amount) + f32::from(y) * amount).round() as u8;
    Color32::from_rgb(
        blend(a.r(), b.r()),
        blend(a.g(), b.g()),
        blend(a.b(), b.b()),
    )
}

fn settings_path() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("Convertalot")
        .join("settings.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_exact_hex_colors() {
        assert_eq!(
            "#0f9D84".parse::<HexColor>().unwrap(),
            hex(0x0f, 0x9d, 0x84)
        );
        assert!("0f9d84".parse::<HexColor>().is_err());
    }

    #[test]
    fn persistence_round_trip_and_corruption_fallback() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("settings.json");
        let mut preferences = Preferences {
            active_theme: ThemeMode::Light,
            glass_translucency: 63,
            glass_blur: 27,
            ..Default::default()
        };
        preferences.custom_tokens.accent = hex(1, 2, 3);
        preferences.save_to(&path).unwrap();
        assert_eq!(Preferences::load_from(&path), preferences);
        fs::write(&path, r#"{"active_theme":"glass","glass_tint":8}"#).unwrap();
        let migrated = Preferences::load_from(&path);
        assert_eq!(migrated.active_theme, ThemeMode::Glass);
        assert_eq!(migrated.glass_translucency, 72);
        assert_eq!(migrated.glass_blur, 18);
        fs::write(&path, "{ definitely not json").unwrap();
        assert_eq!(Preferences::load_from(&path), Preferences::default());
    }
}

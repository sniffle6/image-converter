use std::{
    fs,
    path::{Path, PathBuf},
};

use image_converter::RgbColor;
use serde::{Deserialize, Serialize};

use crate::theme::{HexColor, ThemeTokens};

const SETTINGS_SCHEMA_VERSION: u32 = 3;
const RGB_SETTINGS_SCHEMA_VERSION: u32 = 2;
const DEFAULT_GLASS_BLUR: u8 = 18;
const DEFAULT_GLASS_TRANSLUCENCY: u8 = 72;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BuiltInTheme {
    Light,
    Dark,
    Glass,
}

impl BuiltInTheme {
    pub fn label(self) -> &'static str {
        match self {
            Self::Light => "Light",
            Self::Dark => "Dark",
            Self::Glass => "Glass",
        }
    }

    fn appearance(self) -> AppearanceSpec {
        match self {
            Self::Light => AppearanceSpec::solid(ThemeTokens::light()),
            Self::Dark => AppearanceSpec::solid(ThemeTokens::dark()),
            Self::Glass => AppearanceSpec {
                tokens: ThemeTokens::glass(),
                material: WindowMaterial::Glass {
                    blur: DEFAULT_GLASS_BLUR,
                    translucency: DEFAULT_GLASS_TRANSLUCENCY,
                    solid_when_inactive: false,
                },
            },
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct SavedThemeId(pub u64);

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum ThemeId {
    BuiltIn(BuiltInTheme),
    Saved(SavedThemeId),
}

impl Default for ThemeId {
    fn default() -> Self {
        Self::BuiltIn(BuiltInTheme::Dark)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WindowMaterial {
    Solid,
    Glass {
        blur: u8,
        translucency: u8,
        solid_when_inactive: bool,
    },
}

impl WindowMaterial {
    pub fn is_glass(&self) -> bool {
        matches!(self, Self::Glass { .. })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AppearanceSpec {
    pub tokens: ThemeTokens,
    pub material: WindowMaterial,
}

impl AppearanceSpec {
    fn solid(tokens: ThemeTokens) -> Self {
        Self {
            tokens,
            material: WindowMaterial::Solid,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SavedTheme {
    pub id: SavedThemeId,
    pub name: String,
    pub appearance: AppearanceSpec,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThemeChoice {
    pub id: ThemeId,
    pub label: String,
    pub built_in: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DirtyDecision {
    Save,
    Discard,
    Cancel,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DraftOrigin {
    Selected(ThemeId),
    New,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ThemeDraft {
    origin: DraftOrigin,
    name: String,
    appearance: AppearanceSpec,
    dirty: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThemeCatalog {
    selected: ThemeId,
    saved: Vec<SavedTheme>,
    next_saved_id: u64,
    draft: ThemeDraft,
    pending_selection: Option<ThemeId>,
}

impl Default for ThemeCatalog {
    fn default() -> Self {
        Self::from_persisted(ThemeId::default(), Vec::new(), 1)
    }
}

impl ThemeCatalog {
    fn from_persisted(selected: ThemeId, saved: Vec<SavedTheme>, next_saved_id: u64) -> Self {
        let mut catalog = Self {
            selected,
            saved,
            next_saved_id: next_saved_id.max(1),
            draft: ThemeDraft {
                origin: DraftOrigin::Selected(ThemeId::default()),
                name: String::new(),
                appearance: BuiltInTheme::Dark.appearance(),
                dirty: false,
            },
            pending_selection: None,
        };
        let highest_id = catalog
            .saved
            .iter()
            .map(|theme| theme.id.0)
            .max()
            .unwrap_or(0);
        catalog.next_saved_id = catalog.next_saved_id.max(highest_id.saturating_add(1));
        if !catalog.contains(catalog.selected) {
            catalog.selected = ThemeId::default();
        }
        catalog.load_selected_draft();
        catalog
    }

    pub fn choices(&self) -> Vec<ThemeChoice> {
        let mut choices = [BuiltInTheme::Light, BuiltInTheme::Dark, BuiltInTheme::Glass]
            .into_iter()
            .map(|built_in| ThemeChoice {
                id: ThemeId::BuiltIn(built_in),
                label: built_in.label().to_owned(),
                built_in: true,
            })
            .collect::<Vec<_>>();
        choices.extend(self.saved.iter().map(|theme| ThemeChoice {
            id: ThemeId::Saved(theme.id),
            label: theme.name.clone(),
            built_in: false,
        }));
        choices
    }

    pub fn selected(&self) -> ThemeId {
        self.selected
    }

    pub fn selected_label(&self) -> String {
        self.label(self.selected)
            .unwrap_or_else(|| "Dark".to_owned())
    }

    pub fn editing_saved(&self) -> Option<SavedThemeId> {
        match self.draft.origin {
            DraftOrigin::Selected(ThemeId::Saved(id)) => Some(id),
            _ => None,
        }
    }

    pub fn is_creating(&self) -> bool {
        self.draft.origin == DraftOrigin::New
    }

    pub fn is_dirty(&self) -> bool {
        self.draft.dirty
    }

    pub fn draft_name(&self) -> &str {
        &self.draft.name
    }

    pub fn resolved_appearance(&self) -> &AppearanceSpec {
        &self.draft.appearance
    }

    #[cfg(test)]
    pub fn saved_themes(&self) -> &[SavedTheme] {
        &self.saved
    }

    pub fn pending_selection(&self) -> Option<ThemeId> {
        self.pending_selection
    }

    pub fn begin_new_theme(&mut self) -> bool {
        if self.is_dirty() {
            return false;
        }
        self.draft = ThemeDraft {
            origin: DraftOrigin::New,
            name: String::new(),
            appearance: self.resolved_appearance().clone(),
            dirty: true,
        };
        true
    }

    pub fn set_draft_name(&mut self, name: String) {
        if self.draft.name != name {
            self.draft.name = name;
            self.draft.dirty = true;
        }
    }

    pub fn set_draft_tokens(&mut self, tokens: ThemeTokens) {
        if self.draft.appearance.tokens != tokens {
            self.draft.appearance.tokens = tokens;
            self.draft.dirty = true;
        }
    }

    pub fn set_solid(&mut self) {
        if self.draft.appearance.material != WindowMaterial::Solid {
            self.draft.appearance.material = WindowMaterial::Solid;
            self.draft.dirty = true;
        }
    }

    pub fn set_glass(&mut self) {
        if !self.draft.appearance.material.is_glass() {
            self.draft.appearance.material = WindowMaterial::Glass {
                blur: DEFAULT_GLASS_BLUR,
                translucency: DEFAULT_GLASS_TRANSLUCENCY,
                solid_when_inactive: false,
            };
            self.draft.dirty = true;
        }
    }

    pub fn set_glass_values(&mut self, blur: u8, translucency: u8, solid_when_inactive: bool) {
        let material = WindowMaterial::Glass {
            blur: blur.min(64),
            translucency: translucency.min(90),
            solid_when_inactive,
        };
        if self.draft.appearance.material != material {
            self.draft.appearance.material = material;
            self.draft.dirty = true;
        }
    }

    pub fn request_selection(&mut self, id: ThemeId) -> Result<bool, String> {
        if !self.contains(id) {
            return Err("That theme no longer exists".to_owned());
        }
        if self.draft.dirty {
            if self.draft.origin == DraftOrigin::Selected(id) {
                return Ok(true);
            }
            self.pending_selection = Some(id);
            return Ok(false);
        }
        self.select_now(id);
        Ok(true)
    }

    pub fn resolve_pending(&mut self, decision: DirtyDecision) -> Result<bool, String> {
        let Some(target) = self.pending_selection else {
            return Ok(false);
        };
        match decision {
            DirtyDecision::Cancel => {
                self.pending_selection = None;
                Ok(false)
            }
            DirtyDecision::Discard => {
                self.pending_selection = None;
                self.select_now(target);
                Ok(true)
            }
            DirtyDecision::Save => {
                self.save_draft()?;
                self.pending_selection = None;
                self.select_now(target);
                Ok(true)
            }
        }
    }

    pub fn save_draft(&mut self) -> Result<ThemeId, String> {
        match self.draft.origin {
            DraftOrigin::Selected(ThemeId::Saved(id)) => {
                let name = self.valid_name(&self.draft.name, Some(id))?;
                let theme = self
                    .saved
                    .iter_mut()
                    .find(|theme| theme.id == id)
                    .ok_or_else(|| "That theme no longer exists".to_owned())?;
                theme.name = name.clone();
                theme.appearance = self.draft.appearance.clone();
                self.draft.name = name;
                self.draft.dirty = false;
                Ok(ThemeId::Saved(id))
            }
            DraftOrigin::Selected(ThemeId::BuiltIn(_)) | DraftOrigin::New => {
                let name = self.valid_name(&self.draft.name, None)?;
                let id = SavedThemeId(self.next_saved_id);
                self.next_saved_id = self.next_saved_id.saturating_add(1).max(1);
                self.saved.push(SavedTheme {
                    id,
                    name,
                    appearance: self.draft.appearance.clone(),
                });
                self.selected = ThemeId::Saved(id);
                self.load_selected_draft();
                Ok(self.selected)
            }
        }
    }

    pub fn discard_draft(&mut self) {
        self.pending_selection = None;
        self.load_selected_draft();
    }

    pub fn rename_selected(&mut self, name: &str) -> Result<SavedThemeId, String> {
        if self.is_dirty() {
            return Err("Save or discard changes before renaming".to_owned());
        }
        let Some(id) = self.editing_saved() else {
            return Err("Built-in themes cannot be renamed".to_owned());
        };
        let name = self.valid_name(name, Some(id))?;
        let theme = self.saved.iter_mut().find(|theme| theme.id == id).unwrap();
        theme.name = name.clone();
        self.draft.name = name;
        self.draft.dirty = false;
        Ok(id)
    }

    pub fn delete_selected(&mut self) -> Result<SavedThemeId, String> {
        if self.is_dirty() {
            return Err("Save or discard changes before deleting".to_owned());
        }
        let ThemeId::Saved(id) = self.selected else {
            return Err("Built-in themes cannot be deleted".to_owned());
        };
        self.saved.retain(|theme| theme.id != id);
        self.selected = ThemeId::default();
        self.pending_selection = None;
        self.load_selected_draft();
        Ok(id)
    }

    fn valid_name(&self, name: &str, current: Option<SavedThemeId>) -> Result<String, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("Enter a theme name".to_owned());
        }
        if name.chars().count() > 64 || name.chars().any(char::is_control) {
            return Err("Theme names must be 1–64 printable characters".to_owned());
        }
        if self
            .saved
            .iter()
            .any(|theme| Some(theme.id) != current && theme.name.eq_ignore_ascii_case(name))
        {
            return Err("A theme with that name already exists".to_owned());
        }
        Ok(name.to_owned())
    }

    fn contains(&self, id: ThemeId) -> bool {
        match id {
            ThemeId::BuiltIn(_) => true,
            ThemeId::Saved(id) => self.saved.iter().any(|theme| theme.id == id),
        }
    }

    fn label(&self, id: ThemeId) -> Option<String> {
        match id {
            ThemeId::BuiltIn(theme) => Some(theme.label().to_owned()),
            ThemeId::Saved(id) => self
                .saved
                .iter()
                .find(|theme| theme.id == id)
                .map(|theme| theme.name.clone()),
        }
    }

    fn select_now(&mut self, id: ThemeId) {
        self.selected = id;
        self.pending_selection = None;
        self.load_selected_draft();
    }

    fn load_selected_draft(&mut self) {
        let (name, appearance) = match self.selected {
            ThemeId::BuiltIn(theme) => (String::new(), theme.appearance()),
            ThemeId::Saved(id) => self
                .saved
                .iter()
                .find(|theme| theme.id == id)
                .map(|theme| (theme.name.clone(), theme.appearance.clone()))
                .unwrap_or_else(|| (String::new(), BuiltInTheme::Dark.appearance())),
        };
        self.draft = ThemeDraft {
            origin: DraftOrigin::Selected(self.selected),
            name,
            appearance,
            dirty: false,
        };
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Preferences {
    pub themes: ThemeCatalog,
    pub jpeg_background: HexColor,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            themes: ThemeCatalog::default(),
            jpeg_background: HexColor::from(RgbColor::WHITE),
        }
    }
}

#[derive(Deserialize, Serialize)]
struct PersistedSettings {
    schema_version: u32,
    selected_theme: ThemeId,
    saved_themes: Vec<SavedTheme>,
    next_saved_theme_id: u64,
    jpeg_background: HexColor,
}

#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
enum LegacyThemeMode {
    Light,
    #[default]
    Dark,
    Glass,
    Custom,
    CustomGlass,
}

#[derive(Deserialize)]
#[serde(default)]
struct LegacySettings {
    active_theme: LegacyThemeMode,
    custom_tokens: ThemeTokens,
    custom_theme_saved: bool,
    custom_theme_uses_glass: bool,
    jpeg_background: HexColor,
    glass_translucency: u8,
    glass_blur: u8,
    solid_when_inactive: bool,
}

impl Default for LegacySettings {
    fn default() -> Self {
        Self {
            active_theme: LegacyThemeMode::Dark,
            custom_tokens: ThemeTokens::dark(),
            custom_theme_saved: false,
            custom_theme_uses_glass: false,
            jpeg_background: HexColor::from(RgbColor::WHITE),
            glass_translucency: DEFAULT_GLASS_TRANSLUCENCY,
            glass_blur: DEFAULT_GLASS_BLUR,
            solid_when_inactive: false,
        }
    }
}

impl Preferences {
    pub fn load() -> Self {
        Self::load_from(&settings_path())
    }

    pub fn load_from(path: &Path) -> Self {
        fs::read_to_string(path)
            .ok()
            .and_then(|raw| Self::from_json(&raw))
            .unwrap_or_default()
    }

    fn from_json(raw: &str) -> Option<Self> {
        let value: serde_json::Value = serde_json::from_str(raw).ok()?;
        if value.get("schema_version").is_some() {
            let persisted: PersistedSettings = serde_json::from_value(value).ok()?;
            if !matches!(
                persisted.schema_version,
                RGB_SETTINGS_SCHEMA_VERSION | SETTINGS_SCHEMA_VERSION
            ) {
                return None;
            }
            return Some(Self {
                themes: ThemeCatalog::from_persisted(
                    persisted.selected_theme,
                    persisted.saved_themes,
                    persisted.next_saved_theme_id,
                ),
                jpeg_background: persisted.jpeg_background,
            });
        }
        let legacy: LegacySettings = serde_json::from_value(value).ok()?;
        Some(Self::migrate_legacy(legacy))
    }

    fn migrate_legacy(legacy: LegacySettings) -> Self {
        let selected_builtin = match legacy.active_theme {
            LegacyThemeMode::Light => BuiltInTheme::Light,
            LegacyThemeMode::Glass => BuiltInTheme::Glass,
            _ => BuiltInTheme::Dark,
        };
        let active_custom = matches!(
            legacy.active_theme,
            LegacyThemeMode::Custom | LegacyThemeMode::CustomGlass
        );
        let should_migrate_custom = active_custom || legacy.custom_theme_saved;
        let custom_is_glass = match legacy.active_theme {
            LegacyThemeMode::CustomGlass => true,
            LegacyThemeMode::Custom => false,
            _ => legacy.custom_theme_uses_glass,
        };
        let saved = should_migrate_custom
            .then(|| SavedTheme {
                id: SavedThemeId(1),
                name: if custom_is_glass {
                    "My glass".to_owned()
                } else {
                    "My theme".to_owned()
                },
                appearance: AppearanceSpec {
                    tokens: legacy.custom_tokens,
                    material: if custom_is_glass {
                        WindowMaterial::Glass {
                            blur: legacy.glass_blur.min(64),
                            translucency: legacy.glass_translucency.min(90),
                            solid_when_inactive: legacy.solid_when_inactive,
                        }
                    } else {
                        WindowMaterial::Solid
                    },
                },
            })
            .into_iter()
            .collect::<Vec<_>>();
        let selected = if active_custom {
            ThemeId::Saved(SavedThemeId(1))
        } else {
            ThemeId::BuiltIn(selected_builtin)
        };
        Self {
            themes: ThemeCatalog::from_persisted(selected, saved, 2),
            jpeg_background: legacy.jpeg_background,
        }
    }

    pub fn save(&self) -> Result<(), String> {
        self.save_to(&settings_path())
    }

    pub fn save_to(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let persisted = PersistedSettings {
            schema_version: SETTINGS_SCHEMA_VERSION,
            selected_theme: self.themes.selected,
            saved_themes: self.themes.saved.clone(),
            next_saved_theme_id: self.themes.next_saved_id,
            jpeg_background: self.jpeg_background,
        };
        let json = serde_json::to_vec_pretty(&persisted).map_err(|error| error.to_string())?;
        fs::write(path, json).map_err(|error| error.to_string())
    }
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

    fn save_named(catalog: &mut ThemeCatalog, name: &str) -> ThemeId {
        assert!(catalog.begin_new_theme());
        catalog.set_draft_name(name.to_owned());
        catalog.save_draft().unwrap()
    }

    #[test]
    fn creates_multiple_themes_with_unique_stable_ids_and_rename_preserves_id() {
        let mut catalog = ThemeCatalog::default();
        let first = save_named(&mut catalog, "First");
        let second = save_named(&mut catalog, "Second");
        assert_ne!(first, second);
        let ThemeId::Saved(second_id) = second else {
            panic!()
        };
        assert_eq!(catalog.rename_selected("Renamed").unwrap(), second_id);
        assert_eq!(catalog.selected(), ThemeId::Saved(second_id));
        assert_eq!(catalog.selected_label(), "Renamed");
    }

    #[test]
    fn rejects_blank_invalid_and_duplicate_names() {
        let mut catalog = ThemeCatalog::default();
        assert!(catalog.begin_new_theme());
        assert!(catalog.save_draft().is_err());
        catalog.set_draft_name("bad\nname".to_owned());
        assert!(catalog.save_draft().is_err());
        catalog.set_draft_name("Good".to_owned());
        catalog.save_draft().unwrap();
        assert!(catalog.begin_new_theme());
        catalog.set_draft_name(" good ".to_owned());
        assert!(catalog.save_draft().is_err());
    }

    #[test]
    fn selects_builtins_and_saved_and_restores_each_material() {
        let mut catalog = ThemeCatalog::default();
        catalog
            .request_selection(ThemeId::BuiltIn(BuiltInTheme::Glass))
            .unwrap();
        let glass = save_named(&mut catalog, "Glass custom");
        catalog.set_glass_values(31, 44, true);
        catalog.save_draft().unwrap();
        catalog
            .request_selection(ThemeId::BuiltIn(BuiltInTheme::Light))
            .unwrap();
        let solid = save_named(&mut catalog, "Solid custom");
        catalog.set_solid();
        catalog.save_draft().unwrap();
        catalog.request_selection(glass).unwrap();
        assert_eq!(
            catalog.resolved_appearance().material,
            WindowMaterial::Glass {
                blur: 31,
                translucency: 44,
                solid_when_inactive: true
            }
        );
        catalog.request_selection(solid).unwrap();
        assert_eq!(
            catalog.resolved_appearance().material,
            WindowMaterial::Solid
        );
    }

    #[test]
    fn updates_and_deletes_active_theme_with_dark_fallback() {
        let mut catalog = ThemeCatalog::default();
        let saved = save_named(&mut catalog, "Editable");
        let mut tokens = catalog.resolved_appearance().tokens.clone();
        tokens.accent = HexColor([1, 2, 3, 96]);
        catalog.set_draft_tokens(tokens.clone());
        assert_eq!(catalog.save_draft().unwrap(), saved);
        assert_eq!(catalog.resolved_appearance().tokens, tokens);
        catalog.delete_selected().unwrap();
        assert_eq!(catalog.selected(), ThemeId::BuiltIn(BuiltInTheme::Dark));
        assert_eq!(catalog.resolved_appearance().tokens, ThemeTokens::dark());
    }

    #[test]
    fn dirty_navigation_supports_save_discard_and_cancel() {
        let mut catalog = ThemeCatalog::default();
        catalog.set_draft_name("Derived".to_owned());
        assert!(
            !catalog
                .request_selection(ThemeId::BuiltIn(BuiltInTheme::Light))
                .unwrap()
        );
        assert!(!catalog.resolve_pending(DirtyDecision::Cancel).unwrap());
        assert_eq!(catalog.selected(), ThemeId::BuiltIn(BuiltInTheme::Dark));
        assert!(
            !catalog
                .request_selection(ThemeId::BuiltIn(BuiltInTheme::Light))
                .unwrap()
        );
        assert!(catalog.resolve_pending(DirtyDecision::Save).unwrap());
        assert_eq!(catalog.saved_themes().len(), 1);
        assert_eq!(catalog.selected(), ThemeId::BuiltIn(BuiltInTheme::Light));
        catalog.set_draft_name("Throw away".to_owned());
        assert!(
            !catalog
                .request_selection(ThemeId::BuiltIn(BuiltInTheme::Glass))
                .unwrap()
        );
        assert!(catalog.resolve_pending(DirtyDecision::Discard).unwrap());
        assert_eq!(catalog.selected(), ThemeId::BuiltIn(BuiltInTheme::Glass));
        assert_eq!(catalog.saved_themes().len(), 1);
    }

    #[test]
    fn persistence_round_trip_keeps_multiple_themes_and_selection() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("settings.json");
        let mut preferences = Preferences::default();
        save_named(&mut preferences.themes, "One");
        let selected = save_named(&mut preferences.themes, "Two");
        preferences.themes.set_glass();
        preferences.themes.set_glass_values(12, 34, true);
        let mut tokens = preferences.themes.resolved_appearance().tokens.clone();
        tokens.panel = HexColor([37, 45, 53, 96]);
        preferences.themes.set_draft_tokens(tokens);
        preferences.themes.save_draft().unwrap();
        preferences.save_to(&path).unwrap();
        let loaded = Preferences::load_from(&path);
        assert_eq!(loaded.themes.selected(), selected);
        assert_eq!(loaded.themes.saved_themes().len(), 2);
        assert_eq!(
            loaded.themes.resolved_appearance(),
            preferences.themes.resolved_appearance()
        );
        let persisted: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(persisted["schema_version"], SETTINGS_SCHEMA_VERSION);
    }

    #[test]
    fn migrates_schema_two_rgb_colors_as_opaque() {
        let loaded = Preferences::from_json(
            r#"{"schema_version":2,"selected_theme":{"kind":"built_in","value":"dark"},"saved_themes":[],"next_saved_theme_id":1,"jpeg_background":[12,34,56]}"#,
        )
        .unwrap();
        assert_eq!(
            loaded.themes.selected(),
            ThemeId::BuiltIn(BuiltInTheme::Dark)
        );
        assert_eq!(loaded.jpeg_background, HexColor([12, 34, 56, 255]));
    }

    #[test]
    fn migrates_legacy_builtins() {
        for (mode, expected) in [
            ("light", BuiltInTheme::Light),
            ("dark", BuiltInTheme::Dark),
            ("glass", BuiltInTheme::Glass),
        ] {
            let raw = format!(r#"{{"active_theme":"{mode}"}}"#);
            let loaded = Preferences::from_json(&raw).unwrap();
            assert_eq!(loaded.themes.selected(), ThemeId::BuiltIn(expected));
        }
    }

    #[test]
    fn migrates_legacy_custom_and_custom_glass() {
        let custom = Preferences::from_json(r#"{"active_theme":"custom","custom_tokens":{"canvas":[1,1,1],"background":[2,2,2],"panel":[3,3,3],"control":[4,4,4],"field":[5,5,5],"row_alt":[6,6,6],"border":[7,7,7],"text":[8,8,8],"muted":[9,9,9],"accent":[10,10,10],"on_accent":[11,11,11],"danger":[12,12,12],"title":[13,13,13],"title_text":[14,14,14],"title_muted":[15,15,15],"title_rule":[16,16,16],"title_control":[17,17,17]}}"#).unwrap();
        assert_eq!(custom.themes.selected_label(), "My theme");
        assert_eq!(
            custom.themes.resolved_appearance().tokens.accent,
            HexColor([10, 10, 10, 255])
        );
        assert_eq!(
            custom.themes.resolved_appearance().material,
            WindowMaterial::Solid
        );

        let glass = Preferences::from_json(r#"{"active_theme":"custom_glass","glass_blur":27,"glass_translucency":61,"solid_when_inactive":true}"#).unwrap();
        assert_eq!(glass.themes.selected_label(), "My glass");
        assert_eq!(
            glass.themes.resolved_appearance().material,
            WindowMaterial::Glass {
                blur: 27,
                translucency: 61,
                solid_when_inactive: true
            }
        );
    }

    #[test]
    fn migrates_transitional_saved_custom_without_changing_builtin_selection() {
        let loaded = Preferences::from_json(r#"{"active_theme":"light","custom_theme_saved":true,"custom_theme_uses_glass":true,"glass_blur":22,"glass_translucency":48}"#).unwrap();
        assert_eq!(
            loaded.themes.selected(),
            ThemeId::BuiltIn(BuiltInTheme::Light)
        );
        assert_eq!(loaded.themes.saved_themes().len(), 1);
        assert_eq!(loaded.themes.saved_themes()[0].name, "My glass");
        assert_eq!(
            loaded.themes.saved_themes()[0].appearance.material,
            WindowMaterial::Glass {
                blur: 22,
                translucency: 48,
                solid_when_inactive: false
            }
        );
    }

    #[test]
    fn corrupt_or_unknown_settings_fall_back_safely() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("settings.json");
        fs::write(&path, "{not json").unwrap();
        assert_eq!(Preferences::load_from(&path), Preferences::default());
        fs::write(&path, r#"{"schema_version":999}"#).unwrap();
        assert_eq!(Preferences::load_from(&path), Preferences::default());
    }
}

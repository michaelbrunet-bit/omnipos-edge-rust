use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::catalog::{CategoryConfig, PrepStationConfig};
use crate::tax::TaxRule;

#[derive(Debug, Error)]
pub enum ThemeError {
    #[error("Store name cannot be empty or exceed 80 characters")]
    InvalidStoreName,
    #[error("Invalid hex color value for '{field}': {value}")]
    InvalidColor { field: String, value: String },
    #[error("Unsafe custom CSS rejected: {reason}")]
    UnsafeCustomCss { reason: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IndustryVertical {
    Hospitality,
    SalonBarber,
    Retail,
    Custom,
}

impl Default for IndustryVertical {
    fn default() -> Self {
        Self::Hospitality
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColorScheme {
    Dark,
    Light,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BorderRadiusStyle {
    Sharp,
    Rounded,
    Pill,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiDensity {
    Compact,
    Comfortable,
    Spacious,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoreTheme {
    pub preset_id: String,
    #[serde(default)]
    pub vertical: IndustryVertical,
    pub store_name: String,
    pub tagline: String,
    pub logo_text: String,
    pub currency_symbol: String,
    pub color_scheme: ColorScheme,
    pub primary_color: String,
    pub secondary_color: String,
    pub accent_color: String,
    pub background_color: String,
    pub surface_color: String,
    pub text_color: String,
    pub muted_text_color: String,
    pub font_family: String,
    pub border_radius: BorderRadiusStyle,
    pub ui_density: UiDensity,
    pub receipt_header: String,
    pub receipt_footer: String,
    pub custom_css: Option<String>,
}

impl Default for StoreTheme {
    fn default() -> Self {
        Self::preset_neon_craft_bar()
    }
}

impl StoreTheme {
    pub fn validate(&self) -> Result<(), ThemeError> {
        let trimmed = self.store_name.trim();
        if trimmed.is_empty() || trimmed.len() > 80 {
            return Err(ThemeError::InvalidStoreName);
        }

        let color_fields = [
            ("primary_color", &self.primary_color),
            ("secondary_color", &self.secondary_color),
            ("accent_color", &self.accent_color),
            ("background_color", &self.background_color),
            ("surface_color", &self.surface_color),
            ("text_color", &self.text_color),
            ("muted_text_color", &self.muted_text_color),
        ];

        for (field, value) in color_fields {
            if !is_valid_hex_color(value) {
                return Err(ThemeError::InvalidColor {
                    field: field.to_string(),
                    value: value.clone(),
                });
            }
        }

        if let Some(css) = &self.custom_css {
            let lowered = css.to_ascii_lowercase();
            for forbidden in ["javascript:", "<script", "@import", "expression("] {
                if lowered.contains(forbidden) {
                    return Err(ThemeError::UnsafeCustomCss {
                        reason: format!("contains forbidden token '{forbidden}'"),
                    });
                }
            }
        }

        Ok(())
    }

    pub fn preset_neon_craft_bar() -> Self {
        Self {
            preset_id: "neon_craft_bar".to_string(),
            vertical: IndustryVertical::Hospitality,
            store_name: "Copper & Hops Craft Taproom".to_string(),
            tagline: "Artisanal Pourhouse & Wood-Fired Kitchen (Aloha + Toast Mode)".to_string(),
            logo_text: "C&H".to_string(),
            currency_symbol: "$".to_string(),
            color_scheme: ColorScheme::Dark,
            primary_color: "#f59e0b".to_string(),
            secondary_color: "#0ea5e9".to_string(),
            accent_color: "#10b981".to_string(),
            background_color: "#0f172a".to_string(),
            surface_color: "#1e293b".to_string(),
            text_color: "#f8fafc".to_string(),
            muted_text_color: "#94a3b8".to_string(),
            font_family: "'Inter', system-ui, sans-serif".to_string(),
            border_radius: BorderRadiusStyle::Rounded,
            ui_density: UiDensity::Comfortable,
            receipt_header: "COPPER & HOPS TAPROOM • AUSTIN, TX".to_string(),
            receipt_footer: "Cheers! Ask your server about our Craft Club Loyalty perks.".to_string(),
            custom_css: None,
        }
    }

    pub fn preset_bistro_elegance() -> Self {
        Self {
            preset_id: "bistro_elegance".to_string(),
            vertical: IndustryVertical::Hospitality,
            store_name: "Maison Lumière Bistro".to_string(),
            tagline: "Modern French Dining & Wine Bar".to_string(),
            logo_text: "ML".to_string(),
            currency_symbol: "$".to_string(),
            color_scheme: ColorScheme::Dark,
            primary_color: "#e11d48".to_string(),
            secondary_color: "#c084fc".to_string(),
            accent_color: "#fbbf24".to_string(),
            background_color: "#18181b".to_string(),
            surface_color: "#27272a".to_string(),
            text_color: "#fafafa".to_string(),
            muted_text_color: "#a1a1aa".to_string(),
            font_family: "'Georgia', 'Times New Roman', serif".to_string(),
            border_radius: BorderRadiusStyle::Sharp,
            ui_density: UiDensity::Spacious,
            receipt_header: "MAISON LUMIÈRE • FINE DINING & CELLAR".to_string(),
            receipt_footer: "Merci pour votre visite • Bon Appétit!".to_string(),
            custom_css: None,
        }
    }

    pub fn preset_crown_blade_barber() -> Self {
        Self {
            preset_id: "crown_blade_barber".to_string(),
            vertical: IndustryVertical::SalonBarber,
            store_name: "Crown & Blade Barber & Salon Studio".to_string(),
            tagline: "Master Barbers, Colorists & Apothecary Goods".to_string(),
            logo_text: "C&B".to_string(),
            currency_symbol: "$".to_string(),
            color_scheme: ColorScheme::Dark,
            primary_color: "#d97706".to_string(),
            secondary_color: "#6366f1".to_string(),
            accent_color: "#14b8a6".to_string(),
            background_color: "#111827".to_string(),
            surface_color: "#1f2937".to_string(),
            text_color: "#f9fafb".to_string(),
            muted_text_color: "#9ca3af".to_string(),
            font_family: "'Space Grotesk', system-ui, sans-serif".to_string(),
            border_radius: BorderRadiusStyle::Rounded,
            ui_density: UiDensity::Comfortable,
            receipt_header: "CROWN & BLADE STUDIO • APPOINTMENTS & WALK-INS".to_string(),
            receipt_footer: "Looking sharp! Rebook your next cut in 3 weeks for 10% off.".to_string(),
            custom_css: None,
        }
    }

    pub fn preset_foundry_retail() -> Self {
        Self {
            preset_id: "foundry_retail".to_string(),
            vertical: IndustryVertical::Retail,
            store_name: "Foundry Goods & Modern Apparel".to_string(),
            tagline: "Curated Menswear, Footwear & Everyday Carry".to_string(),
            logo_text: "FG".to_string(),
            currency_symbol: "$".to_string(),
            color_scheme: ColorScheme::Light,
            primary_color: "#2563eb".to_string(),
            secondary_color: "#0f172a".to_string(),
            accent_color: "#059669".to_string(),
            background_color: "#f8fafc".to_string(),
            surface_color: "#ffffff".to_string(),
            text_color: "#0f172a".to_string(),
            muted_text_color: "#64748b".to_string(),
            font_family: "'Inter', system-ui, sans-serif".to_string(),
            border_radius: BorderRadiusStyle::Sharp,
            ui_density: UiDensity::Compact,
            receipt_header: "FOUNDRY GOODS CO. • FLAGSHIP STORE".to_string(),
            receipt_footer: "30-day unworn returns with receipt barcode • Thank you!".to_string(),
            custom_css: None,
        }
    }

    pub fn preset_sunlit_cafe() -> Self {
        Self {
            preset_id: "sunlit_cafe".to_string(),
            vertical: IndustryVertical::Hospitality,
            store_name: "Solstice Roasters & Kitchen".to_string(),
            tagline: "Specialty Coffee, Brunch & Natural Cocktails".to_string(),
            logo_text: "SR".to_string(),
            currency_symbol: "$".to_string(),
            color_scheme: ColorScheme::Light,
            primary_color: "#059669".to_string(),
            secondary_color: "#d97706".to_string(),
            accent_color: "#2563eb".to_string(),
            background_color: "#f8fafc".to_string(),
            surface_color: "#ffffff".to_string(),
            text_color: "#0f172a".to_string(),
            muted_text_color: "#64748b".to_string(),
            font_family: "'Plus Jakarta Sans', system-ui, sans-serif".to_string(),
            border_radius: BorderRadiusStyle::Pill,
            ui_density: UiDensity::Comfortable,
            receipt_header: "SOLSTICE ROASTERS • SUSTAINABLY SOURCED".to_string(),
            receipt_footer: "Have a bright day! Free Wi-Fi: SolsticeGuest".to_string(),
            custom_css: None,
        }
    }

    pub fn all_presets() -> Vec<Self> {
        vec![
            Self::preset_neon_craft_bar(),
            Self::preset_crown_blade_barber(),
            Self::preset_foundry_retail(),
            Self::preset_bistro_elegance(),
            Self::preset_sunlit_cafe(),
        ]
    }
}

/// Self-contained tenant/store profile so multiple verticals or store locations
/// coexist cleanly in storage without overwriting each other.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreProfile {
    pub store_id: String,
    pub vertical: IndustryVertical,
    pub theme: StoreTheme,
    pub categories: Vec<CategoryConfig>,
    pub prep_stations: Vec<PrepStationConfig>,
    pub tax_rules: Vec<TaxRule>,
}

impl StoreProfile {
    pub fn default_stores() -> Vec<Self> {
        vec![
            Self {
                store_id: "store-hospitality".to_string(),
                vertical: IndustryVertical::Hospitality,
                theme: StoreTheme::preset_neon_craft_bar(),
                categories: CategoryConfig::hospitality_categories(),
                prep_stations: PrepStationConfig::hospitality_stations(),
                tax_rules: TaxRule::default_rules(),
            },
            Self {
                store_id: "store-salon".to_string(),
                vertical: IndustryVertical::SalonBarber,
                theme: StoreTheme::preset_crown_blade_barber(),
                categories: CategoryConfig::salon_categories(),
                prep_stations: PrepStationConfig::salon_stations(),
                tax_rules: TaxRule::default_rules(),
            },
            Self {
                store_id: "store-retail".to_string(),
                vertical: IndustryVertical::Retail,
                theme: StoreTheme::preset_foundry_retail(),
                categories: CategoryConfig::retail_categories(),
                prep_stations: PrepStationConfig::retail_stations(),
                tax_rules: TaxRule::default_rules(),
            },
        ]
    }
}

fn is_valid_hex_color(value: &str) -> bool {
    let bytes = value.as_bytes();
    if (bytes.len() != 4 && bytes.len() != 7) || bytes[0] != b'#' {
        return false;
    }
    bytes[1..].iter().all(|b| b.is_ascii_hexdigit())
}

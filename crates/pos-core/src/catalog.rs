use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::tax::TaxMode;
use crate::theme::IndustryVertical;

/// Data-driven category definition (replaces closed Rust enums so merchants can add custom categories).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CategoryConfig {
    pub id: String,
    pub label: String,
    pub icon_emoji: String,
    pub sort_order: u32,
    pub default_tax_rule_id: String,
}

impl CategoryConfig {
    pub fn hospitality_categories() -> Vec<Self> {
        vec![
            Self {
                id: "cocktails".to_string(),
                label: "Cocktails".to_string(),
                icon_emoji: "🥃".to_string(),
                sort_order: 1,
                default_tax_rule_id: "tax-standard-sales".to_string(),
            },
            Self {
                id: "draft_beer".to_string(),
                label: "Draft Beer".to_string(),
                icon_emoji: "🍺".to_string(),
                sort_order: 2,
                default_tax_rule_id: "tax-standard-sales".to_string(),
            },
            Self {
                id: "wine".to_string(),
                label: "Wine".to_string(),
                icon_emoji: "🍷".to_string(),
                sort_order: 3,
                default_tax_rule_id: "tax-standard-sales".to_string(),
            },
            Self {
                id: "appetizers".to_string(),
                label: "Appetizers".to_string(),
                icon_emoji: "🍟".to_string(),
                sort_order: 4,
                default_tax_rule_id: "tax-standard-sales".to_string(),
            },
            Self {
                id: "mains".to_string(),
                label: "Mains".to_string(),
                icon_emoji: "🍔".to_string(),
                sort_order: 5,
                default_tax_rule_id: "tax-standard-sales".to_string(),
            },
            Self {
                id: "desserts".to_string(),
                label: "Desserts".to_string(),
                icon_emoji: "🍰".to_string(),
                sort_order: 6,
                default_tax_rule_id: "tax-standard-sales".to_string(),
            },
        ]
    }

    pub fn salon_categories() -> Vec<Self> {
        vec![
            Self {
                id: "haircuts_fades".to_string(),
                label: "Cuts & Skin Fades".to_string(),
                icon_emoji: "💈".to_string(),
                sort_order: 1,
                default_tax_rule_id: "tax-service-exempt".to_string(),
            },
            Self {
                id: "beard_grooming".to_string(),
                label: "Beard & Hot Towel".to_string(),
                icon_emoji: "🪒".to_string(),
                sort_order: 2,
                default_tax_rule_id: "tax-service-exempt".to_string(),
            },
            Self {
                id: "color_treatments".to_string(),
                label: "Balayage & Color".to_string(),
                icon_emoji: "✨".to_string(),
                sort_order: 3,
                default_tax_rule_id: "tax-service-exempt".to_string(),
            },
            Self {
                id: "salon_retail".to_string(),
                label: "Take-Home Apothecary".to_string(),
                icon_emoji: "🫙".to_string(),
                sort_order: 4,
                default_tax_rule_id: "tax-standard-sales".to_string(),
            },
        ]
    }

    pub fn retail_categories() -> Vec<Self> {
        vec![
            Self {
                id: "apparel".to_string(),
                label: "Apparel & Denim".to_string(),
                icon_emoji: "🧥".to_string(),
                sort_order: 1,
                default_tax_rule_id: "tax-standard-sales".to_string(),
            },
            Self {
                id: "footwear".to_string(),
                label: "Footwear".to_string(),
                icon_emoji: "🥾".to_string(),
                sort_order: 2,
                default_tax_rule_id: "tax-standard-sales".to_string(),
            },
            Self {
                id: "accessories".to_string(),
                label: "Everyday Carry".to_string(),
                icon_emoji: "🖊️".to_string(),
                sort_order: 3,
                default_tax_rule_id: "tax-standard-sales".to_string(),
            },
            Self {
                id: "home_goods".to_string(),
                label: "Home Goods".to_string(),
                icon_emoji: "🕯️".to_string(),
                sort_order: 4,
                default_tax_rule_id: "tax-standard-sales".to_string(),
            },
        ]
    }
}

/// Data-driven routing station (replaces closed PrepStation enum).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PrepStationConfig {
    pub id: String,
    pub label: String,
    pub is_kds_routed: bool,
}

impl PrepStationConfig {
    pub fn hospitality_stations() -> Vec<Self> {
        vec![
            Self {
                id: "bar".to_string(),
                label: "Service Bar & Speed Rail".to_string(),
                is_kds_routed: true,
            },
            Self {
                id: "kitchen".to_string(),
                label: "Hot Line & Wood Oven KDS".to_string(),
                is_kds_routed: true,
            },
            Self {
                id: "counter".to_string(),
                label: "Expo / Counter".to_string(),
                is_kds_routed: false,
            },
        ]
    }

    pub fn salon_stations() -> Vec<Self> {
        vec![
            Self {
                id: "salon_chair".to_string(),
                label: "Barber / Stylist Chair".to_string(),
                is_kds_routed: true,
            },
            Self {
                id: "counter".to_string(),
                label: "Front Desk Retail Shelf".to_string(),
                is_kds_routed: false,
            },
        ]
    }

    pub fn retail_stations() -> Vec<Self> {
        vec![Self {
            id: "retail_register".to_string(),
            label: "Point-of-Sale Bagging Counter".to_string(),
            is_kds_routed: false,
        }]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModifierQualifier {
    Regular,
    No,
    Light,
    Extra,
    OnSide,
}

impl Default for ModifierQualifier {
    fn default() -> Self {
        Self::Regular
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModifierOption {
    pub id: String,
    pub name: String,
    pub price_delta_cents: i64,
    #[serde(default)]
    pub qualifier: ModifierQualifier,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CatalogItem {
    pub id: String,
    #[serde(default = "default_hospitality_store_id")]
    pub store_id: String,
    pub sku: String,
    #[serde(default)]
    pub vertical: IndustryVertical,
    pub name: String,
    pub description: String,
    /// Open category identifier referencing `CategoryConfig.id` (e.g., "cocktails", "haircuts_fades", or merchant-defined)
    pub category: String,
    pub price_cents: i64,
    pub tax_rate_bps: u32,
    #[serde(default)]
    pub tax_mode: TaxMode,
    /// Open station identifier referencing `PrepStationConfig.id`
    pub prep_station: String,
    pub available: bool,
    #[serde(default)]
    pub stock_count: Option<u32>,
    #[serde(default)]
    pub duration_minutes: Option<u32>,
    #[serde(default)]
    pub barcode_upc: Option<String>,
    pub badge_emoji: String,
    pub tags: Vec<String>,
    pub modifiers: Vec<ModifierOption>,
    /// Extensible metadata bag for arbitrary vertical/merchant fields
    #[serde(default)]
    pub attributes: BTreeMap<String, String>,
}

fn default_hospitality_store_id() -> String {
    "store-hospitality".to_string()
}

impl CatalogItem {
    pub fn catalog_for_store(store_id: &str, vertical: IndustryVertical) -> Vec<Self> {
        match vertical {
            IndustryVertical::Hospitality | IndustryVertical::Custom => {
                Self::hospitality_catalog(store_id)
            }
            IndustryVertical::SalonBarber => Self::salon_barber_catalog(store_id),
            IndustryVertical::Retail => Self::retail_catalog(store_id),
        }
    }

    pub fn hospitality_catalog(store_id: &str) -> Vec<Self> {
        vec![
            Self {
                id: "item-smoked-old-fashioned".to_string(),
                store_id: store_id.to_string(),
                sku: "BAR-101".to_string(),
                vertical: IndustryVertical::Hospitality,
                name: "Smoked Bourbon Old Fashioned".to_string(),
                description: "Small-batch Kentucky bourbon, demerara, angostura, hickory smoke & orange peel.".to_string(),
                category: "cocktails".to_string(),
                price_cents: 1500,
                tax_rate_bps: 825,
                tax_mode: TaxMode::Exclusive,
                prep_station: "bar".to_string(),
                available: true,
                stock_count: Some(24),
                duration_minutes: None,
                barcode_upc: None,
                badge_emoji: "🥃".to_string(),
                tags: vec!["popular".to_string(), "craft".to_string()],
                modifiers: vec![
                    ModifierOption {
                        id: "mod-large-rock".to_string(),
                        name: "King Cube Ice".to_string(),
                        price_delta_cents: 0,
                        qualifier: ModifierQualifier::Regular,
                    },
                    ModifierOption {
                        id: "mod-top-shelf-rye".to_string(),
                        name: "Single Barrel Rye Upgrade".to_string(),
                        price_delta_cents: 400,
                        qualifier: ModifierQualifier::Regular,
                    },
                ],
                attributes: BTreeMap::from([("abv".to_string(), "34%".to_string())]),
            },
            Self {
                id: "item-yuzu-mezcal-margarita".to_string(),
                store_id: store_id.to_string(),
                sku: "BAR-102".to_string(),
                vertical: IndustryVertical::Hospitality,
                name: "Spicy Yuzu Mezcalita".to_string(),
                description: "Artisanal Oaxaca mezcal, fresh yuzu juice, agave nectar, charred jalapeño & tajín rim.".to_string(),
                category: "cocktails".to_string(),
                price_cents: 1400,
                tax_rate_bps: 825,
                tax_mode: TaxMode::Exclusive,
                prep_station: "bar".to_string(),
                available: true,
                stock_count: Some(18),
                duration_minutes: None,
                barcode_upc: None,
                badge_emoji: "🍸".to_string(),
                tags: vec!["spicy".to_string(), "craft".to_string()],
                modifiers: vec![
                    ModifierOption {
                        id: "mod-no-salt".to_string(),
                        name: "Tajín Rim".to_string(),
                        price_delta_cents: 0,
                        qualifier: ModifierQualifier::Regular,
                    },
                    ModifierOption {
                        id: "mod-extra-mezcal".to_string(),
                        name: "Mezcal Float".to_string(),
                        price_delta_cents: 350,
                        qualifier: ModifierQualifier::Regular,
                    },
                ],
                attributes: BTreeMap::new(),
            },
            Self {
                id: "item-hazy-ipa".to_string(),
                store_id: store_id.to_string(),
                sku: "DRF-201".to_string(),
                vertical: IndustryVertical::Hospitality,
                name: "Cloudburst Hazy IPA (16oz)".to_string(),
                description: "Double dry-hopped Citra & Mosaic draft with tropical mango notes (6.8% ABV).".to_string(),
                category: "draft_beer".to_string(),
                price_cents: 850,
                tax_rate_bps: 825,
                tax_mode: TaxMode::Exclusive,
                prep_station: "bar".to_string(),
                available: true,
                stock_count: Some(40),
                duration_minutes: None,
                barcode_upc: None,
                badge_emoji: "🍺".to_string(),
                tags: vec!["draft".to_string(), "popular".to_string()],
                modifiers: vec![ModifierOption {
                    id: "mod-pitcher".to_string(),
                    name: "60oz Pitcher Upgrade".to_string(),
                    price_delta_cents: 1950,
                    qualifier: ModifierQualifier::Regular,
                }],
                attributes: BTreeMap::from([("abv".to_string(), "6.8%".to_string())]),
            },
            Self {
                id: "item-pinot-noir".to_string(),
                store_id: store_id.to_string(),
                sku: "WIN-301".to_string(),
                vertical: IndustryVertical::Hospitality,
                name: "Willamette Valley Pinot Noir".to_string(),
                description: "Oregon estate red wine with dark cherry, cedar, and silky tannins (6oz pour).".to_string(),
                category: "wine".to_string(),
                price_cents: 1300,
                tax_rate_bps: 825,
                tax_mode: TaxMode::Exclusive,
                prep_station: "bar".to_string(),
                available: true,
                stock_count: Some(9),
                duration_minutes: None,
                barcode_upc: None,
                badge_emoji: "🍷".to_string(),
                tags: vec!["wine".to_string(), "vegan".to_string()],
                modifiers: vec![ModifierOption {
                    id: "mod-wine-bottle".to_string(),
                    name: "Full Bottle (750ml)".to_string(),
                    price_delta_cents: 3500,
                    qualifier: ModifierQualifier::Regular,
                }],
                attributes: BTreeMap::new(),
            },
            Self {
                id: "item-truffle-fries".to_string(),
                store_id: store_id.to_string(),
                sku: "KIT-401".to_string(),
                vertical: IndustryVertical::Hospitality,
                name: "Black Truffle Parmesan Fries".to_string(),
                description: "Hand-cut Kennebec potatoes, white truffle oil, 24-month aged Parmigiano, garlic aioli.".to_string(),
                category: "appetizers".to_string(),
                price_cents: 1150,
                tax_rate_bps: 825,
                tax_mode: TaxMode::Exclusive,
                prep_station: "kitchen".to_string(),
                available: true,
                stock_count: Some(15),
                duration_minutes: None,
                barcode_upc: None,
                badge_emoji: "🍟".to_string(),
                tags: vec!["vegetarian".to_string(), "shareable".to_string()],
                modifiers: vec![ModifierOption {
                    id: "mod-extra-aioli".to_string(),
                    name: "Garlic Aioli".to_string(),
                    price_delta_cents: 150,
                    qualifier: ModifierQualifier::Regular,
                }],
                attributes: BTreeMap::new(),
            },
            Self {
                id: "item-wagyu-smash-burger".to_string(),
                store_id: store_id.to_string(),
                sku: "KIT-501".to_string(),
                vertical: IndustryVertical::Hospitality,
                name: "Double Wagyu Smash Burger".to_string(),
                description: "Two crispy-edge Texas Wagyu patties, smoked cheddar, caramelized onion jam, brioche bun.".to_string(),
                category: "mains".to_string(),
                price_cents: 1950,
                tax_rate_bps: 825,
                tax_mode: TaxMode::Exclusive,
                prep_station: "kitchen".to_string(),
                available: true,
                stock_count: Some(6),
                duration_minutes: None,
                barcode_upc: None,
                badge_emoji: "🍔".to_string(),
                tags: vec!["popular".to_string(), "chef-special".to_string()],
                modifiers: vec![
                    ModifierOption {
                        id: "mod-thick-bacon".to_string(),
                        name: "Applewood Bacon".to_string(),
                        price_delta_cents: 300,
                        qualifier: ModifierQualifier::Regular,
                    },
                    ModifierOption {
                        id: "mod-gf-bun".to_string(),
                        name: "Gluten-Free Artisan Bun".to_string(),
                        price_delta_cents: 200,
                        qualifier: ModifierQualifier::Regular,
                    },
                ],
                attributes: BTreeMap::new(),
            },
            Self {
                id: "item-hot-honey-pepperoni-pizza".to_string(),
                store_id: store_id.to_string(),
                sku: "KIT-502".to_string(),
                vertical: IndustryVertical::Hospitality,
                name: "Wood-Fired Hot Honey Soppressata Pizza".to_string(),
                description: "72-hour sourdough crust, San Marzano tomato, fior di latte, soppressata, habanero honey.".to_string(),
                category: "mains".to_string(),
                price_cents: 2100,
                tax_rate_bps: 825,
                tax_mode: TaxMode::Exclusive,
                prep_station: "kitchen".to_string(),
                available: true,
                stock_count: Some(12),
                duration_minutes: None,
                barcode_upc: None,
                badge_emoji: "🍕".to_string(),
                tags: vec!["spicy".to_string(), "shareable".to_string()],
                modifiers: vec![ModifierOption {
                    id: "mod-burrata-center".to_string(),
                    name: "Fresh Burrata Center".to_string(),
                    price_delta_cents: 450,
                    qualifier: ModifierQualifier::Regular,
                }],
                attributes: BTreeMap::new(),
            },
            Self {
                id: "item-basque-cheesecake".to_string(),
                store_id: store_id.to_string(),
                sku: "KIT-601".to_string(),
                vertical: IndustryVertical::Hospitality,
                name: "Burnt Basque Cheesecake".to_string(),
                description: "Caramelized crust, creamy vanilla bean center, macerated blackberry compote.".to_string(),
                category: "desserts".to_string(),
                price_cents: 1100,
                tax_rate_bps: 825,
                tax_mode: TaxMode::Exclusive,
                prep_station: "kitchen".to_string(),
                available: true,
                stock_count: Some(4),
                duration_minutes: None,
                barcode_upc: None,
                badge_emoji: "🍰".to_string(),
                tags: vec!["vegetarian".to_string(), "sweet".to_string()],
                modifiers: vec![],
                attributes: BTreeMap::new(),
            },
        ]
    }

    pub fn salon_barber_catalog(store_id: &str) -> Vec<Self> {
        vec![
            Self {
                id: "salon-executive-skin-fade".to_string(),
                store_id: store_id.to_string(),
                sku: "SVC-101".to_string(),
                vertical: IndustryVertical::SalonBarber,
                name: "Executive Skin Fade & Hot Towel Finish".to_string(),
                description: "Precision scissor & foil skin fade, straight-razor neck cleanup, eucalyptus hot towel & style.".to_string(),
                category: "haircuts_fades".to_string(),
                price_cents: 5500,
                tax_rate_bps: 0,
                tax_mode: TaxMode::Exempt,
                prep_station: "salon_chair".to_string(),
                available: true,
                stock_count: None,
                duration_minutes: Some(45),
                barcode_upc: None,
                badge_emoji: "💈".to_string(),
                tags: vec!["45-min".to_string(), "popular".to_string()],
                modifiers: vec![
                    ModifierOption {
                        id: "mod-razor-part".to_string(),
                        name: "Straight-Razor Hard Part".to_string(),
                        price_delta_cents: 1000,
                        qualifier: ModifierQualifier::Regular,
                    },
                    ModifierOption {
                        id: "mod-scalp-massage".to_string(),
                        name: "Tea-Tree Scalp Detox Massage (+15m)".to_string(),
                        price_delta_cents: 1800,
                        qualifier: ModifierQualifier::Regular,
                    },
                ],
                attributes: BTreeMap::from([("commission_class".to_string(), "service".to_string())]),
            },
            Self {
                id: "salon-signature-scissor-cut".to_string(),
                store_id: store_id.to_string(),
                sku: "SVC-102".to_string(),
                vertical: IndustryVertical::SalonBarber,
                name: "Signature Salon Cut & Blowout".to_string(),
                description: "Consultation, invigorating wash, precision layered cut, and round-brush silk blowout.".to_string(),
                category: "haircuts_fades".to_string(),
                price_cents: 8500,
                tax_rate_bps: 0,
                tax_mode: TaxMode::Exempt,
                prep_station: "salon_chair".to_string(),
                available: true,
                stock_count: None,
                duration_minutes: Some(60),
                barcode_upc: None,
                badge_emoji: "✂️".to_string(),
                tags: vec!["60-min".to_string(), "salon".to_string()],
                modifiers: vec![ModifierOption {
                    id: "mod-deep-mask".to_string(),
                    name: "Keratin Bond Conditioning Mask".to_string(),
                    price_delta_cents: 2500,
                    qualifier: ModifierQualifier::Regular,
                }],
                attributes: BTreeMap::from([("commission_class".to_string(), "service".to_string())]),
            },
            Self {
                id: "salon-beard-sculpt-shave".to_string(),
                store_id: store_id.to_string(),
                sku: "SVC-201".to_string(),
                vertical: IndustryVertical::SalonBarber,
                name: "Beard Sculpt & Straight-Razor Hot Lather".to_string(),
                description: "Beard shaping, three hot/cold aromatic towels, rich sandalwood lather, and cedar beard balm.".to_string(),
                category: "beard_grooming".to_string(),
                price_cents: 3800,
                tax_rate_bps: 0,
                tax_mode: TaxMode::Exempt,
                prep_station: "salon_chair".to_string(),
                available: true,
                stock_count: None,
                duration_minutes: Some(30),
                barcode_upc: None,
                badge_emoji: "🪒".to_string(),
                tags: vec!["30-min".to_string(), "grooming".to_string()],
                modifiers: vec![ModifierOption {
                    id: "mod-charcoal-peel".to_string(),
                    name: "Black Charcoal Pore Mask".to_string(),
                    price_delta_cents: 1500,
                    qualifier: ModifierQualifier::Regular,
                }],
                attributes: BTreeMap::from([("commission_class".to_string(), "service".to_string())]),
            },
            Self {
                id: "salon-dimensional-balayage".to_string(),
                store_id: store_id.to_string(),
                sku: "SVC-301".to_string(),
                vertical: IndustryVertical::SalonBarber,
                name: "Full Dimensional Balayage & Gloss Toner".to_string(),
                description: "Hand-painted sun-kissed highlights, custom pH-balanced gloss toner, bond builder & style.".to_string(),
                category: "color_treatments".to_string(),
                price_cents: 19500,
                tax_rate_bps: 0,
                tax_mode: TaxMode::Exempt,
                prep_station: "salon_chair".to_string(),
                available: true,
                stock_count: None,
                duration_minutes: Some(135),
                barcode_upc: None,
                badge_emoji: "✨".to_string(),
                tags: vec!["135-min".to_string(), "color".to_string()],
                modifiers: vec![ModifierOption {
                    id: "mod-root-shadow".to_string(),
                    name: "Root Shadow Smudge".to_string(),
                    price_delta_cents: 3500,
                    qualifier: ModifierQualifier::Regular,
                }],
                attributes: BTreeMap::from([("commission_class".to_string(), "service".to_string())]),
            },
            Self {
                id: "salon-matte-clay-pomade".to_string(),
                store_id: store_id.to_string(),
                sku: "RET-401".to_string(),
                vertical: IndustryVertical::SalonBarber,
                name: "Apothecary Matte Clay Pomade (4oz)".to_string(),
                description: "Kaolin clay & beeswax textured hold with zero shine. Eligible for 15% Service Bundle discount.".to_string(),
                category: "salon_retail".to_string(),
                price_cents: 2800,
                tax_rate_bps: 825,
                tax_mode: TaxMode::Exclusive,
                prep_station: "counter".to_string(),
                available: true,
                stock_count: Some(14),
                duration_minutes: None,
                barcode_upc: Some("8500192041".to_string()),
                badge_emoji: "🫙".to_string(),
                tags: vec!["take-home".to_string(), "bundle-eligible".to_string()],
                modifiers: vec![],
                attributes: BTreeMap::from([("commission_class".to_string(), "retail".to_string())]),
            },
            Self {
                id: "salon-argan-beard-elixir".to_string(),
                store_id: store_id.to_string(),
                sku: "RET-402".to_string(),
                vertical: IndustryVertical::SalonBarber,
                name: "Cold-Pressed Cedar & Argan Beard Oil".to_string(),
                description: "Organic Moroccan argan, jojoba, and Atlas cedarwood essential oil dropper bottle.".to_string(),
                category: "salon_retail".to_string(),
                price_cents: 3200,
                tax_rate_bps: 825,
                tax_mode: TaxMode::Exclusive,
                prep_station: "counter".to_string(),
                available: true,
                stock_count: Some(8),
                duration_minutes: None,
                barcode_upc: Some("8500192042".to_string()),
                badge_emoji: "💧".to_string(),
                tags: vec!["take-home".to_string(), "bundle-eligible".to_string()],
                modifiers: vec![],
                attributes: BTreeMap::from([("commission_class".to_string(), "retail".to_string())]),
            },
        ]
    }

    pub fn retail_catalog(store_id: &str) -> Vec<Self> {
        vec![
            Self {
                id: "ret-selvedge-denim-jacket".to_string(),
                store_id: store_id.to_string(),
                sku: "APP-101".to_string(),
                vertical: IndustryVertical::Retail,
                name: "14oz Japanese Selvedge Denim Chore Coat".to_string(),
                description: "Raw indigo rope-dyed selvedge canvas with custom copper tack buttons.".to_string(),
                category: "apparel".to_string(),
                price_cents: 16800,
                tax_rate_bps: 825,
                tax_mode: TaxMode::Exclusive,
                prep_station: "retail_register".to_string(),
                available: true,
                stock_count: Some(7),
                duration_minutes: None,
                barcode_upc: Some("01942500101".to_string()),
                badge_emoji: "🧥".to_string(),
                tags: vec!["selvedge".to_string(), "flagship".to_string()],
                modifiers: vec![
                    ModifierOption {
                        id: "mod-size-m".to_string(),
                        name: "Size: Medium".to_string(),
                        price_delta_cents: 0,
                        qualifier: ModifierQualifier::Regular,
                    },
                    ModifierOption {
                        id: "mod-size-l".to_string(),
                        name: "Size: Large".to_string(),
                        price_delta_cents: 0,
                        qualifier: ModifierQualifier::Regular,
                    },
                ],
                attributes: BTreeMap::from([("material".to_string(), "14oz Japanese Selvedge".to_string())]),
            },
            Self {
                id: "ret-heavyweight-pocket-tee".to_string(),
                store_id: store_id.to_string(),
                sku: "APP-102".to_string(),
                vertical: IndustryVertical::Retail,
                name: "9oz Loopwheel Heavyweight Pocket Tee".to_string(),
                description: "Knit on vintage loopwheel looms in Wakayama; tubular seamless torso.".to_string(),
                category: "apparel".to_string(),
                price_cents: 4800,
                tax_rate_bps: 825,
                tax_mode: TaxMode::Exclusive,
                prep_station: "retail_register".to_string(),
                available: true,
                stock_count: Some(22),
                duration_minutes: None,
                barcode_upc: Some("01942500102".to_string()),
                badge_emoji: "👕".to_string(),
                tags: vec!["bestseller".to_string(), "cotton".to_string()],
                modifiers: vec![
                    ModifierOption {
                        id: "mod-color-slate".to_string(),
                        name: "Color: Washed Slate".to_string(),
                        price_delta_cents: 0,
                        qualifier: ModifierQualifier::Regular,
                    },
                    ModifierOption {
                        id: "mod-color-oat".to_string(),
                        name: "Color: Natural Oat".to_string(),
                        price_delta_cents: 0,
                        qualifier: ModifierQualifier::Regular,
                    },
                ],
                attributes: BTreeMap::new(),
            },
            Self {
                id: "ret-goodyear-welt-boots".to_string(),
                store_id: store_id.to_string(),
                sku: "FTW-201".to_string(),
                vertical: IndustryVertical::Retail,
                name: "Horween Chromexcel Service Boots".to_string(),
                description: "Goodyear-welted Vibram mini-lug sole, vegetable-tanned leather insole.".to_string(),
                category: "footwear".to_string(),
                price_cents: 29500,
                tax_rate_bps: 825,
                tax_mode: TaxMode::Exclusive,
                prep_station: "retail_register".to_string(),
                available: true,
                stock_count: Some(3),
                duration_minutes: None,
                barcode_upc: Some("01942500201".to_string()),
                badge_emoji: "🥾".to_string(),
                tags: vec!["handcrafted".to_string(), "low-stock".to_string()],
                modifiers: vec![ModifierOption {
                    id: "mod-cedar-trees".to_string(),
                    name: "Add Cedar Boot Trees".to_string(),
                    price_delta_cents: 2800,
                    qualifier: ModifierQualifier::Regular,
                }],
                attributes: BTreeMap::new(),
            },
            Self {
                id: "ret-titanium-edc-pen".to_string(),
                store_id: store_id.to_string(),
                sku: "ACC-301".to_string(),
                vertical: IndustryVertical::Retail,
                name: "Machined Grade-5 Titanium Bolt Action Pen".to_string(),
                description: "CNC-milled solid titanium body with Schmidt EasyFlow pressurized ink cartridge.".to_string(),
                category: "accessories".to_string(),
                price_cents: 6500,
                tax_rate_bps: 825,
                tax_mode: TaxMode::Exclusive,
                prep_station: "retail_register".to_string(),
                available: true,
                stock_count: Some(15),
                duration_minutes: None,
                barcode_upc: Some("01942500301".to_string()),
                badge_emoji: "🖊️".to_string(),
                tags: vec!["edc".to_string(), "gift".to_string()],
                modifiers: vec![],
                attributes: BTreeMap::new(),
            },
            Self {
                id: "ret-hinoki-cedar-candle".to_string(),
                store_id: store_id.to_string(),
                sku: "HOM-401".to_string(),
                vertical: IndustryVertical::Retail,
                name: "Japanese Hinoki & Smoked Vetiver Soy Candle".to_string(),
                description: "Hand-poured 10oz matte ceramic vessel with 60-hour braided cotton burn time.".to_string(),
                category: "home_goods".to_string(),
                price_cents: 3600,
                tax_rate_bps: 825,
                tax_mode: TaxMode::Exclusive,
                prep_station: "retail_register".to_string(),
                available: true,
                stock_count: Some(19),
                duration_minutes: None,
                barcode_upc: Some("01942500401".to_string()),
                badge_emoji: "🕯️".to_string(),
                tags: vec!["home".to_string(), "bundle-favorite".to_string()],
                modifiers: vec![],
                attributes: BTreeMap::new(),
            },
        ]
    }
}

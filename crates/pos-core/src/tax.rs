use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaxMode {
    /// Tax is added on top of the net line price (e.g., US state/local sales tax).
    Exclusive,
    /// Tax is already embedded inside the item price (e.g., VAT / GST).
    Inclusive,
    /// Item is non-taxable (e.g., labor/service in exempt jurisdictions).
    Exempt,
}

impl Default for TaxMode {
    fn default() -> Self {
        Self::Exclusive
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaxRule {
    pub id: String,
    pub name: String,
    pub rate_bps: u32,
    pub mode: TaxMode,
}

impl TaxRule {
    pub fn default_rules() -> Vec<Self> {
        vec![
            Self {
                id: "tax-standard-sales".to_string(),
                name: "Standard State & Local Sales Tax (8.25%)".to_string(),
                rate_bps: 825,
                mode: TaxMode::Exclusive,
            },
            Self {
                id: "tax-service-exempt".to_string(),
                name: "Personal Care / Labor Exempt (0%)".to_string(),
                rate_bps: 0,
                mode: TaxMode::Exempt,
            },
            Self {
                id: "tax-vat-inclusive".to_string(),
                name: "Included Hospitality VAT (10% Inclusive)".to_string(),
                rate_bps: 1000,
                mode: TaxMode::Inclusive,
            },
        ]
    }
}

/// Computes `(added_exclusive_tax_cents, embedded_inclusive_tax_cents)` for a net line amount.
pub fn calculate_line_tax(net_amount_cents: i64, rate_bps: u32, mode: TaxMode) -> (i64, i64) {
    if net_amount_cents <= 0 || rate_bps == 0 || mode == TaxMode::Exempt {
        return (0, 0);
    }
    let bps = i64::from(rate_bps);
    match mode {
        TaxMode::Exclusive => {
            let tax = (net_amount_cents * bps + 5_000) / 10_000;
            (tax, 0)
        }
        TaxMode::Inclusive => {
            // Gross = Net * (10000 + bps) / 10000 => Embedded Tax = Gross - Gross * 10000 / (10000 + bps)
            let pre_tax = (net_amount_cents * 10_000 + (10_000 + bps) / 2) / (10_000 + bps);
            let embedded = (net_amount_cents - pre_tax).max(0);
            (0, embedded)
        }
        TaxMode::Exempt => (0, 0),
    }
}

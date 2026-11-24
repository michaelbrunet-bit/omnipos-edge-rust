use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::catalog::ModifierOption;
use crate::tax::{calculate_line_tax, TaxMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderSource {
    BartenderSpeedRail,
    ServerTableside,
    CustomerSelfService,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderStatus {
    Open,
    SentToPrep,
    CheckRequested,
    PartiallyPaid,
    Paid,
    Voided,
    Refunded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemPrepStatus {
    Held,
    Fired,
    Ready,
    Served,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrderLineItem {
    pub id: String,
    pub catalog_item_id: String,
    pub name: String,
    pub category: String,
    pub quantity: u32,
    pub unit_price_cents: i64,
    pub selected_modifiers: Vec<ModifierOption>,
    pub seat_number: Option<u8>,
    pub notes: Option<String>,
    pub added_by: String,
    pub source: OrderSource,
    pub prep_station: String,
    pub prep_status: ItemPrepStatus,
    pub tax_rate_bps: u32,
    #[serde(default)]
    pub tax_mode: TaxMode,
    #[serde(default)]
    pub comped: bool,
    #[serde(default)]
    pub comp_reason: Option<String>,
    #[serde(default)]
    pub comp_approved_by: Option<String>,
}

impl OrderLineItem {
    pub fn modifier_total_per_unit_cents(&self) -> i64 {
        self.selected_modifiers
            .iter()
            .map(|m| m.price_delta_cents)
            .sum()
    }

    pub fn effective_unit_price_cents(&self) -> i64 {
        self.unit_price_cents + self.modifier_total_per_unit_cents()
    }

    pub fn gross_line_cents(&self) -> i64 {
        self.effective_unit_price_cents() * i64::from(self.quantity)
    }

    /// Chargeable subtotal before order-level mod discounts (0 if comped).
    pub fn line_subtotal_cents(&self) -> i64 {
        if self.comped {
            0
        } else {
            self.gross_line_cents()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModAdjustment {
    pub mod_id: String,
    pub mod_name: String,
    pub label: String,
    /// Negative for discounts/promos, positive for service charges/surcharges
    pub amount_cents: i64,
    pub receipt_note: Option<String>,
    /// Optional categories targeted by this adjustment for exact line-item tax proration
    #[serde(default)]
    pub target_categories: Vec<String>,
    /// For positive surcharges, whether sales tax applies to the surcharge
    #[serde(default)]
    pub taxable_surcharge_bps: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaymentMethod {
    CardTapP2PE,
    ApplePay,
    GooglePay,
    Cash,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaymentRecord {
    pub id: String,
    pub amount_cents: i64,
    pub tip_cents: i64,
    pub method: PaymentMethod,
    pub payer_label: String,
    pub token_reference: String,
    pub paid_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RefundRecord {
    pub id: String,
    pub payment_id: String,
    pub amount_cents: i64,
    pub reason: String,
    pub approved_by: String,
    pub refunded_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SplitCheckShare {
    pub share_index: u8,
    pub total_shares: u8,
    pub label: String,
    pub subtotal_cents: i64,
    pub adjustment_cents: i64,
    pub tax_cents: i64,
    pub total_cents: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderSummaryTotals {
    pub gross_merchandise_cents: i64,
    pub comped_total_cents: i64,
    pub subtotal_cents: i64,
    pub adjustments_total_cents: i64,
    pub taxable_amount_cents: i64,
    pub tax_cents: i64,
    pub embedded_vat_cents: i64,
    pub total_before_tip_cents: i64,
    pub paid_principal_cents: i64,
    pub refunded_cents: i64,
    pub tips_total_cents: i64,
    pub remaining_balance_cents: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub id: String,
    #[serde(default = "default_store_id")]
    pub store_id: String,
    pub table_or_tab_name: String,
    pub is_bar_tab: bool,
    pub guest_count: u8,
    pub assigned_operator: String,
    #[serde(default)]
    pub preauth_card: Option<String>,
    #[serde(default)]
    pub client_profile_notes: Option<String>,
    pub status: OrderStatus,
    pub items: Vec<OrderLineItem>,
    pub mod_adjustments: Vec<ModAdjustment>,
    pub payments: Vec<PaymentRecord>,
    #[serde(default)]
    pub refunds: Vec<RefundRecord>,
    pub receipt_notes: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

fn default_store_id() -> String {
    "store-hospitality".to_string()
}

impl Order {
    pub fn new(
        id: impl Into<String>,
        table_or_tab_name: impl Into<String>,
        is_bar_tab: bool,
        guest_count: u8,
        assigned_operator: impl Into<String>,
    ) -> Self {
        Self::new_for_store(
            "store-hospitality",
            id,
            table_or_tab_name,
            is_bar_tab,
            guest_count,
            assigned_operator,
        )
    }

    pub fn new_for_store(
        store_id: impl Into<String>,
        id: impl Into<String>,
        table_or_tab_name: impl Into<String>,
        is_bar_tab: bool,
        guest_count: u8,
        assigned_operator: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: id.into(),
            store_id: store_id.into(),
            table_or_tab_name: table_or_tab_name.into(),
            is_bar_tab,
            guest_count: guest_count.max(1),
            assigned_operator: assigned_operator.into(),
            preauth_card: None,
            client_profile_notes: None,
            status: OrderStatus::Open,
            items: Vec::new(),
            mod_adjustments: Vec::new(),
            payments: Vec::new(),
            refunds: Vec::new(),
            receipt_notes: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn gross_merchandise_cents(&self) -> i64 {
        self.items.iter().map(OrderLineItem::gross_line_cents).sum()
    }

    pub fn comped_total_cents(&self) -> i64 {
        self.items
            .iter()
            .filter(|i| i.comped)
            .map(OrderLineItem::gross_line_cents)
            .sum()
    }

    pub fn subtotal_cents(&self) -> i64 {
        self.items
            .iter()
            .map(OrderLineItem::line_subtotal_cents)
            .sum()
    }

    pub fn adjustments_total_cents(&self) -> i64 {
        self.mod_adjustments.iter().map(|a| a.amount_cents).sum()
    }

    /// Computes exact per-line net amounts after category-targeted and order-wide discounts,
    /// returning `(taxable_base_cents, added_exclusive_tax_cents, embedded_inclusive_tax_cents)`.
    pub fn compute_tax_breakdown(&self) -> (i64, i64, i64) {
        if self.items.is_empty() {
            return (0, 0, 0);
        }

        let mut net_lines: Vec<i64> = self
            .items
            .iter()
            .map(OrderLineItem::line_subtotal_cents)
            .collect();

        // Distribute negative discount adjustments across matching lines proportionally
        for adj in &self.mod_adjustments {
            if adj.amount_cents < 0 {
                let discount_to_allocate = -adj.amount_cents;
                let eligible_indices: Vec<usize> = self
                    .items
                    .iter()
                    .enumerate()
                    .filter(|(idx, item)| {
                        net_lines[*idx] > 0
                            && (adj.target_categories.is_empty()
                                || adj.target_categories.contains(&item.category))
                    })
                    .map(|(idx, _)| idx)
                    .collect();

                let eligible_pool: i64 = eligible_indices.iter().map(|&idx| net_lines[idx]).sum();
                if eligible_pool > 0 {
                    let mut allocated = 0i64;
                    for (pos, &idx) in eligible_indices.iter().enumerate() {
                        let share = if pos + 1 == eligible_indices.len() {
                            (discount_to_allocate - allocated).max(0)
                        } else {
                            let s = (discount_to_allocate * net_lines[idx]) / eligible_pool;
                            allocated += s;
                            s
                        };
                        net_lines[idx] = (net_lines[idx] - share).max(0);
                    }
                }
            }
        }

        let mut taxable_base = 0i64;
        let mut exclusive_tax = 0i64;
        let mut embedded_vat = 0i64;

        for (idx, item) in self.items.iter().enumerate() {
            let net = net_lines[idx];
            if item.tax_mode != TaxMode::Exempt && item.tax_rate_bps > 0 {
                taxable_base += net;
            }
            let (added, embedded) = calculate_line_tax(net, item.tax_rate_bps, item.tax_mode);
            exclusive_tax += added;
            embedded_vat += embedded;
        }

        // Handle taxable positive surcharges if configured
        for adj in &self.mod_adjustments {
            if adj.amount_cents > 0 && adj.taxable_surcharge_bps > 0 {
                taxable_base += adj.amount_cents;
                let (added, _) =
                    calculate_line_tax(adj.amount_cents, adj.taxable_surcharge_bps, TaxMode::Exclusive);
                exclusive_tax += added;
            }
        }

        (taxable_base, exclusive_tax, embedded_vat)
    }

    pub fn taxable_amount_cents(&self) -> i64 {
        self.compute_tax_breakdown().0
    }

    pub fn tax_cents(&self) -> i64 {
        self.compute_tax_breakdown().1
    }

    pub fn embedded_vat_cents(&self) -> i64 {
        self.compute_tax_breakdown().2
    }

    pub fn total_before_tip_cents(&self) -> i64 {
        (self.subtotal_cents() + self.adjustments_total_cents() + self.tax_cents()).max(0)
    }

    pub fn paid_principal_cents(&self) -> i64 {
        self.payments.iter().map(|p| p.amount_cents).sum()
    }

    pub fn refunded_cents(&self) -> i64 {
        self.refunds.iter().map(|r| r.amount_cents).sum()
    }

    pub fn net_paid_principal_cents(&self) -> i64 {
        (self.paid_principal_cents() - self.refunded_cents()).max(0)
    }

    pub fn tips_total_cents(&self) -> i64 {
        self.payments.iter().map(|p| p.tip_cents).sum()
    }

    pub fn remaining_balance_cents(&self) -> i64 {
        if self.status == OrderStatus::Refunded || self.status == OrderStatus::Voided {
            return 0;
        }
        (self.total_before_tip_cents() - self.paid_principal_cents()).max(0)
    }

    pub fn totals(&self) -> OrderSummaryTotals {
        let (taxable_amount_cents, tax_cents, embedded_vat_cents) = self.compute_tax_breakdown();
        OrderSummaryTotals {
            gross_merchandise_cents: self.gross_merchandise_cents(),
            comped_total_cents: self.comped_total_cents(),
            subtotal_cents: self.subtotal_cents(),
            adjustments_total_cents: self.adjustments_total_cents(),
            taxable_amount_cents,
            tax_cents,
            embedded_vat_cents,
            total_before_tip_cents: self.total_before_tip_cents(),
            paid_principal_cents: self.paid_principal_cents(),
            refunded_cents: self.refunded_cents(),
            tips_total_cents: self.tips_total_cents(),
            remaining_balance_cents: self.remaining_balance_cents(),
        }
    }

    pub fn split_equally(&self, ways: u8) -> Vec<SplitCheckShare> {
        let ways = ways.clamp(1, 20);
        let remaining = self.remaining_balance_cents();
        let subtotal = self.subtotal_cents();
        let adj = self.adjustments_total_cents();
        let tax = self.tax_cents();

        let ways_i64 = i64::from(ways);
        let base_total = remaining / ways_i64;
        let remainder = remaining % ways_i64;

        (0..ways)
            .map(|idx| {
                let extra = if i64::from(idx) < remainder { 1 } else { 0 };
                SplitCheckShare {
                    share_index: idx + 1,
                    total_shares: ways,
                    label: format!("Split {} of {}", idx + 1, ways),
                    subtotal_cents: subtotal / ways_i64,
                    adjustment_cents: adj / ways_i64,
                    tax_cents: tax / ways_i64,
                    total_cents: base_total + extra,
                }
            })
            .collect()
    }

    pub fn split_by_seat(&self) -> Vec<SplitCheckShare> {
        let seats: Vec<u8> = self
            .items
            .iter()
            .filter(|i| !i.comped)
            .filter_map(|i| i.seat_number)
            .collect::<BTreeSet<u8>>()
            .into_iter()
            .collect();

        if seats.len() <= 1 {
            return self.split_equally(self.guest_count.clamp(1, 4));
        }

        let total_seats = seats.len() as u8;
        let shared_subtotal: i64 = self
            .items
            .iter()
            .filter(|i| !i.comped && i.seat_number.is_none())
            .map(OrderLineItem::line_subtotal_cents)
            .sum();
        let shared_per_seat = shared_subtotal / i64::from(total_seats);

        let full_sub = self.subtotal_cents().max(1);
        let remaining = self.remaining_balance_cents();
        let adj_total = self.adjustments_total_cents();
        let tax_total = self.tax_cents();

        let mut shares = Vec::with_capacity(seats.len());
        let mut allocated_total = 0i64;

        for (idx, seat_no) in seats.iter().enumerate() {
            let seat_items_sub: i64 = self
                .items
                .iter()
                .filter(|i| !i.comped && i.seat_number == Some(*seat_no))
                .map(OrderLineItem::line_subtotal_cents)
                .sum();
            let seat_sub = seat_items_sub + shared_per_seat;

            let seat_adj = (adj_total * seat_sub) / full_sub;
            let seat_tax = (tax_total * seat_sub) / full_sub;
            let seat_total = if idx + 1 == seats.len() {
                (remaining - allocated_total).max(0)
            } else {
                let proportional = (remaining * seat_sub) / full_sub;
                allocated_total += proportional;
                proportional
            };

            shares.push(SplitCheckShare {
                share_index: (idx as u8) + 1,
                total_shares: total_seats,
                label: format!("Seat {seat_no} Check"),
                subtotal_cents: seat_sub,
                adjustment_cents: seat_adj,
                tax_cents: seat_tax,
                total_cents: seat_total,
            });
        }

        shares
    }

    pub fn record_payment(
        &mut self,
        amount_cents: i64,
        tip_cents: i64,
        method: PaymentMethod,
        payer_label: impl Into<String>,
        token_override: Option<String>,
    ) -> PaymentRecord {
        let clamped_amount = amount_cents.clamp(1, self.remaining_balance_cents().max(1));
        let token_ref = token_override.unwrap_or_else(|| {
            format!(
                "P2PE-TOK-{}",
                &Uuid::new_v4().to_string().to_uppercase()[..8]
            )
        });
        let record = PaymentRecord {
            id: format!("pay-{}", &Uuid::new_v4().to_string()[..8]),
            amount_cents: clamped_amount,
            tip_cents: tip_cents.max(0),
            method,
            payer_label: payer_label.into(),
            token_reference: token_ref,
            paid_at: Utc::now(),
        };
        self.payments.push(record.clone());
        self.updated_at = Utc::now();

        if self.remaining_balance_cents() == 0 {
            self.status = OrderStatus::Paid;
        } else {
            self.status = OrderStatus::PartiallyPaid;
        }
        record
    }

    pub fn record_refund(
        &mut self,
        payment_id: &str,
        amount_cents: i64,
        reason: impl Into<String>,
        approved_by: impl Into<String>,
    ) -> Option<RefundRecord> {
        let max_refundable = self.net_paid_principal_cents();
        if max_refundable <= 0 || amount_cents <= 0 {
            return None;
        }
        if !self.payments.iter().any(|p| p.id == payment_id) {
            return None;
        }

        let refund_amount = amount_cents.min(max_refundable);
        let refund = RefundRecord {
            id: format!("rfnd-{}", &Uuid::new_v4().to_string()[..8]),
            payment_id: payment_id.to_string(),
            amount_cents: refund_amount,
            reason: reason.into(),
            approved_by: approved_by.into(),
            refunded_at: Utc::now(),
        };
        self.refunds.push(refund.clone());
        self.updated_at = Utc::now();

        if self.net_paid_principal_cents() == 0 {
            self.status = OrderStatus::Refunded;
        }
        Some(refund)
    }
}

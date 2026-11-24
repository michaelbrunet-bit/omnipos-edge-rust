use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use pos_core::{Order, PaymentMethod, PaymentRecord, RefundRecord};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum LedgerError {
    #[error("Unbalanced journal entry: debits ({debits}) != credits ({credits})")]
    UnbalancedEntry { debits: i64, credits: i64 },
    #[error("Cash drawer shift is already closed")]
    ShiftAlreadyClosed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountCode {
    // Assets
    CashInDrawer,
    CardProcessorReceivable,
    // Liabilities
    SalesTaxPayable,
    StaffTipsPayable,
    // Contra-Revenue / Expenses
    PromotionalDiscountsContra,
    ManagerCompsContra,
    SalesRefundsContra,
    PaidOutExpense,
    TillOverShortExpense,
    // Revenue & Equity
    GrossMerchandiseSales,
    ServiceChargeRevenue,
    TillFloatEquity,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LedgerPosting {
    pub account: AccountCode,
    pub debit_cents: i64,
    pub credit_cents: i64,
    pub memo: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalEntry {
    pub id: String,
    pub store_id: String,
    pub order_id: Option<String>,
    pub shift_id: Option<String>,
    pub description: String,
    pub postings: Vec<LedgerPosting>,
    pub created_at: DateTime<Utc>,
}

impl JournalEntry {
    pub fn new_checked(
        store_id: impl Into<String>,
        order_id: Option<String>,
        shift_id: Option<String>,
        description: impl Into<String>,
        postings: Vec<LedgerPosting>,
    ) -> Result<Self, LedgerError> {
        let debits: i64 = postings.iter().map(|p| p.debit_cents).sum();
        let credits: i64 = postings.iter().map(|p| p.credit_cents).sum();
        if debits != credits {
            return Err(LedgerError::UnbalancedEntry { debits, credits });
        }

        let cleaned: Vec<LedgerPosting> = postings
            .into_iter()
            .filter(|p| p.debit_cents > 0 || p.credit_cents > 0)
            .collect();

        Ok(Self {
            id: format!("jrnl-{}", &Uuid::new_v4().to_string()[..8]),
            store_id: store_id.into(),
            order_id,
            shift_id,
            description: description.into(),
            postings: cleaned,
            created_at: Utc::now(),
        })
    }

    /// Generates a balanced double-entry journal posting for a full or split payment on an Order.
    pub fn for_order_payment(
        order: &Order,
        payment: &PaymentRecord,
        shift_id: Option<String>,
    ) -> Result<Self, LedgerError> {
        let asset_account = match payment.method {
            PaymentMethod::Cash => AccountCode::CashInDrawer,
            PaymentMethod::CardTapP2PE | PaymentMethod::ApplePay | PaymentMethod::GooglePay => {
                AccountCode::CardProcessorReceivable
            }
        };

        let total_order = order.total_before_tip_cents().max(1);
        let ratio_num = payment.amount_cents;

        // Proportionally allocate tax, discounts, comps, and surcharges for partial/split payments;
        // if final payment closes the order, reconcile exact remaining cents.
        let is_closing_payment = order.remaining_balance_cents() == 0 && order.payments.len() == 1;

        let tax_share = if is_closing_payment {
            order.tax_cents()
        } else {
            (order.tax_cents() * ratio_num) / total_order
        };

        let discount_abs: i64 = order
            .mod_adjustments
            .iter()
            .filter(|a| a.amount_cents < 0)
            .map(|a| -a.amount_cents)
            .sum();
        let surcharge_pos: i64 = order
            .mod_adjustments
            .iter()
            .filter(|a| a.amount_cents > 0)
            .map(|a| a.amount_cents)
            .sum();

        let discount_share = if is_closing_payment {
            discount_abs
        } else {
            (discount_abs * ratio_num) / total_order
        };

        let surcharge_share = if is_closing_payment {
            surcharge_pos
        } else {
            (surcharge_pos * ratio_num) / total_order
        };

        let comp_share = if is_closing_payment {
            order.comped_total_cents()
        } else {
            (order.comped_total_cents() * ratio_num) / total_order
        };

        // Ensure Debits == Credits to the exact cent:
        // Debits = (principal + tip) + discount_share + comp_share
        // Credits = gross_sales_share + surcharge_share + tax_share + tip + comp_share
        // => gross_sales_share = principal + discount_share - surcharge_share - tax_share
        let gross_sales_credit =
            (payment.amount_cents + discount_share - surcharge_share - tax_share).max(0);
        // Reconcile rounding penny if gross_sales_credit clamped at 0
        let total_credits_without_sales = surcharge_share + tax_share + payment.tip_cents;
        let total_debits = payment.amount_cents + payment.tip_cents + discount_share + comp_share;
        let balanced_sales_credit =
            (total_debits - total_credits_without_sales).max(gross_sales_credit);

        let postings = vec![
            LedgerPosting {
                account: asset_account,
                debit_cents: payment.amount_cents + payment.tip_cents,
                credit_cents: 0,
                memo: format!("Tender {:?} [{}]", payment.method, payment.token_reference),
            },
            LedgerPosting {
                account: AccountCode::PromotionalDiscountsContra,
                debit_cents: discount_share,
                credit_cents: 0,
                memo: "Mod promotional discounts".to_string(),
            },
            LedgerPosting {
                account: AccountCode::ManagerCompsContra,
                debit_cents: comp_share,
                credit_cents: 0,
                memo: "Comped items contra-revenue".to_string(),
            },
            LedgerPosting {
                account: AccountCode::GrossMerchandiseSales,
                debit_cents: 0,
                credit_cents: balanced_sales_credit,
                memo: format!("Gross sales & comps ({})", order.table_or_tab_name),
            },
            LedgerPosting {
                account: AccountCode::ServiceChargeRevenue,
                debit_cents: 0,
                credit_cents: surcharge_share,
                memo: "Service charges".to_string(),
            },
            LedgerPosting {
                account: AccountCode::SalesTaxPayable,
                debit_cents: 0,
                credit_cents: tax_share,
                memo: "Collected sales tax".to_string(),
            },
            LedgerPosting {
                account: AccountCode::StaffTipsPayable,
                debit_cents: 0,
                credit_cents: payment.tip_cents,
                memo: format!("Gratuity payable to {}", order.assigned_operator),
            },
        ];

        Self::new_checked(
            &order.store_id,
            Some(order.id.clone()),
            shift_id,
            format!(
                "Payment {} on '{}'",
                payment.id, order.table_or_tab_name
            ),
            postings,
        )
    }

    pub fn for_refund(
        store_id: &str,
        order_id: &str,
        refund: &RefundRecord,
        method: PaymentMethod,
        shift_id: Option<String>,
    ) -> Result<Self, LedgerError> {
        let credit_asset = match method {
            PaymentMethod::Cash => AccountCode::CashInDrawer,
            _ => AccountCode::CardProcessorReceivable,
        };
        Self::new_checked(
            store_id,
            Some(order_id.to_string()),
            shift_id,
            format!("Refund {} ({})", refund.id, refund.reason),
            vec![
                LedgerPosting {
                    account: AccountCode::SalesRefundsContra,
                    debit_cents: refund.amount_cents,
                    credit_cents: 0,
                    memo: refund.reason.clone(),
                },
                LedgerPosting {
                    account: credit_asset,
                    debit_cents: 0,
                    credit_cents: refund.amount_cents,
                    memo: format!("Refunded by {}", refund.approved_by),
                },
            ],
        )
    }
}

pub fn summarize_trial_balance(entries: &[JournalEntry]) -> BTreeMap<AccountCode, (i64, i64)> {
    let mut map: BTreeMap<AccountCode, (i64, i64)> = BTreeMap::new();
    for entry in entries {
        for p in &entry.postings {
            let slot = map.entry(p.account).or_insert((0, 0));
            slot.0 += p.debit_cents;
            slot.1 += p.credit_cents;
        }
    }
    map
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CashMovementKind {
    PaidIn,
    PaidOut,
    SafeDrop,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CashMovement {
    pub id: String,
    pub kind: CashMovementKind,
    pub amount_cents: i64,
    pub reason: String,
    pub actor_name: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShiftStatus {
    Open,
    ClosedZReport,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CashDrawerShift {
    pub id: String,
    pub store_id: String,
    pub terminal_id: String,
    pub opened_by: String,
    pub opened_at: DateTime<Utc>,
    pub opening_float_cents: i64,
    pub cash_sales_cents: i64,
    pub cash_refunds_cents: i64,
    pub card_sales_cents: i64,
    pub tips_collected_cents: i64,
    pub movements: Vec<CashMovement>,
    pub status: ShiftStatus,
    pub closed_by: Option<String>,
    pub closed_at: Option<DateTime<Utc>>,
    pub counted_cash_cents: Option<i64>,
    pub over_short_cents: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShiftReport {
    pub report_type: String,
    pub shift_id: String,
    pub store_id: String,
    pub status: ShiftStatus,
    pub opening_float_cents: i64,
    pub cash_sales_cents: i64,
    pub cash_refunds_cents: i64,
    pub net_paid_in_out_cents: i64,
    pub expected_drawer_cash_cents: i64,
    pub counted_cash_cents: Option<i64>,
    pub over_short_cents: Option<i64>,
    pub card_sales_cents: i64,
    pub tips_collected_cents: i64,
    pub generated_at: DateTime<Utc>,
}

impl CashDrawerShift {
    pub fn open_new(
        store_id: impl Into<String>,
        terminal_id: impl Into<String>,
        opened_by: impl Into<String>,
        opening_float_cents: i64,
    ) -> Self {
        Self {
            id: format!("shift-{}", &Uuid::new_v4().to_string()[..8]),
            store_id: store_id.into(),
            terminal_id: terminal_id.into(),
            opened_by: opened_by.into(),
            opened_at: Utc::now(),
            opening_float_cents: opening_float_cents.max(0),
            cash_sales_cents: 0,
            cash_refunds_cents: 0,
            card_sales_cents: 0,
            tips_collected_cents: 0,
            movements: Vec::new(),
            status: ShiftStatus::Open,
            closed_by: None,
            closed_at: None,
            counted_cash_cents: None,
            over_short_cents: None,
        }
    }

    pub fn net_paid_in_out_cents(&self) -> i64 {
        self.movements
            .iter()
            .map(|m| match m.kind {
                CashMovementKind::PaidIn => m.amount_cents,
                CashMovementKind::PaidOut | CashMovementKind::SafeDrop => -m.amount_cents,
            })
            .sum()
    }

    pub fn expected_drawer_cash_cents(&self) -> i64 {
        self.opening_float_cents + self.cash_sales_cents - self.cash_refunds_cents
            + self.net_paid_in_out_cents()
    }

    pub fn record_movement(
        &mut self,
        kind: CashMovementKind,
        amount_cents: i64,
        reason: impl Into<String>,
        actor_name: impl Into<String>,
    ) -> Result<CashMovement, LedgerError> {
        if self.status != ShiftStatus::Open {
            return Err(LedgerError::ShiftAlreadyClosed);
        }
        let m = CashMovement {
            id: format!("mov-{}", &Uuid::new_v4().to_string()[..8]),
            kind,
            amount_cents: amount_cents.max(0),
            reason: reason.into(),
            actor_name: actor_name.into(),
            created_at: Utc::now(),
        };
        self.movements.push(m.clone());
        Ok(m)
    }

    pub fn x_report(&self) -> ShiftReport {
        ShiftReport {
            report_type: "X-REPORT (Mid-Shift Snapshot)".to_string(),
            shift_id: self.id.clone(),
            store_id: self.store_id.clone(),
            status: self.status,
            opening_float_cents: self.opening_float_cents,
            cash_sales_cents: self.cash_sales_cents,
            cash_refunds_cents: self.cash_refunds_cents,
            net_paid_in_out_cents: self.net_paid_in_out_cents(),
            expected_drawer_cash_cents: self.expected_drawer_cash_cents(),
            counted_cash_cents: self.counted_cash_cents,
            over_short_cents: self.over_short_cents,
            card_sales_cents: self.card_sales_cents,
            tips_collected_cents: self.tips_collected_cents,
            generated_at: Utc::now(),
        }
    }

    pub fn close_z_report(
        &mut self,
        counted_cash_cents: i64,
        closed_by: impl Into<String>,
    ) -> Result<ShiftReport, LedgerError> {
        if self.status != ShiftStatus::Open {
            return Err(LedgerError::ShiftAlreadyClosed);
        }
        let expected = self.expected_drawer_cash_cents();
        let over_short = counted_cash_cents - expected;
        self.status = ShiftStatus::ClosedZReport;
        self.counted_cash_cents = Some(counted_cash_cents);
        self.over_short_cents = Some(over_short);
        self.closed_by = Some(closed_by.into());
        self.closed_at = Some(Utc::now());

        let mut report = self.x_report();
        report.report_type = "Z-REPORT (End-of-Day Final Close)".to_string();
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pos_core::{ItemPrepStatus, ModAdjustment, OrderLineItem, OrderSource, TaxMode};

    #[test]
    fn test_double_entry_payment_and_z_report_balancing() {
        let mut order = Order::new_for_store(
            "store-hospitality",
            "ord-100",
            "Table 1",
            false,
            2,
            "Sofia Chen",
        );
        order.items.push(OrderLineItem {
            id: "l1".to_string(),
            catalog_item_id: "c1".to_string(),
            name: "Cocktail".to_string(),
            category: "cocktails".to_string(),
            quantity: 2,
            unit_price_cents: 1500,
            selected_modifiers: vec![],
            seat_number: Some(1),
            notes: None,
            added_by: "Sofia".to_string(),
            source: OrderSource::ServerTableside,
            prep_station: "bar".to_string(),
            prep_status: ItemPrepStatus::Served,
            tax_rate_bps: 825,
            tax_mode: TaxMode::Exclusive,
            comped: false,
            comp_reason: None,
            comp_approved_by: None,
        });
        order.mod_adjustments.push(ModAdjustment {
            mod_id: "hh".to_string(),
            mod_name: "Happy Hour".to_string(),
            label: "15% Off".to_string(),
            amount_cents: -450,
            receipt_note: None,
            target_categories: vec!["cocktails".to_string()],
            taxable_surcharge_bps: 0,
        });

        let balance = order.remaining_balance_cents();
        let payment = order.record_payment(balance, 500, PaymentMethod::Cash, "Guest", None);

        let mut shift = CashDrawerShift::open_new("store-hospitality", "reg-1", "Marcus", 20_000);
        shift.cash_sales_cents += payment.amount_cents;

        let journal =
            JournalEntry::for_order_payment(&order, &payment, Some(shift.id.clone())).unwrap();
        let debits: i64 = journal.postings.iter().map(|p| p.debit_cents).sum();
        let credits: i64 = journal.postings.iter().map(|p| p.credit_cents).sum();
        assert_eq!(debits, credits);

        let _ = shift
            .record_movement(CashMovementKind::PaidOut, 1_500, "Fresh lemons", "Marcus")
            .unwrap();
        let expected_cash = 20_000 + payment.amount_cents - 1_500;
        let z = shift.close_z_report(expected_cash, "Elena Vance").unwrap();
        assert_eq!(z.over_short_cents, Some(0));
    }
}

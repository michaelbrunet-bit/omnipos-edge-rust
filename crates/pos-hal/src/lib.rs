use chrono::{DateTime, Utc};
use pos_core::{Order, PaymentMethod, StoreTheme};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum HalError {
    #[error("Payment intent '{0}' not found on P2PE terminal")]
    IntentNotFound(String),
    #[error("Invalid terminal state transition: {0}")]
    InvalidIntentState(String),
    #[error("Hardware device I/O error: {0}")]
    DeviceError(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaymentIntentStatus {
    Authorized,
    Captured,
    Voided,
    Refunded,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentIntent {
    pub id: String,
    pub order_id: String,
    pub method: PaymentMethod,
    pub card_label: String,
    pub p2pe_token: String,
    pub authorized_cents: i64,
    pub captured_cents: i64,
    pub tip_cents: i64,
    pub refunded_cents: i64,
    pub status: PaymentIntentStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub trait PaymentTerminal: Send + Sync {
    fn preauth(
        &mut self,
        order_id: &str,
        card_label: &str,
        hold_cents: i64,
    ) -> Result<PaymentIntent, HalError>;

    fn incremental_auth(
        &mut self,
        intent_id: &str,
        additional_cents: i64,
    ) -> Result<PaymentIntent, HalError>;

    fn capture(
        &mut self,
        intent_id: &str,
        principal_cents: i64,
        tip_cents: i64,
    ) -> Result<PaymentIntent, HalError>;

    fn sale(
        &mut self,
        order_id: &str,
        method: PaymentMethod,
        card_label: &str,
        principal_cents: i64,
        tip_cents: i64,
    ) -> Result<PaymentIntent, HalError>;

    fn void_auth(&mut self, intent_id: &str) -> Result<PaymentIntent, HalError>;

    fn refund(&mut self, intent_id: &str, amount_cents: i64) -> Result<PaymentIntent, HalError>;

    fn list_intents(&self) -> &[PaymentIntent];
}

/// PCI P2PE-isolated payment terminal simulator (EMV contactless / NFC / Pre-Auth tabs).
/// Never exposes raw track data or PANs to host POS memory—only opaque `P2PE-TOK-*` tokens.
#[derive(Debug, Clone, Default)]
pub struct VirtualP2peTerminal {
    pub terminal_serial: String,
    intents: Vec<PaymentIntent>,
}

impl VirtualP2peTerminal {
    pub fn new(serial: impl Into<String>) -> Self {
        Self {
            terminal_serial: serial.into(),
            intents: Vec::new(),
        }
    }

    pub fn find_open_auth_for_order(&self, order_id: &str) -> Option<PaymentIntent> {
        self.intents
            .iter()
            .rev()
            .find(|i| i.order_id == order_id && i.status == PaymentIntentStatus::Authorized)
            .cloned()
    }

    pub fn find_by_token(&self, token_ref: &str) -> Option<PaymentIntent> {
        self.intents
            .iter()
            .rev()
            .find(|i| i.p2pe_token == token_ref)
            .cloned()
    }
}

impl PaymentTerminal for VirtualP2peTerminal {
    fn preauth(
        &mut self,
        order_id: &str,
        card_label: &str,
        hold_cents: i64,
    ) -> Result<PaymentIntent, HalError> {
        if hold_cents <= 0 {
            return Err(HalError::InvalidIntentState(
                "preauth hold must be positive".to_string(),
            ));
        }
        let now = Utc::now();
        let intent = PaymentIntent {
            id: format!("pi-{}", &Uuid::new_v4().to_string()[..8]),
            order_id: order_id.to_string(),
            method: PaymentMethod::CardTapP2PE,
            card_label: card_label.to_string(),
            p2pe_token: format!(
                "P2PE-AUTH-{}",
                &Uuid::new_v4().to_string().to_uppercase()[..8]
            ),
            authorized_cents: hold_cents,
            captured_cents: 0,
            tip_cents: 0,
            refunded_cents: 0,
            status: PaymentIntentStatus::Authorized,
            created_at: now,
            updated_at: now,
        };
        self.intents.push(intent.clone());
        Ok(intent)
    }

    fn incremental_auth(
        &mut self,
        intent_id: &str,
        additional_cents: i64,
    ) -> Result<PaymentIntent, HalError> {
        let intent = self
            .intents
            .iter_mut()
            .find(|i| i.id == intent_id)
            .ok_or_else(|| HalError::IntentNotFound(intent_id.to_string()))?;
        if intent.status != PaymentIntentStatus::Authorized {
            return Err(HalError::InvalidIntentState(format!(
                "cannot increment intent in {:?} state",
                intent.status
            )));
        }
        intent.authorized_cents += additional_cents.max(0);
        intent.updated_at = Utc::now();
        Ok(intent.clone())
    }

    fn capture(
        &mut self,
        intent_id: &str,
        principal_cents: i64,
        tip_cents: i64,
    ) -> Result<PaymentIntent, HalError> {
        let intent = self
            .intents
            .iter_mut()
            .find(|i| i.id == intent_id)
            .ok_or_else(|| HalError::IntentNotFound(intent_id.to_string()))?;
        if intent.status != PaymentIntentStatus::Authorized {
            return Err(HalError::InvalidIntentState(format!(
                "cannot capture intent in {:?} state",
                intent.status
            )));
        }
        let total_needed = principal_cents.max(0) + tip_cents.max(0);
        if total_needed > intent.authorized_cents {
            intent.authorized_cents = total_needed;
        }
        intent.captured_cents = principal_cents.max(0);
        intent.tip_cents = tip_cents.max(0);
        intent.status = PaymentIntentStatus::Captured;
        intent.updated_at = Utc::now();
        Ok(intent.clone())
    }

    fn sale(
        &mut self,
        order_id: &str,
        method: PaymentMethod,
        card_label: &str,
        principal_cents: i64,
        tip_cents: i64,
    ) -> Result<PaymentIntent, HalError> {
        let now = Utc::now();
        let prefix = match method {
            PaymentMethod::Cash => "CASH-TILL",
            PaymentMethod::ApplePay => "APAY-TOK",
            PaymentMethod::GooglePay => "GPAY-TOK",
            PaymentMethod::CardTapP2PE => "P2PE-TOK",
        };
        let intent = PaymentIntent {
            id: format!("pi-{}", &Uuid::new_v4().to_string()[..8]),
            order_id: order_id.to_string(),
            method,
            card_label: card_label.to_string(),
            p2pe_token: format!("{prefix}-{}", &Uuid::new_v4().to_string().to_uppercase()[..8]),
            authorized_cents: principal_cents.max(0) + tip_cents.max(0),
            captured_cents: principal_cents.max(0),
            tip_cents: tip_cents.max(0),
            refunded_cents: 0,
            status: PaymentIntentStatus::Captured,
            created_at: now,
            updated_at: now,
        };
        self.intents.push(intent.clone());
        Ok(intent)
    }

    fn void_auth(&mut self, intent_id: &str) -> Result<PaymentIntent, HalError> {
        let intent = self
            .intents
            .iter_mut()
            .find(|i| i.id == intent_id)
            .ok_or_else(|| HalError::IntentNotFound(intent_id.to_string()))?;
        if intent.status != PaymentIntentStatus::Authorized {
            return Err(HalError::InvalidIntentState(
                "only Authorized intents can be voided; Captured intents require refund".to_string(),
            ));
        }
        intent.status = PaymentIntentStatus::Voided;
        intent.updated_at = Utc::now();
        Ok(intent.clone())
    }

    fn refund(&mut self, intent_id: &str, amount_cents: i64) -> Result<PaymentIntent, HalError> {
        let intent = self
            .intents
            .iter_mut()
            .find(|i| i.id == intent_id)
            .ok_or_else(|| HalError::IntentNotFound(intent_id.to_string()))?;
        if intent.status != PaymentIntentStatus::Captured
            && intent.status != PaymentIntentStatus::Refunded
        {
            return Err(HalError::InvalidIntentState(
                "only Captured intents can be refunded".to_string(),
            ));
        }
        let max_avail = (intent.captured_cents - intent.refunded_cents).max(0);
        let applied = amount_cents.clamp(0, max_avail);
        intent.refunded_cents += applied;
        if intent.refunded_cents >= intent.captured_cents {
            intent.status = PaymentIntentStatus::Refunded;
        }
        intent.updated_at = Utc::now();
        Ok(intent.clone())
    }

    fn list_intents(&self) -> &[PaymentIntent] {
        &self.intents
    }
}

/// Rendered ESC/POS thermal receipt + RJ-11 cash drawer pulse record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderedReceipt {
    pub id: String,
    pub order_id: String,
    pub store_name: String,
    pub drawer_kicked: bool,
    pub plain_text_preview: String,
    pub escpos_hex: String,
    pub byte_length: usize,
    pub printed_at: DateTime<Utc>,
}

pub trait ReceiptPrinter: Send + Sync {
    fn print_order_receipt(
        &mut self,
        theme: &StoreTheme,
        order: &Order,
        kick_drawer: bool,
    ) -> Result<RenderedReceipt, HalError>;

    fn kick_cash_drawer(&mut self, reason: &str) -> Result<RenderedReceipt, HalError>;

    fn recent_jobs(&self) -> &[RenderedReceipt];
}

#[derive(Debug, Clone, Default)]
pub struct VirtualEscPosPrinter {
    pub printer_model: String,
    pub drawer_kick_count: u64,
    jobs: Vec<RenderedReceipt>,
}

impl VirtualEscPosPrinter {
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            printer_model: model.into(),
            drawer_kick_count: 0,
            jobs: Vec::new(),
        }
    }

    fn format_cents(symbol: &str, cents: i64) -> String {
        if cents < 0 {
            format!("-{}{:.2}", symbol, (-cents) as f64 / 100.0)
        } else {
            format!("{}{:.2}", symbol, cents as f64 / 100.0)
        }
    }

    fn two_col(left: &str, right: &str, width: usize) -> String {
        let max_left = width.saturating_sub(right.chars().count() + 1);
        let left_trunc: String = left.chars().take(max_left).collect();
        let pad = width
            .saturating_sub(left_trunc.chars().count() + right.chars().count())
            .max(1);
        format!("{}{}{}", left_trunc, " ".repeat(pad), right)
    }
}

impl ReceiptPrinter for VirtualEscPosPrinter {
    fn print_order_receipt(
        &mut self,
        theme: &StoreTheme,
        order: &Order,
        kick_drawer: bool,
    ) -> Result<RenderedReceipt, HalError> {
        const WIDTH: usize = 42;
        let sym = &theme.currency_symbol;
        let totals = order.totals();

        let mut esc: Vec<u8> = Vec::with_capacity(1024);
        let mut preview_lines: Vec<String> = Vec::new();

        // ESC @ (Initialize printer)
        esc.extend_from_slice(&[0x1B, 0x40]);

        // Center + Bold + Double Size for Store Header
        esc.extend_from_slice(&[0x1B, 0x61, 0x01, 0x1B, 0x45, 0x01, 0x1D, 0x21, 0x11]);
        esc.extend_from_slice(theme.store_name.as_bytes());
        esc.push(b'\n');
        preview_lines.push(format!("=== {} ===", theme.store_name.to_uppercase()));

        // Normal font, centered receipt header
        esc.extend_from_slice(&[0x1D, 0x21, 0x00, 0x1B, 0x45, 0x00]);
        esc.extend_from_slice(theme.receipt_header.as_bytes());
        esc.push(b'\n');
        preview_lines.push(theme.receipt_header.clone());
        preview_lines.push("-".repeat(WIDTH));

        // Left align
        esc.extend_from_slice(&[0x1B, 0x61, 0x00]);
        let meta1 = Self::two_col(
            &format!("Ticket: {}", order.table_or_tab_name),
            &format!("Guests: {}", order.guest_count),
            WIDTH,
        );
        let meta2 = Self::two_col(
            &format!("Server: {}", order.assigned_operator),
            &order.id,
            WIDTH,
        );
        for line in [&meta1, &meta2] {
            esc.extend_from_slice(line.as_bytes());
            esc.push(b'\n');
            preview_lines.push(line.clone());
        }
        preview_lines.push("-".repeat(WIDTH));

        for item in &order.items {
            let price_str = if item.comped {
                "COMP".to_string()
            } else {
                Self::format_cents(sym, item.line_subtotal_cents())
            };
            let row = Self::two_col(
                &format!("{}x {}", item.quantity, item.name),
                &price_str,
                WIDTH,
            );
            esc.extend_from_slice(row.as_bytes());
            esc.push(b'\n');
            preview_lines.push(row);

            for m in &item.selected_modifiers {
                let mod_line = format!("   + {:?} {}", m.qualifier, m.name);
                esc.extend_from_slice(mod_line.as_bytes());
                esc.push(b'\n');
                preview_lines.push(mod_line);
            }
            if item.comped {
                if let Some(reason) = &item.comp_reason {
                    let comp_note = format!("   * COMPED ({reason})");
                    esc.extend_from_slice(comp_note.as_bytes());
                    esc.push(b'\n');
                    preview_lines.push(comp_note);
                }
            }
        }

        preview_lines.push("-".repeat(WIDTH));
        let sub_row = Self::two_col(
            "Subtotal",
            &Self::format_cents(sym, totals.subtotal_cents),
            WIDTH,
        );
        esc.extend_from_slice(sub_row.as_bytes());
        esc.push(b'\n');
        preview_lines.push(sub_row);

        for adj in &order.mod_adjustments {
            let adj_row = Self::two_col(
                &adj.label,
                &Self::format_cents(sym, adj.amount_cents),
                WIDTH,
            );
            esc.extend_from_slice(adj_row.as_bytes());
            esc.push(b'\n');
            preview_lines.push(adj_row);
        }

        let tax_row = Self::two_col("Tax", &Self::format_cents(sym, totals.tax_cents), WIDTH);
        esc.extend_from_slice(tax_row.as_bytes());
        esc.push(b'\n');
        preview_lines.push(tax_row);

        // Bold Total
        esc.extend_from_slice(&[0x1B, 0x45, 0x01]);
        let total_row = Self::two_col(
            "TOTAL",
            &Self::format_cents(sym, totals.total_before_tip_cents),
            WIDTH,
        );
        esc.extend_from_slice(total_row.as_bytes());
        esc.push(b'\n');
        esc.extend_from_slice(&[0x1B, 0x45, 0x00]);
        preview_lines.push(total_row);

        if totals.tips_total_cents > 0 {
            let tip_row = Self::two_col(
                "Tips",
                &Self::format_cents(sym, totals.tips_total_cents),
                WIDTH,
            );
            esc.extend_from_slice(tip_row.as_bytes());
            esc.push(b'\n');
            preview_lines.push(tip_row);
        }

        if !order.receipt_notes.is_empty() {
            preview_lines.push("-".repeat(WIDTH));
            for note in &order.receipt_notes {
                esc.extend_from_slice(note.as_bytes());
                esc.push(b'\n');
                preview_lines.push(note.clone());
            }
        }

        // Centered footer
        esc.extend_from_slice(&[0x1B, 0x61, 0x01]);
        esc.extend_from_slice(theme.receipt_footer.as_bytes());
        esc.push(b'\n');
        preview_lines.push("-".repeat(WIDTH));
        preview_lines.push(theme.receipt_footer.clone());

        // Optional RJ-11 cash drawer pulse (ESC p 0 25 250)
        if kick_drawer {
            esc.extend_from_slice(&[0x1B, 0x70, 0x00, 0x19, 0xFA]);
            self.drawer_kick_count += 1;
            preview_lines.push("[RJ-11 CASH DRAWER PULSE TRIGGERED]".to_string());
        }

        // Partial paper cut (GS V 66 0)
        esc.extend_from_slice(&[0x1D, 0x56, 0x42, 0x00]);

        let rendered = RenderedReceipt {
            id: format!("rcpt-{}", &Uuid::new_v4().to_string()[..8]),
            order_id: order.id.clone(),
            store_name: theme.store_name.clone(),
            drawer_kicked: kick_drawer,
            plain_text_preview: preview_lines.join("\n"),
            escpos_hex: hex::encode(&esc),
            byte_length: esc.len(),
            printed_at: Utc::now(),
        };
        self.jobs.push(rendered.clone());
        Ok(rendered)
    }

    fn kick_cash_drawer(&mut self, reason: &str) -> Result<RenderedReceipt, HalError> {
        let esc = vec![0x1B, 0x40, 0x1B, 0x70, 0x00, 0x19, 0xFA];
        self.drawer_kick_count += 1;
        let rendered = RenderedReceipt {
            id: format!("kick-{}", &Uuid::new_v4().to_string()[..8]),
            order_id: "drawer-pulse".to_string(),
            store_name: "Cash Drawer Solenoid".to_string(),
            drawer_kicked: true,
            plain_text_preview: format!("[NO-SALE / SHIFT DRAWER KICK] Reason: {reason}"),
            escpos_hex: hex::encode(&esc),
            byte_length: esc.len(),
            printed_at: Utc::now(),
        };
        self.jobs.push(rendered.clone());
        Ok(rendered)
    }

    fn recent_jobs(&self) -> &[RenderedReceipt] {
        &self.jobs
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HalStatusSnapshot {
    pub terminal_serial: String,
    pub printer_model: String,
    pub drawer_kick_count: u64,
    pub payment_intents: Vec<PaymentIntent>,
    pub recent_receipts: Vec<RenderedReceipt>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_p2pe_preauth_incremental_capture_and_refund() {
        let mut term = VirtualP2peTerminal::new("VERIFONE-P400-SIM");
        let auth = term
            .preauth("ord-bar-1", "Visa ••4821", 5_000)
            .expect("preauth succeeds");
        assert_eq!(auth.status, PaymentIntentStatus::Authorized);
        assert!(auth.p2pe_token.starts_with("P2PE-AUTH-"));

        let inc = term
            .incremental_auth(&auth.id, 2_500)
            .expect("incremental auth");
        assert_eq!(inc.authorized_cents, 7_500);

        let cap = term.capture(&auth.id, 6_200, 1_200).expect("capture");
        assert_eq!(cap.status, PaymentIntentStatus::Captured);
        assert_eq!(cap.captured_cents, 6_200);
        assert_eq!(cap.tip_cents, 1_200);

        let refunded = term.refund(&auth.id, 6_200).expect("refund");
        assert_eq!(refunded.status, PaymentIntentStatus::Refunded);
    }

    #[test]
    fn test_escpos_receipt_binary_builder_and_drawer_kick() {
        let mut printer = VirtualEscPosPrinter::new("EPSON-TM-T88VII-SIM");
        let theme = StoreTheme::preset_neon_craft_bar();
        let order = Order::new("ord-test", "Table 12", false, 2, "Sofia Chen");

        let receipt = printer
            .print_order_receipt(&theme, &order, true)
            .expect("receipt printed");
        assert!(receipt.drawer_kicked);
        assert_eq!(printer.drawer_kick_count, 1);
        // Starts with ESC @ (1b40) and contains drawer kick (1b700019fa) and GS V cut (1d564200)
        assert!(receipt.escpos_hex.starts_with("1b40"));
        assert!(receipt.escpos_hex.contains("1b700019fa"));
        assert!(receipt.escpos_hex.ends_with("1d564200"));
    }
}

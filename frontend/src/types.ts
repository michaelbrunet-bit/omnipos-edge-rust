export type Role = "owner" | "manager" | "bartender" | "server" | "customer";

export type IndustryVertical = "hospitality" | "salon_barber" | "retail" | "custom";
export type ColorScheme = "dark" | "light";
export type BorderRadiusStyle = "sharp" | "rounded" | "pill";
export type UiDensity = "compact" | "comfortable" | "spacious";
export type TaxMode = "exclusive" | "inclusive" | "exempt";

export interface StoreTheme {
  preset_id: string;
  vertical: IndustryVertical;
  store_name: string;
  tagline: string;
  logo_text: string;
  currency_symbol: string;
  color_scheme: ColorScheme;
  primary_color: string;
  secondary_color: string;
  accent_color: string;
  background_color: string;
  surface_color: string;
  text_color: string;
  muted_text_color: string;
  font_family: string;
  border_radius: BorderRadiusStyle;
  ui_density: UiDensity;
  receipt_header: string;
  receipt_footer: string;
  custom_css: string | null;
}

export interface CategoryConfig {
  id: string;
  label: string;
  icon_emoji: string;
  sort_order: number;
  default_tax_rule_id: string;
}

export interface PrepStationConfig {
  id: string;
  label: string;
  is_kds_routed: boolean;
}

export interface TaxRule {
  id: string;
  name: string;
  rate_bps: number;
  mode: TaxMode;
}

export type ModifierQualifier =
  | "regular"
  | "no"
  | "light"
  | "extra"
  | "on_side";

export interface ModifierOption {
  id: string;
  name: string;
  price_delta_cents: number;
  qualifier?: ModifierQualifier;
}

export interface CatalogItem {
  id: string;
  store_id?: string;
  sku: string;
  vertical: IndustryVertical;
  name: string;
  description: string;
  category: string;
  price_cents: number;
  tax_rate_bps: number;
  tax_mode?: TaxMode;
  prep_station: string;
  available: boolean;
  stock_count: number | null;
  duration_minutes: number | null;
  barcode_upc: string | null;
  badge_emoji: string;
  tags: string[];
  modifiers: ModifierOption[];
  attributes?: Record<string, string>;
}

export type OrderSource =
  | "bartender_speed_rail"
  | "server_tableside"
  | "customer_self_service";

export type OrderStatus =
  | "open"
  | "sent_to_prep"
  | "check_requested"
  | "partially_paid"
  | "paid"
  | "voided"
  | "refunded";

export type ItemPrepStatus = "held" | "fired" | "ready" | "served";

export interface OrderLineItem {
  id: string;
  catalog_item_id: string;
  name: string;
  category: string;
  quantity: number;
  unit_price_cents: number;
  selected_modifiers: ModifierOption[];
  seat_number: number | null;
  notes: string | null;
  added_by: string;
  source: OrderSource;
  prep_station: string;
  prep_status: ItemPrepStatus;
  tax_rate_bps: number;
  tax_mode?: TaxMode;
  comped?: boolean;
  comp_reason?: string | null;
  comp_approved_by?: string | null;
}

export interface ModAdjustment {
  mod_id: string;
  mod_name: string;
  label: string;
  amount_cents: number;
  receipt_note: string | null;
  target_categories?: string[];
}

export type PaymentMethod =
  | "card_tap_p2_p_e"
  | "apple_pay"
  | "google_pay"
  | "cash";

export interface PaymentRecord {
  id: string;
  amount_cents: number;
  tip_cents: number;
  method: PaymentMethod;
  payer_label: string;
  token_reference: string;
  paid_at: string;
}

export interface RefundRecord {
  id: string;
  payment_id: string;
  amount_cents: number;
  reason: string;
  approved_by: string;
  refunded_at: string;
}

export interface SplitCheckShare {
  share_index: number;
  total_shares: number;
  label: string;
  subtotal_cents: number;
  adjustment_cents: number;
  tax_cents: number;
  total_cents: number;
}

export interface OrderSummaryTotals {
  gross_merchandise_cents: number;
  comped_total_cents: number;
  subtotal_cents: number;
  adjustments_total_cents: number;
  taxable_amount_cents: number;
  tax_cents: number;
  embedded_vat_cents: number;
  total_before_tip_cents: number;
  paid_principal_cents: number;
  refunded_cents: number;
  tips_total_cents: number;
  remaining_balance_cents: number;
}

export interface OrderView {
  id: string;
  store_id: string;
  table_or_tab_name: string;
  is_bar_tab: boolean;
  guest_count: number;
  assigned_operator: string;
  preauth_card: string | null;
  client_profile_notes: string | null;
  status: OrderStatus;
  items: OrderLineItem[];
  mod_adjustments: ModAdjustment[];
  payments: PaymentRecord[];
  refunds: RefundRecord[];
  receipt_notes: string[];
  created_at: string;
  updated_at: string;
  totals: OrderSummaryTotals;
  split_two_ways: SplitCheckShare[];
  split_four_ways: SplitCheckShare[];
  split_by_seat: SplitCheckShare[];
}

export interface ModManifest {
  id: string;
  name: string;
  version: string;
  author: string;
  description: string;
  runtime: string;
  wasm_sha256: string;
  publisher_pubkey_hex: string;
  ed25519_signature_hex: string;
  capabilities: string[];
  fuel_limit: number;
  memory_limit_kb: number;
  enabled: boolean;
  last_fuel_used: number;
  invocations_count: number;
}

export interface AuditVerificationReport {
  valid: boolean;
  total_entries: number;
  head_hash: string;
  broken_at_sequence: number | null;
}

export interface AuditLogEntry {
  sequence: number;
  timestamp: string;
  store_id: string;
  actor_name: string;
  role: Role;
  action: string;
  details: string;
  prev_hash: string;
  entry_hash: string;
}

export interface OperatorPublicView {
  id: string;
  name: string;
  role: Role;
  demo_pin_hint: string;
}

export interface StoreSummaryView {
  store_id: string;
  vertical: IndustryVertical;
  store_name: string;
  active: boolean;
}

export type CashMovementKind = "paid_in" | "paid_out" | "safe_drop";

export interface CashMovement {
  id: string;
  kind: CashMovementKind;
  amount_cents: number;
  reason: string;
  actor_name: string;
  created_at: string;
}

export interface CashDrawerShift {
  id: string;
  store_id: string;
  terminal_id: string;
  opened_by: string;
  opened_at: string;
  opening_float_cents: number;
  cash_sales_cents: number;
  cash_refunds_cents: number;
  card_sales_cents: number;
  tips_collected_cents: number;
  movements: CashMovement[];
  status: "open" | "closed_z_report";
  closed_by: string | null;
  closed_at: string | null;
  counted_cash_cents: number | null;
  over_short_cents: number | null;
}

export interface ShiftReport {
  report_type: string;
  shift_id: string;
  store_id: string;
  status: "open" | "closed_z_report";
  opening_float_cents: number;
  cash_sales_cents: number;
  cash_refunds_cents: number;
  net_paid_in_out_cents: number;
  expected_drawer_cash_cents: number;
  counted_cash_cents: number | null;
  over_short_cents: number | null;
  card_sales_cents: number;
  tips_collected_cents: number;
  generated_at: string;
}

export interface ShiftsSummaryResponse {
  current_shift: CashDrawerShift | null;
  x_report: ShiftReport | null;
  history: CashDrawerShift[];
}

export interface TrialBalanceRow {
  account: string;
  debit_cents: number;
  credit_cents: number;
  net_cents: number;
}

export interface LedgerPosting {
  account: string;
  debit_cents: number;
  credit_cents: number;
  memo: string;
}

export interface JournalEntry {
  id: string;
  store_id: string;
  order_id: string | null;
  shift_id: string | null;
  description: string;
  postings: LedgerPosting[];
  created_at: string;
}

export interface LedgerSummaryResponse {
  store_id: string;
  is_balanced: boolean;
  total_debits_cents: number;
  total_credits_cents: number;
  trial_balance: TrialBalanceRow[];
  entries: JournalEntry[];
}

export interface RenderedReceipt {
  id: string;
  order_id: string;
  store_name: string;
  drawer_kicked: boolean;
  plain_text_preview: string;
  escpos_hex: string;
  byte_length: number;
  printed_at: string;
}

export interface PaymentIntent {
  id: string;
  order_id: string;
  method: PaymentMethod;
  card_label: string;
  p2pe_token: string;
  authorized_cents: number;
  captured_cents: number;
  tip_cents: number;
  refunded_cents: number;
  status: "authorized" | "captured" | "voided" | "refunded";
  created_at: string;
  updated_at: string;
}

export interface HalStatusSnapshot {
  terminal_serial: string;
  printer_model: string;
  drawer_kick_count: number;
  payment_intents: PaymentIntent[];
  recent_receipts: RenderedReceipt[];
}

export interface SessionInfo {
  token: string;
  actor_id: string;
  actor_name: string;
  role: Role;
  store_id: string;
  scoped_order_id: string | null;
  exp_unix: number;
}

export interface BootstrapResponse {
  active_store_id: string;
  stores: StoreSummaryView[];
  theme: StoreTheme;
  categories: CategoryConfig[];
  prep_stations: PrepStationConfig[];
  tax_rules: TaxRule[];
  theme_presets: StoreTheme[];
  catalog: CatalogItem[];
  orders: OrderView[];
  mods: ModManifest[];
  current_shift: CashDrawerShift | null;
  audit_report: AuditVerificationReport;
  recent_audit: AuditLogEntry[];
  operators: OperatorPublicView[];
  supported_platforms: string[];
}

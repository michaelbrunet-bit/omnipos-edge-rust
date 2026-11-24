pub mod auth_guard;

use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, Query, State,
    },
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get, patch, post},
    Json, Router,
};
use chrono::{Duration, Utc};
use pos_core::{
    CatalogItem, CategoryConfig, IndustryVertical, ItemPrepStatus, ManagerOverrideClaims,
    ModifierOption, ModifierQualifier, Order, OrderLineItem, OrderSource, OrderStatus,
    OrderSummaryTotals, PaymentMethod, Permission, PrepStationConfig, Role, SessionClaims,
    SplitCheckShare, StoreProfile, StoreTheme, TaxRule, TokenSigner,
};
use pos_hal::{
    HalStatusSnapshot, PaymentTerminal, ReceiptPrinter, RenderedReceipt, VirtualEscPosPrinter,
    VirtualP2peTerminal,
};
use pos_ledger::{
    summarize_trial_balance, AccountCode, CashDrawerShift, CashMovementKind, JournalEntry,
    LedgerPosting, ShiftReport,
};
use pos_mod_engine::ModManifest;
use pos_storage::{AuditLogEntry, AuditVerificationReport, PosStorage, SyncEvent};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, RwLock};
use tower_http::{
    cors::{Any, CorsLayer},
    services::{ServeDir, ServeFile},
};
use uuid::Uuid;

use crate::auth_guard::{
    api_err, auth_err_to_api, ApiError, AuthenticatedSession, OptionalSession,
    require_manager_approval, require_order_access, require_permission,
};

#[derive(Clone)]
pub struct AppState {
    pub storage: Arc<RwLock<PosStorage>>,
    pub signer: Arc<TokenSigner>,
    pub terminal: Arc<RwLock<VirtualP2peTerminal>>,
    pub printer: Arc<RwLock<VirtualEscPosPrinter>>,
    pub events_tx: broadcast::Sender<String>,
}

impl AppState {
    pub fn new(storage: PosStorage) -> Self {
        let secret = std::env::var("POS_HMAC_SECRET")
            .ok()
            .filter(|s| s.len() >= 16)
            .map(|s| s.into_bytes())
            .unwrap_or_else(|| storage.persisted_hmac_secret());

        let (events_tx, _) = broadcast::channel(256);
        Self {
            storage: Arc::new(RwLock::new(storage)),
            signer: Arc::new(TokenSigner::new(secret)),
            terminal: Arc::new(RwLock::new(VirtualP2peTerminal::new("PAX-A920-P2PE-01"))),
            printer: Arc::new(RwLock::new(VirtualEscPosPrinter::new("EPSON-TM-T88VII-LAN"))),
            events_tx,
        }
    }

    pub fn broadcast_sync_event(&self, ev: &SyncEvent) {
        let payload = serde_json::json!({
            "seq": ev.seq,
            "store_id": ev.store_id,
            "event": ev.event_type,
            "entity_id": ev.entity_id,
            "timestamp": ev.created_at.to_rfc3339(),
        })
        .to_string();
        let _ = self.events_tx.send(payload);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderView {
    #[serde(flatten)]
    pub order: Order,
    pub totals: OrderSummaryTotals,
    pub split_two_ways: Vec<SplitCheckShare>,
    pub split_four_ways: Vec<SplitCheckShare>,
    pub split_by_seat: Vec<SplitCheckShare>,
}

impl From<&Order> for OrderView {
    fn from(order: &Order) -> Self {
        Self {
            order: order.clone(),
            totals: order.totals(),
            split_two_ways: order.split_equally(2),
            split_four_ways: order.split_equally(4),
            split_by_seat: order.split_by_seat(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct OperatorPublicView {
    pub id: String,
    pub name: String,
    pub role: Role,
    pub demo_pin_hint: &'static str,
}

#[derive(Debug, Serialize)]
pub struct StoreSummaryView {
    pub store_id: String,
    pub vertical: IndustryVertical,
    pub store_name: String,
    pub active: bool,
}

#[derive(Debug, Serialize)]
pub struct BootstrapResponse {
    pub active_store_id: String,
    pub stores: Vec<StoreSummaryView>,
    pub theme: StoreTheme,
    pub categories: Vec<CategoryConfig>,
    pub prep_stations: Vec<PrepStationConfig>,
    pub tax_rules: Vec<TaxRule>,
    pub theme_presets: Vec<StoreTheme>,
    pub catalog: Vec<CatalogItem>,
    pub orders: Vec<OrderView>,
    pub mods: Vec<ModManifest>,
    pub current_shift: Option<CashDrawerShift>,
    pub audit_report: AuditVerificationReport,
    pub recent_audit: Vec<AuditLogEntry>,
    pub operators: Vec<OperatorPublicView>,
    pub supported_platforms: Vec<&'static str>,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub role: Role,
    pub pin: Option<String>,
    pub scoped_order_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub actor_id: String,
    pub actor_name: String,
    pub role: Role,
    pub store_id: String,
    pub scoped_order_id: Option<String>,
    pub exp_unix: i64,
}

#[derive(Debug, Deserialize)]
pub struct ManagerOverrideRequest {
    pub pin: String,
    pub action: String,
}

#[derive(Debug, Serialize)]
pub struct ManagerOverrideResponse {
    pub override_token: String,
    pub manager_name: String,
    pub approved_action: String,
    pub exp_unix: i64,
}

#[derive(Debug, Serialize)]
pub struct SignedQrLinkResponse {
    pub order_id: String,
    pub token: String,
    pub qr_url_path: String,
    pub exp_unix: i64,
}

#[derive(Debug, Deserialize)]
pub struct CreateOrderRequest {
    pub table_or_tab_name: String,
    pub is_bar_tab: bool,
    pub guest_count: u8,
    pub assigned_operator: String,
    pub preauth_card: Option<String>,
    pub client_profile_notes: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AddOrderItemRequest {
    pub catalog_item_id: String,
    pub quantity: u32,
    pub modifier_ids: Vec<String>,
    #[serde(default)]
    pub modifier_qualifier: ModifierQualifier,
    pub seat_number: Option<u8>,
    pub notes: Option<String>,
    pub added_by: String,
    pub source: OrderSource,
}

#[derive(Debug, Deserialize)]
pub struct CompItemRequest {
    pub reason: String,
}

#[derive(Debug, Deserialize)]
pub struct PreauthCardRequest {
    pub card_label: String,
    pub hold_cents: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateOrderNotesRequest {
    pub client_profile_notes: String,
}

#[derive(Debug, Deserialize)]
pub struct PayOrderRequest {
    pub amount_cents: i64,
    pub tip_cents: i64,
    pub method: PaymentMethod,
    pub payer_label: String,
}

#[derive(Debug, Deserialize)]
pub struct RefundOrderRequest {
    pub payment_id: String,
    pub amount_cents: i64,
    pub reason: String,
}

#[derive(Debug, Deserialize)]
pub struct PrintReceiptRequest {
    #[serde(default)]
    pub kick_drawer: bool,
}

#[derive(Debug, Deserialize)]
pub struct ToggleModRequest {
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
pub struct OpenShiftRequest {
    pub terminal_id: Option<String>,
    pub opening_float_cents: i64,
}

#[derive(Debug, Deserialize)]
pub struct CashMovementRequest {
    pub kind: CashMovementKind,
    pub amount_cents: i64,
    pub reason: String,
}

#[derive(Debug, Deserialize)]
pub struct CloseShiftZRequest {
    pub counted_cash_cents: i64,
}

#[derive(Debug, Serialize)]
pub struct TrialBalanceRow {
    pub account: AccountCode,
    pub debit_cents: i64,
    pub credit_cents: i64,
    pub net_cents: i64,
}

#[derive(Debug, Serialize)]
pub struct LedgerSummaryResponse {
    pub store_id: String,
    pub is_balanced: bool,
    pub total_debits_cents: i64,
    pub total_credits_cents: i64,
    pub trial_balance: Vec<TrialBalanceRow>,
    pub entries: Vec<JournalEntry>,
}

#[derive(Debug, Serialize)]
pub struct ShiftsSummaryResponse {
    pub current_shift: Option<CashDrawerShift>,
    pub x_report: Option<ShiftReport>,
    pub history: Vec<CashDrawerShift>,
}

#[derive(Debug, Deserialize)]
pub struct OutboxQuery {
    #[serde(default)]
    pub since: u64,
    #[serde(default = "default_outbox_limit")]
    pub limit: usize,
}

fn default_outbox_limit() -> usize {
    100
}

pub fn build_router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let api_router = Router::new()
        .route("/api/health", get(health_handler))
        .route("/api/bootstrap", get(bootstrap_handler))
        .route("/api/auth/login", post(login_handler))
        .route("/api/auth/manager-override", post(manager_override_handler))
        .route("/api/stores", get(list_stores_handler))
        .route("/api/stores/:store_id/activate", post(activate_store_handler))
        .route("/api/vertical/:vertical", post(switch_vertical_handler))
        .route("/api/theme", get(get_theme_handler).put(update_theme_handler))
        .route(
            "/api/theme/preset/:preset_id",
            post(apply_theme_preset_handler),
        )
        .route(
            "/api/catalog",
            get(list_catalog_handler).post(create_catalog_item_handler),
        )
        .route("/api/catalog/categories", post(create_category_handler))
        .route(
            "/api/catalog/:id/toggle-availability",
            patch(toggle_catalog_availability_handler),
        )
        .route(
            "/api/orders",
            get(list_orders_handler).post(create_order_handler),
        )
        .route("/api/orders/:id/qr-link", post(mint_order_qr_link_handler))
        .route("/api/orders/:id/items", post(add_order_item_handler))
        .route(
            "/api/orders/:id/items/:line_id",
            delete(remove_order_item_handler),
        )
        .route(
            "/api/orders/:id/items/:line_id/comp",
            post(comp_order_item_handler),
        )
        .route(
            "/api/orders/:id/items/:line_id/advance-prep",
            post(advance_item_prep_handler),
        )
        .route("/api/orders/:id/preauth", post(preauth_order_card_handler))
        .route("/api/orders/:id/notes", patch(update_order_notes_handler))
        .route("/api/orders/:id/fire", post(fire_order_handler))
        .route("/api/orders/:id/request-check", post(request_check_handler))
        .route("/api/orders/:id/split/:ways", get(split_order_handler))
        .route("/api/orders/:id/pay", post(pay_order_handler))
        .route("/api/orders/:id/refund", post(refund_order_handler))
        .route(
            "/api/orders/:id/print-receipt",
            post(print_order_receipt_handler),
        )
        .route("/api/mods", get(list_mods_handler))
        .route("/api/mods/:id/toggle", post(toggle_mod_handler))
        .route("/api/ledger", get(get_ledger_handler))
        .route("/api/shifts", get(get_shifts_handler))
        .route("/api/shifts/open", post(open_shift_handler))
        .route("/api/shifts/movement", post(shift_movement_handler))
        .route("/api/shifts/close-z", post(close_shift_z_handler))
        .route("/api/hal/status", get(get_hal_status_handler))
        .route("/api/hal/kick-drawer", post(kick_drawer_handler))
        .route("/api/sync/outbox", get(get_sync_outbox_handler))
        .route("/api/audit", get(get_audit_handler))
        .route("/ws", get(ws_handler))
        .layer(cors)
        .with_state(state);

    // Automatically host built SPA (`frontend/dist` or `POS_STATIC_DIR`) for LAN tablets & QR guests
    let static_dir = std::env::var("POS_STATIC_DIR")
        .map(PathBuf::from)
        .ok()
        .or_else(|| {
            for candidate in ["./frontend/dist", "../../frontend/dist"] {
                let p = PathBuf::from(candidate);
                if p.join("index.html").exists() {
                    return Some(p);
                }
            }
            None
        });
    if let Some(dir) = static_dir {
        let index_file = dir.join("index.html");
        api_router.fallback_service(ServeDir::new(dir).not_found_service(ServeFile::new(index_file)))
    } else {
        api_router
    }
}

fn check_idempotency(
    storage: &PosStorage,
    actor_id: &str,
    action_scope: &str,
    headers: &HeaderMap,
) -> Option<(Option<String>, Option<Response>)> {
    let raw_key = headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())?;
    let scoped = format!("{actor_id}:{action_scope}:{raw_key}");
    if let Some(hit) = storage.get_idempotency(&scoped) {
        let status = StatusCode::from_u16(hit.status_code).unwrap_or(StatusCode::OK);
        let val: serde_json::Value =
            serde_json::from_str(&hit.response_json).unwrap_or(serde_json::Value::Null);
        return Some((Some(scoped), Some((status, Json(val)).into_response())));
    }
    Some((Some(scoped), None))
}

async fn health_handler(State(state): State<AppState>) -> impl IntoResponse {
    let guard = state.storage.read().await;
    let active = guard.active_store();
    let audit = guard.verify_audit_chain();
    let enabled_mods = guard
        .mod_engine
        .list_mods()
        .iter()
        .filter(|m| m.enabled)
        .count();

    Json(serde_json::json!({
        "status": "ok",
        "version": "0.2.0",
        "engine": "pos-core + pos-ledger + redb + wasmi + pos-hal (100% Pure Rust)",
        "active_store_id": active.store_id,
        "vertical": active.vertical,
        "platforms": ["linux", "windows", "ios", "android"],
        "audit_chain_valid": audit.valid,
        "audit_entries": audit.total_entries,
        "enabled_mods": enabled_mods,
    }))
}

async fn bootstrap_handler(
    State(state): State<AppState>,
    OptionalSession(maybe_claims): OptionalSession,
) -> impl IntoResponse {
    let guard = state.storage.read().await;
    let active = guard.active_store();
    let all_stores = guard
        .list_stores()
        .into_iter()
        .map(|s| StoreSummaryView {
            active: s.store_id == active.store_id,
            store_id: s.store_id,
            vertical: s.vertical,
            store_name: s.theme.store_name,
        })
        .collect();

    let raw_orders = guard.list_orders(&active.store_id);
    // Deny-by-default Customer check isolation: if authenticated as Customer, return ONLY their scoped check!
    let filtered_orders: Vec<OrderView> = match &maybe_claims {
        Some(claims) if claims.role == Role::Customer => raw_orders
            .iter()
            .filter(|o| claims.can_access_order(&o.id))
            .map(OrderView::from)
            .collect(),
        _ => raw_orders.iter().map(OrderView::from).collect(),
    };

    let audit_report = guard.verify_audit_chain();
    let mut recent_audit = if maybe_claims
        .as_ref()
        .map(|c| c.role == Role::Customer)
        .unwrap_or(false)
    {
        Vec::new()
    } else {
        guard.list_audit_entries()
    };
    recent_audit.reverse();
    recent_audit.truncate(30);

    let operators = vec![
        OperatorPublicView {
            id: "op-owner".to_string(),
            name: "Elena Vance (Store Owner)".to_string(),
            role: Role::Owner,
            demo_pin_hint: "1111",
        },
        OperatorPublicView {
            id: "op-manager".to_string(),
            name: "David Park (Floor Manager)".to_string(),
            role: Role::Manager,
            demo_pin_hint: "9999",
        },
        OperatorPublicView {
            id: "op-bartender".to_string(),
            name: "Marcus Cole (Bartender / Cashier)".to_string(),
            role: Role::Bartender,
            demo_pin_hint: "2222",
        },
        OperatorPublicView {
            id: "op-server".to_string(),
            name: "Sofia Chen (Server / Stylist)".to_string(),
            role: Role::Server,
            demo_pin_hint: "3333",
        },
    ];

    Json(BootstrapResponse {
        active_store_id: active.store_id.clone(),
        stores: all_stores,
        theme: active.theme,
        categories: active.categories,
        prep_stations: active.prep_stations,
        tax_rules: active.tax_rules,
        theme_presets: StoreTheme::all_presets(),
        catalog: guard.list_catalog(&active.store_id),
        orders: filtered_orders,
        mods: guard.mod_engine.list_mods().to_vec(),
        current_shift: guard.current_open_shift(&active.store_id),
        audit_report,
        recent_audit,
        operators,
        supported_platforms: vec!["Linux", "Windows", "iOS", "Android"],
    })
}

async fn login_handler(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, ApiError> {
    let mut guard = state.storage.write().await;
    let active_store_id = guard.active_store_id();

    if req.role == Role::Customer {
        let scoped_id = req
            .scoped_order_id
            .clone()
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| {
                api_err(
                    StatusCode::BAD_REQUEST,
                    "Customer self-service session requires a valid scoped_order_id",
                )
            })?;

        if guard.get_order(&active_store_id, &scoped_id).is_none() {
            return Err(api_err(
                StatusCode::NOT_FOUND,
                format!("Order '{scoped_id}' not found in active store"),
            ));
        }

        let exp_unix = (Utc::now() + Duration::hours(4)).timestamp();
        let claims = SessionClaims {
            jti: Uuid::new_v4().to_string(),
            actor_id: format!("guest-{scoped_id}"),
            actor_name: "Table Guest (QR Self-Service)".to_string(),
            role: Role::Customer,
            store_id: active_store_id.clone(),
            scoped_order_id: Some(scoped_id.clone()),
            exp_unix,
        };
        let token = state.signer.sign_session(&claims);

        guard.append_audit(
            &claims.actor_name,
            Role::Customer,
            "CUSTOMER_QR_SESSION_ISSUED",
            format!("Signed HMAC-SHA256 customer session strictly scoped to '{scoped_id}'"),
        );

        return Ok(Json(LoginResponse {
            token,
            actor_id: claims.actor_id,
            actor_name: claims.actor_name,
            role: Role::Customer,
            store_id: active_store_id,
            scoped_order_id: Some(scoped_id),
            exp_unix,
        }));
    }

    let raw_pin = req.pin.as_deref().unwrap_or("");
    let op = guard
        .authenticate_operator_by_role(req.role, raw_pin)
        .map_err(auth_err_to_api)?;

    let exp_unix = (Utc::now() + Duration::hours(12)).timestamp();
    let claims = SessionClaims {
        jti: Uuid::new_v4().to_string(),
        actor_id: op.id.clone(),
        actor_name: op.name.clone(),
        role: op.role,
        store_id: active_store_id.clone(),
        scoped_order_id: None,
        exp_unix,
    };
    let token = state.signer.sign_session(&claims);

    guard.append_audit(
        &op.name,
        op.role,
        "OPERATOR_ARGON2ID_LOGIN",
        format!("Verified Argon2id PIN & issued signed session for '{}' ({:?})", op.name, op.role),
    );

    Ok(Json(LoginResponse {
        token,
        actor_id: op.id,
        actor_name: op.name,
        role: op.role,
        store_id: active_store_id,
        scoped_order_id: None,
        exp_unix,
    }))
}

async fn manager_override_handler(
    State(state): State<AppState>,
    AuthenticatedSession(_caller): AuthenticatedSession,
    Json(req): Json<ManagerOverrideRequest>,
) -> Result<Json<ManagerOverrideResponse>, ApiError> {
    let mut guard = state.storage.write().await;
    let mgr = guard
        .authenticate_manager_override(&req.pin)
        .map_err(auth_err_to_api)?;

    // Short-lived (3-minute) step-up approval token
    let exp_unix = (Utc::now() + Duration::minutes(3)).timestamp();
    let override_claims = ManagerOverrideClaims {
        jti: Uuid::new_v4().to_string(),
        manager_id: mgr.id.clone(),
        manager_name: mgr.name.clone(),
        role: mgr.role,
        approved_action: req.action.clone(),
        exp_unix,
    };
    let override_token = state.signer.sign_manager_override(&override_claims);

    guard.append_audit(
        &mgr.name,
        mgr.role,
        "MANAGER_STEP_UP_APPROVED",
        format!("Issued step-up override token for action '{}'", req.action),
    );

    Ok(Json(ManagerOverrideResponse {
        override_token,
        manager_name: mgr.name,
        approved_action: req.action,
        exp_unix,
    }))
}

async fn mint_order_qr_link_handler(
    State(state): State<AppState>,
    AuthenticatedSession(claims): AuthenticatedSession,
    Path(order_id): Path<String>,
) -> Result<Json<SignedQrLinkResponse>, ApiError> {
    require_permission(&claims, Permission::ManageAllOrders)?;
    let guard = state.storage.read().await;
    let store_id = guard.active_store_id();
    if guard.get_order(&store_id, &order_id).is_none() {
        return Err(api_err(StatusCode::NOT_FOUND, "Order not found"));
    }

    let exp_unix = (Utc::now() + Duration::hours(4)).timestamp();
    let qr_claims = SessionClaims {
        jti: Uuid::new_v4().to_string(),
        actor_id: format!("qr-guest-{order_id}"),
        actor_name: "QR Guest".to_string(),
        role: Role::Customer,
        store_id,
        scoped_order_id: Some(order_id.clone()),
        exp_unix,
    };
    let token = state.signer.sign_session(&qr_claims);
    let qr_url_path = format!("/?qr_token={token}&order={order_id}");

    Ok(Json(SignedQrLinkResponse {
        order_id,
        token,
        qr_url_path,
        exp_unix,
    }))
}

async fn list_stores_handler(
    State(state): State<AppState>,
    AuthenticatedSession(_claims): AuthenticatedSession,
) -> impl IntoResponse {
    let guard = state.storage.read().await;
    let active_id = guard.active_store_id();
    let stores: Vec<StoreProfile> = guard.list_stores();
    Json(serde_json::json!({
        "active_store_id": active_id,
        "stores": stores,
    }))
}

async fn activate_store_handler(
    State(state): State<AppState>,
    AuthenticatedSession(claims): AuthenticatedSession,
    Path(store_id): Path<String>,
) -> Result<Json<StoreProfile>, ApiError> {
    require_permission(&claims, Permission::ManageAllOrders)?;
    let mut guard = state.storage.write().await;
    let profile = guard
        .activate_store(&store_id)
        .map_err(|e| api_err(StatusCode::NOT_FOUND, e.to_string()))?;
    let ev = guard.record_outbox_event(
        &profile.store_id,
        "store_switched",
        &profile.store_id,
        &serde_json::json!({ "store_id": profile.store_id }),
    );
    drop(guard);

    state.broadcast_sync_event(&ev);
    Ok(Json(profile))
}

async fn switch_vertical_handler(
    State(state): State<AppState>,
    AuthenticatedSession(claims): AuthenticatedSession,
    Path(vertical_str): Path<String>,
) -> Result<Json<StoreTheme>, ApiError> {
    require_permission(&claims, Permission::ManageAllOrders)?;
    let vertical = match vertical_str.as_str() {
        "hospitality" => IndustryVertical::Hospitality,
        "salon_barber" => IndustryVertical::SalonBarber,
        "retail" => IndustryVertical::Retail,
        _ => {
            return Err(api_err(
                StatusCode::BAD_REQUEST,
                "Unsupported vertical. Expected 'hospitality', 'salon_barber', or 'retail'.",
            ))
        }
    };

    let mut guard = state.storage.write().await;
    let profile = guard.switch_vertical(vertical);
    let ev = guard.record_outbox_event(
        &profile.store_id,
        "vertical_updated",
        &profile.store_id,
        &serde_json::json!({ "vertical": vertical }),
    );
    drop(guard);

    state.broadcast_sync_event(&ev);
    Ok(Json(profile.theme))
}

async fn get_theme_handler(State(state): State<AppState>) -> impl IntoResponse {
    let guard = state.storage.read().await;
    Json(guard.active_store().theme)
}

async fn update_theme_handler(
    State(state): State<AppState>,
    AuthenticatedSession(claims): AuthenticatedSession,
    Json(new_theme): Json<StoreTheme>,
) -> Result<Json<StoreTheme>, ApiError> {
    require_permission(&claims, Permission::ManageTheme)?;
    if let Err(err) = new_theme.validate() {
        return Err(api_err(StatusCode::BAD_REQUEST, err.to_string()));
    }

    let mut guard = state.storage.write().await;
    let profile = guard.update_active_theme(new_theme.clone());
    guard.append_audit(
        &claims.actor_name,
        claims.role,
        "THEME_UPDATED",
        format!(
            "Updated store theme '{}' (primary: {}, scheme: {:?})",
            new_theme.store_name, new_theme.primary_color, new_theme.color_scheme
        ),
    );
    let ev = guard.record_outbox_event(
        &profile.store_id,
        "theme_updated",
        &profile.store_id,
        &serde_json::json!({ "preset_id": new_theme.preset_id }),
    );
    drop(guard);

    state.broadcast_sync_event(&ev);
    Ok(Json(new_theme))
}

async fn apply_theme_preset_handler(
    State(state): State<AppState>,
    AuthenticatedSession(claims): AuthenticatedSession,
    Path(preset_id): Path<String>,
) -> Result<Json<StoreTheme>, ApiError> {
    require_permission(&claims, Permission::ManageTheme)?;
    let preset = StoreTheme::all_presets()
        .into_iter()
        .find(|p| p.preset_id == preset_id)
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "Theme preset not found"))?;

    let mut guard = state.storage.write().await;
    let profile = guard.update_active_theme(preset.clone());
    guard.append_audit(
        &claims.actor_name,
        claims.role,
        "THEME_PRESET_APPLIED",
        format!(
            "Applied store theme preset '{}' ({})",
            preset.preset_id, preset.store_name
        ),
    );
    let ev = guard.record_outbox_event(
        &profile.store_id,
        "theme_updated",
        &preset.preset_id,
        &serde_json::json!({ "preset_id": preset.preset_id }),
    );
    drop(guard);

    state.broadcast_sync_event(&ev);
    Ok(Json(preset))
}

async fn list_catalog_handler(State(state): State<AppState>) -> impl IntoResponse {
    let guard = state.storage.read().await;
    let store_id = guard.active_store_id();
    Json(guard.list_catalog(&store_id))
}

async fn create_category_handler(
    State(state): State<AppState>,
    AuthenticatedSession(claims): AuthenticatedSession,
    Json(category): Json<CategoryConfig>,
) -> Result<(StatusCode, Json<StoreProfile>), ApiError> {
    require_permission(&claims, Permission::ManageCatalog)?;
    if category.id.trim().is_empty() || category.label.trim().is_empty() {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "Category id and label cannot be empty",
        ));
    }

    let mut guard = state.storage.write().await;
    let profile = guard.add_category_to_active_store(category.clone());
    guard.append_audit(
        &claims.actor_name,
        claims.role,
        "CATEGORY_CREATED",
        format!("Added custom data-driven category '{}' ({})", category.label, category.id),
    );
    let ev = guard.record_outbox_event(
        &profile.store_id,
        "catalog_updated",
        &category.id,
        &serde_json::json!({ "category_id": category.id }),
    );
    drop(guard);

    state.broadcast_sync_event(&ev);
    Ok((StatusCode::CREATED, Json(profile)))
}

async fn create_catalog_item_handler(
    State(state): State<AppState>,
    AuthenticatedSession(claims): AuthenticatedSession,
    Json(mut item): Json<CatalogItem>,
) -> Result<(StatusCode, Json<CatalogItem>), ApiError> {
    require_permission(&claims, Permission::ManageCatalog)?;
    let mut guard = state.storage.write().await;
    let active = guard.active_store();
    item.store_id = active.store_id.clone();
    item.vertical = active.vertical;
    guard.save_catalog_item(&item);

    guard.append_audit(
        &claims.actor_name,
        claims.role,
        "CATALOG_ITEM_CREATED",
        format!(
            "Added catalog item '{}' (${:.2}) in category '{}'",
            item.name,
            item.price_cents as f64 / 100.0,
            item.category
        ),
    );
    let ev = guard.record_outbox_event(
        &active.store_id,
        "catalog_updated",
        &item.id,
        &serde_json::json!({ "item_id": item.id }),
    );
    drop(guard);

    state.broadcast_sync_event(&ev);
    Ok((StatusCode::CREATED, Json(item)))
}

async fn toggle_catalog_availability_handler(
    State(state): State<AppState>,
    AuthenticatedSession(claims): AuthenticatedSession,
    Path(id): Path<String>,
) -> Result<Json<CatalogItem>, ApiError> {
    require_permission(&claims, Permission::ManageAllOrders)?;
    let mut guard = state.storage.write().await;
    let store_id = guard.active_store_id();
    let mut item = guard
        .get_catalog_item(&store_id, &id)
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "Catalog item not found"))?;

    item.available = !item.available;
    if item.available && item.stock_count == Some(0) {
        item.stock_count = Some(10);
    }
    guard.save_catalog_item(&item);

    guard.append_audit(
        &claims.actor_name,
        claims.role,
        "CATALOG_AVAILABILITY_TOGGLED",
        format!(
            "Item '{}' marked as {}",
            item.name,
            if item.available {
                "Available"
            } else {
                "86'd (Unavailable)"
            }
        ),
    );
    let ev = guard.record_outbox_event(
        &store_id,
        "catalog_updated",
        &item.id,
        &serde_json::json!({ "item_id": item.id, "available": item.available }),
    );
    drop(guard);

    state.broadcast_sync_event(&ev);
    Ok(Json(item))
}

async fn list_orders_handler(
    State(state): State<AppState>,
    AuthenticatedSession(claims): AuthenticatedSession,
) -> impl IntoResponse {
    let guard = state.storage.read().await;
    let store_id = guard.active_store_id();
    let views: Vec<OrderView> = guard
        .list_orders(&store_id)
        .iter()
        .filter(|o| claims.can_access_order(&o.id))
        .map(OrderView::from)
        .collect();
    Json(views)
}

async fn create_order_handler(
    State(state): State<AppState>,
    AuthenticatedSession(claims): AuthenticatedSession,
    headers: HeaderMap,
    Json(req): Json<CreateOrderRequest>,
) -> Result<Response, ApiError> {
    require_permission(&claims, Permission::ManageAllOrders)?;

    let mut guard = state.storage.write().await;
    let mut idem_key_to_save: Option<String> = None;
    if let Some((scoped, maybe_cached)) =
        check_idempotency(&guard, &claims.actor_id, "create_order", &headers)
    {
        if let Some(resp) = maybe_cached {
            return Ok(resp);
        }
        idem_key_to_save = scoped;
    }

    let store_id = guard.active_store_id();
    let id = format!("ord-{}", &Uuid::new_v4().to_string()[..8]);
    let mut order = Order::new_for_store(
        &store_id,
        &id,
        &req.table_or_tab_name,
        req.is_bar_tab,
        req.guest_count,
        &req.assigned_operator,
    );

    if let Some(card_label) = &req.preauth_card {
        let mut term = state.terminal.write().await;
        if let Ok(intent) = term.preauth(&id, card_label, 5_000) {
            order.preauth_card = Some(format!("{} [{}]", card_label, intent.p2pe_token));
        } else {
            order.preauth_card = Some(card_label.clone());
        }
    }
    order.client_profile_notes = req.client_profile_notes;
    guard.save_order_with_hooks(&mut order);

    let view = OrderView::from(&order);
    let body_json = serde_json::to_string(&view).unwrap_or_default();
    if let Some(scoped) = idem_key_to_save {
        guard.save_idempotency(&scoped, StatusCode::CREATED.as_u16(), body_json);
    }

    guard.append_audit(
        &claims.actor_name,
        claims.role,
        "ORDER_OPENED",
        format!(
            "Opened '{}' ({} guests) in store '{}'",
            req.table_or_tab_name, req.guest_count, store_id
        ),
    );
    let ev = guard.record_outbox_event(
        &store_id,
        "orders_updated",
        &order.id,
        &serde_json::json!({ "order_id": order.id, "action": "created" }),
    );
    drop(guard);

    state.broadcast_sync_event(&ev);
    Ok((StatusCode::CREATED, Json(view)).into_response())
}

async fn add_order_item_handler(
    State(state): State<AppState>,
    AuthenticatedSession(claims): AuthenticatedSession,
    headers: HeaderMap,
    Path(order_id): Path<String>,
    Json(req): Json<AddOrderItemRequest>,
) -> Result<Response, ApiError> {
    require_order_access(&claims, &order_id)?;
    if claims.role == Role::Customer {
        require_permission(&claims, Permission::CustomerSelfOrder)?;
    } else {
        require_permission(&claims, Permission::ManageAllOrders)?;
    }

    let mut guard = state.storage.write().await;
    let mut idem_key_to_save: Option<String> = None;
    if let Some((scoped, maybe_cached)) = check_idempotency(
        &guard,
        &claims.actor_id,
        &format!("add_item:{order_id}"),
        &headers,
    ) {
        if let Some(resp) = maybe_cached {
            return Ok(resp);
        }
        idem_key_to_save = scoped;
    }

    let store_id = guard.active_store_id();
    let mut catalog_item = guard
        .get_catalog_item(&store_id, &req.catalog_item_id)
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "Catalog item not found"))?;

    if !catalog_item.available {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "This item is currently 86'd / out of stock",
        ));
    }

    let mut order = guard
        .get_order(&store_id, &order_id)
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "Order not found"))?;

    let qty = req.quantity.max(1);
    if let Some(current_stock) = catalog_item.stock_count {
        let new_stock = current_stock.saturating_sub(qty);
        catalog_item.stock_count = Some(new_stock);
        if new_stock == 0 {
            catalog_item.available = false;
        }
        guard.save_catalog_item(&catalog_item);
    }

    let selected_modifiers: Vec<ModifierOption> = catalog_item
        .modifiers
        .iter()
        .filter(|m| req.modifier_ids.contains(&m.id))
        .map(|m| {
            let mut cloned = m.clone();
            cloned.qualifier = req.modifier_qualifier;
            cloned
        })
        .collect();

    let effective_source = if claims.role == Role::Customer {
        OrderSource::CustomerSelfService
    } else {
        req.source
    };

    let prep_status = match effective_source {
        OrderSource::BartenderSpeedRail | OrderSource::CustomerSelfService => ItemPrepStatus::Fired,
        OrderSource::ServerTableside => ItemPrepStatus::Held,
    };

    let line = OrderLineItem {
        id: format!("line-{}", &Uuid::new_v4().to_string()[..8]),
        catalog_item_id: catalog_item.id.clone(),
        name: catalog_item.name.clone(),
        category: catalog_item.category.clone(),
        quantity: qty,
        unit_price_cents: catalog_item.price_cents,
        selected_modifiers,
        seat_number: req.seat_number,
        notes: req.notes.clone(),
        added_by: claims.actor_name.clone(),
        source: effective_source,
        prep_station: catalog_item.prep_station.clone(),
        prep_status,
        tax_rate_bps: catalog_item.tax_rate_bps,
        tax_mode: catalog_item.tax_mode,
        comped: false,
        comp_reason: None,
        comp_approved_by: None,
    };

    order.items.push(line);
    order.updated_at = Utc::now();
    guard.save_order_with_hooks(&mut order);

    let view = OrderView::from(&order);
    if let Some(scoped) = idem_key_to_save {
        let json_str = serde_json::to_string(&view).unwrap_or_default();
        guard.save_idempotency(&scoped, StatusCode::OK.as_u16(), json_str);
    }

    guard.append_audit(
        &claims.actor_name,
        claims.role,
        "ORDER_ITEM_ADDED",
        format!(
            "Added {}x '{}' to '{}' via {:?}",
            qty, catalog_item.name, order.table_or_tab_name, effective_source
        ),
    );
    let ev = guard.record_outbox_event(
        &store_id,
        "orders_updated",
        &order.id,
        &serde_json::json!({ "order_id": order.id, "action": "item_added" }),
    );
    drop(guard);

    state.broadcast_sync_event(&ev);
    Ok((StatusCode::OK, Json(view)).into_response())
}

async fn remove_order_item_handler(
    State(state): State<AppState>,
    AuthenticatedSession(claims): AuthenticatedSession,
    headers: HeaderMap,
    Path((order_id, line_id)): Path<(String, String)>,
) -> Result<Json<OrderView>, ApiError> {
    require_permission(&claims, Permission::VoidUnfiredItem)?;
    let mut guard = state.storage.write().await;
    let store_id = guard.active_store_id();
    let mut order = guard
        .get_order(&store_id, &order_id)
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "Order not found"))?;

    let target_item = order
        .items
        .iter()
        .find(|i| i.id == line_id)
        .cloned()
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "Line item not found"))?;

    // If item was already Fired, Ready, or Served to KDS/Bar, require Manager Step-Up Override!
    let approver = if target_item.prep_status != ItemPrepStatus::Held {
        require_manager_approval(&state, &claims, &headers, "Void Fired/Prepared Item")?
    } else {
        claims.actor_name.clone()
    };

    order.items.retain(|i| i.id != line_id);
    order.updated_at = Utc::now();
    guard.save_order_with_hooks(&mut order);

    guard.append_audit(
        &approver,
        claims.role,
        "ORDER_ITEM_VOIDED",
        format!(
            "Voided item '{}' (status {:?}) from '{}'",
            target_item.name, target_item.prep_status, order.table_or_tab_name
        ),
    );
    let ev = guard.record_outbox_event(
        &store_id,
        "orders_updated",
        &order.id,
        &serde_json::json!({ "order_id": order.id, "action": "item_voided" }),
    );
    drop(guard);

    state.broadcast_sync_event(&ev);
    Ok(Json(OrderView::from(&order)))
}

async fn comp_order_item_handler(
    State(state): State<AppState>,
    AuthenticatedSession(claims): AuthenticatedSession,
    headers: HeaderMap,
    Path((order_id, line_id)): Path<(String, String)>,
    Json(req): Json<CompItemRequest>,
) -> Result<Json<OrderView>, ApiError> {
    require_permission(&claims, Permission::ManageAllOrders)?;
    let approver = require_manager_approval(&state, &claims, &headers, "Comp Line Item")?;

    let mut guard = state.storage.write().await;
    let store_id = guard.active_store_id();
    let mut order = guard
        .get_order(&store_id, &order_id)
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "Order not found"))?;

    let item = order
        .items
        .iter_mut()
        .find(|i| i.id == line_id)
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "Line item not found"))?;

    item.comped = !item.comped;
    let item_name = item.name.clone();
    let now_comped = item.comped;
    if now_comped {
        item.comp_reason = Some(req.reason.clone());
        item.comp_approved_by = Some(approver.clone());
    } else {
        item.comp_reason = None;
        item.comp_approved_by = None;
    }

    order.updated_at = Utc::now();
    guard.save_order_with_hooks(&mut order);

    guard.append_audit(
        &approver,
        claims.role,
        if now_comped {
            "ORDER_ITEM_COMPED"
        } else {
            "ORDER_ITEM_UNCOMPED"
        },
        format!(
            "{} '{}' on '{}' (Reason: {})",
            if now_comped { "Comped" } else { "Restored" },
            item_name,
            order.table_or_tab_name,
            req.reason
        ),
    );
    let ev = guard.record_outbox_event(
        &store_id,
        "orders_updated",
        &order.id,
        &serde_json::json!({ "order_id": order.id, "action": "item_comped" }),
    );
    drop(guard);

    state.broadcast_sync_event(&ev);
    Ok(Json(OrderView::from(&order)))
}

async fn advance_item_prep_handler(
    State(state): State<AppState>,
    AuthenticatedSession(claims): AuthenticatedSession,
    Path((order_id, line_id)): Path<(String, String)>,
) -> Result<Json<OrderView>, ApiError> {
    require_permission(&claims, Permission::ManageAllOrders)?;
    let mut guard = state.storage.write().await;
    let store_id = guard.active_store_id();
    let mut order = guard
        .get_order(&store_id, &order_id)
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "Order not found"))?;

    for item in &mut order.items {
        if item.id == line_id {
            item.prep_status = match item.prep_status {
                ItemPrepStatus::Held => ItemPrepStatus::Fired,
                ItemPrepStatus::Fired => ItemPrepStatus::Ready,
                ItemPrepStatus::Ready => ItemPrepStatus::Served,
                ItemPrepStatus::Served => ItemPrepStatus::Served,
            };
        }
    }
    order.updated_at = Utc::now();
    guard.save_order_raw(&order);
    let ev = guard.record_outbox_event(
        &store_id,
        "orders_updated",
        &order.id,
        &serde_json::json!({ "order_id": order.id, "action": "prep_advanced" }),
    );
    drop(guard);

    state.broadcast_sync_event(&ev);
    Ok(Json(OrderView::from(&order)))
}

async fn preauth_order_card_handler(
    State(state): State<AppState>,
    AuthenticatedSession(claims): AuthenticatedSession,
    Path(order_id): Path<String>,
    Json(req): Json<PreauthCardRequest>,
) -> Result<Json<OrderView>, ApiError> {
    require_permission(&claims, Permission::ManageAllOrders)?;
    let mut guard = state.storage.write().await;
    let store_id = guard.active_store_id();
    let mut order = guard
        .get_order(&store_id, &order_id)
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "Order not found"))?;

    let hold_cents = req.hold_cents.unwrap_or(5_000).max(500);
    let intent = state
        .terminal
        .write()
        .await
        .preauth(&order_id, &req.card_label, hold_cents)
        .map_err(|e| api_err(StatusCode::BAD_REQUEST, e.to_string()))?;

    order.preauth_card = Some(format!(
        "{} [{} • ${:.2} Hold]",
        req.card_label,
        intent.p2pe_token,
        hold_cents as f64 / 100.0
    ));
    order.updated_at = Utc::now();
    guard.save_order_raw(&order);

    guard.append_audit(
        &claims.actor_name,
        claims.role,
        "P2PE_CARD_PREAUTH_BOUND",
        format!(
            "Authorized P2PE hold {} (${:.2}) for '{}' on '{}'",
            intent.p2pe_token,
            hold_cents as f64 / 100.0,
            req.card_label,
            order.table_or_tab_name
        ),
    );
    let ev = guard.record_outbox_event(
        &store_id,
        "orders_updated",
        &order.id,
        &serde_json::json!({ "order_id": order.id, "p2pe_token": intent.p2pe_token }),
    );
    drop(guard);

    state.broadcast_sync_event(&ev);
    Ok(Json(OrderView::from(&order)))
}

async fn update_order_notes_handler(
    State(state): State<AppState>,
    AuthenticatedSession(claims): AuthenticatedSession,
    Path(order_id): Path<String>,
    Json(req): Json<UpdateOrderNotesRequest>,
) -> Result<Json<OrderView>, ApiError> {
    require_permission(&claims, Permission::ManageAllOrders)?;
    let mut guard = state.storage.write().await;
    let store_id = guard.active_store_id();
    let mut order = guard
        .get_order(&store_id, &order_id)
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "Order not found"))?;

    order.client_profile_notes = if req.client_profile_notes.trim().is_empty() {
        None
    } else {
        Some(req.client_profile_notes.trim().to_string())
    };
    order.updated_at = Utc::now();
    guard.save_order_raw(&order);
    let ev = guard.record_outbox_event(
        &store_id,
        "orders_updated",
        &order.id,
        &serde_json::json!({ "order_id": order.id, "action": "notes_updated" }),
    );
    drop(guard);

    state.broadcast_sync_event(&ev);
    Ok(Json(OrderView::from(&order)))
}

async fn fire_order_handler(
    State(state): State<AppState>,
    AuthenticatedSession(claims): AuthenticatedSession,
    Path(order_id): Path<String>,
) -> Result<Json<OrderView>, ApiError> {
    require_permission(&claims, Permission::ManageAllOrders)?;
    let mut guard = state.storage.write().await;
    let store_id = guard.active_store_id();
    let mut order = guard
        .get_order(&store_id, &order_id)
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "Order not found"))?;

    for item in &mut order.items {
        if item.prep_status == ItemPrepStatus::Held {
            item.prep_status = ItemPrepStatus::Fired;
        }
    }
    if order.status == OrderStatus::Open {
        order.status = OrderStatus::SentToPrep;
    }
    order.updated_at = Utc::now();
    guard.save_order_raw(&order);

    guard.append_audit(
        &claims.actor_name,
        claims.role,
        "ORDER_FIRED_TO_PREP",
        format!("Fired held items for '{}'", order.table_or_tab_name),
    );
    let ev = guard.record_outbox_event(
        &store_id,
        "orders_updated",
        &order.id,
        &serde_json::json!({ "order_id": order.id, "action": "fired" }),
    );
    drop(guard);

    state.broadcast_sync_event(&ev);
    Ok(Json(OrderView::from(&order)))
}

async fn request_check_handler(
    State(state): State<AppState>,
    AuthenticatedSession(claims): AuthenticatedSession,
    Path(order_id): Path<String>,
) -> Result<Json<OrderView>, ApiError> {
    require_order_access(&claims, &order_id)?;
    let mut guard = state.storage.write().await;
    let store_id = guard.active_store_id();
    let mut order = guard
        .get_order(&store_id, &order_id)
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "Order not found"))?;

    if order.status != OrderStatus::Paid && order.status != OrderStatus::PartiallyPaid {
        order.status = OrderStatus::CheckRequested;
    }
    order.updated_at = Utc::now();
    guard.save_order_raw(&order);

    guard.append_audit(
        &claims.actor_name,
        claims.role,
        "CHECK_REQUESTED",
        format!("Check requested for '{}'", order.table_or_tab_name),
    );
    let ev = guard.record_outbox_event(
        &store_id,
        "orders_updated",
        &order.id,
        &serde_json::json!({ "order_id": order.id, "action": "check_requested" }),
    );
    drop(guard);

    state.broadcast_sync_event(&ev);
    Ok(Json(OrderView::from(&order)))
}

async fn split_order_handler(
    State(state): State<AppState>,
    AuthenticatedSession(claims): AuthenticatedSession,
    Path((order_id, ways)): Path<(String, u8)>,
) -> Result<Json<Vec<SplitCheckShare>>, ApiError> {
    require_order_access(&claims, &order_id)?;
    let guard = state.storage.read().await;
    let store_id = guard.active_store_id();
    let order = guard
        .get_order(&store_id, &order_id)
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "Order not found"))?;

    Ok(Json(order.split_equally(ways)))
}

async fn pay_order_handler(
    State(state): State<AppState>,
    AuthenticatedSession(claims): AuthenticatedSession,
    headers: HeaderMap,
    Path(order_id): Path<String>,
    Json(req): Json<PayOrderRequest>,
) -> Result<Response, ApiError> {
    require_order_access(&claims, &order_id)?;
    require_permission(&claims, Permission::CustomerPayCheck)?;

    let mut guard = state.storage.write().await;
    let mut idem_key_to_save: Option<String> = None;
    if let Some((scoped, maybe_cached)) = check_idempotency(
        &guard,
        &claims.actor_id,
        &format!("pay:{order_id}"),
        &headers,
    ) {
        if let Some(resp) = maybe_cached {
            return Ok(resp);
        }
        idem_key_to_save = scoped;
    }

    let store_id = guard.active_store_id();
    let mut order = guard
        .get_order(&store_id, &order_id)
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "Order not found"))?;

    if order.remaining_balance_cents() <= 0 {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "Order has no remaining balance to pay",
        ));
    }

    let clamped_principal = req.amount_cents.clamp(1, order.remaining_balance_cents());
    let clamped_tip = req.tip_cents.max(0);

    // Execute payment via HAL P2PE Terminal (capturing open pre-auth if applicable)
    let p2pe_token = {
        let mut term = state.terminal.write().await;
        if req.method != PaymentMethod::Cash {
            if let Some(open_auth) = term.find_open_auth_for_order(&order_id) {
                term.capture(&open_auth.id, clamped_principal, clamped_tip)
                    .map(|i| i.p2pe_token)
                    .map_err(|e| api_err(StatusCode::BAD_REQUEST, e.to_string()))?
            } else {
                term.sale(
                    &order_id,
                    req.method,
                    &req.payer_label,
                    clamped_principal,
                    clamped_tip,
                )
                .map(|i| i.p2pe_token)
                .map_err(|e| api_err(StatusCode::BAD_REQUEST, e.to_string()))?
            }
        } else {
            let _ = state
                .printer
                .write()
                .await
                .kick_cash_drawer(&format!("Cash tender on {}", order.table_or_tab_name));
            term.sale(
                &order_id,
                PaymentMethod::Cash,
                &req.payer_label,
                clamped_principal,
                clamped_tip,
            )
            .map(|i| i.p2pe_token)
            .map_err(|e| api_err(StatusCode::BAD_REQUEST, e.to_string()))?
        }
    };

    let payment = order.record_payment(
        clamped_principal,
        clamped_tip,
        req.method,
        &req.payer_label,
        Some(p2pe_token),
    );
    guard.save_order_raw(&order);

    // Update Cash Drawer Shift & Double-Entry Ledger
    let mut shift_id_opt = None;
    if let Some(mut shift) = guard.current_open_shift(&store_id) {
        if req.method == PaymentMethod::Cash {
            shift.cash_sales_cents += payment.amount_cents + payment.tip_cents;
        } else {
            shift.card_sales_cents += payment.amount_cents;
        }
        shift.tips_collected_cents += payment.tip_cents;
        shift_id_opt = Some(shift.id.clone());
        guard.save_shift(&shift);
    }

    if let Ok(journal) = JournalEntry::for_order_payment(&order, &payment, shift_id_opt) {
        guard.append_journal_entry(&journal);
    }

    let view = OrderView::from(&order);
    if let Some(scoped) = idem_key_to_save {
        let json_str = serde_json::to_string(&view).unwrap_or_default();
        guard.save_idempotency(&scoped, StatusCode::OK.as_u16(), json_str);
    }

    guard.append_audit(
        &claims.actor_name,
        claims.role,
        "ORDER_PAYMENT_CAPTURED",
        format!(
            "Captured ${:.2} (+ ${:.2} tip) via {:?} [{}] on '{}'",
            payment.amount_cents as f64 / 100.0,
            payment.tip_cents as f64 / 100.0,
            payment.method,
            payment.token_reference,
            order.table_or_tab_name
        ),
    );
    let ev = guard.record_outbox_event(
        &store_id,
        "orders_updated",
        &order.id,
        &serde_json::json!({ "order_id": order.id, "payment_id": payment.id }),
    );
    drop(guard);

    state.broadcast_sync_event(&ev);
    Ok((StatusCode::OK, Json(view)).into_response())
}

async fn refund_order_handler(
    State(state): State<AppState>,
    AuthenticatedSession(claims): AuthenticatedSession,
    headers: HeaderMap,
    Path(order_id): Path<String>,
    Json(req): Json<RefundOrderRequest>,
) -> Result<Json<OrderView>, ApiError> {
    require_permission(&claims, Permission::ManageAllOrders)?;
    let approver = require_manager_approval(&state, &claims, &headers, "Issue Payment Refund")?;

    let mut guard = state.storage.write().await;
    let store_id = guard.active_store_id();
    let mut order = guard
        .get_order(&store_id, &order_id)
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "Order not found"))?;

    let orig_payment = order
        .payments
        .iter()
        .find(|p| p.id == req.payment_id)
        .cloned()
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "Payment record not found on order"))?;

    let refund = order
        .record_refund(&req.payment_id, req.amount_cents, &req.reason, &approver)
        .ok_or_else(|| api_err(StatusCode::BAD_REQUEST, "Order has no refundable principal"))?;

    guard.save_order_raw(&order);

    // Refund on P2PE terminal if intent exists
    {
        let mut term = state.terminal.write().await;
        if let Some(intent) = term.find_by_token(&orig_payment.token_reference) {
            let _ = term.refund(&intent.id, refund.amount_cents);
        }
    }

    let mut shift_id_opt = None;
    if let Some(mut shift) = guard.current_open_shift(&store_id) {
        if orig_payment.method == PaymentMethod::Cash {
            shift.cash_refunds_cents += refund.amount_cents;
        }
        shift_id_opt = Some(shift.id.clone());
        guard.save_shift(&shift);
    }

    if let Ok(journal) = JournalEntry::for_refund(
        &store_id,
        &order.id,
        &refund,
        orig_payment.method,
        shift_id_opt,
    ) {
        guard.append_journal_entry(&journal);
    }

    guard.append_audit(
        &approver,
        claims.role,
        "ORDER_PAYMENT_REFUNDED",
        format!(
            "Refunded ${:.2} on '{}' (Reason: {})",
            refund.amount_cents as f64 / 100.0,
            order.table_or_tab_name,
            refund.reason
        ),
    );
    let ev = guard.record_outbox_event(
        &store_id,
        "orders_updated",
        &order.id,
        &serde_json::json!({ "order_id": order.id, "refund_id": refund.id }),
    );
    drop(guard);

    state.broadcast_sync_event(&ev);
    Ok(Json(OrderView::from(&order)))
}

async fn print_order_receipt_handler(
    State(state): State<AppState>,
    AuthenticatedSession(claims): AuthenticatedSession,
    Path(order_id): Path<String>,
    Json(req): Json<PrintReceiptRequest>,
) -> Result<Json<RenderedReceipt>, ApiError> {
    require_order_access(&claims, &order_id)?;
    let guard = state.storage.read().await;
    let active = guard.active_store();
    let order = guard
        .get_order(&active.store_id, &order_id)
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "Order not found"))?;
    drop(guard);

    let kick = req.kick_drawer && claims.role != Role::Customer;
    let rendered = state
        .printer
        .write()
        .await
        .print_order_receipt(&active.theme, &order, kick)
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(rendered))
}

async fn list_mods_handler(State(state): State<AppState>) -> impl IntoResponse {
    let guard = state.storage.read().await;
    Json(guard.mod_engine.list_mods().to_vec())
}

async fn toggle_mod_handler(
    State(state): State<AppState>,
    AuthenticatedSession(claims): AuthenticatedSession,
    Path(mod_id): Path<String>,
    Json(req): Json<ToggleModRequest>,
) -> Result<Json<ModManifest>, ApiError> {
    require_permission(&claims, Permission::ManageMods)?;
    let mut guard = state.storage.write().await;
    let store_id = guard.active_store_id();
    let updated_mod = guard
        .mod_engine
        .set_mod_enabled(&mod_id, req.enabled)
        .map_err(|err| api_err(StatusCode::NOT_FOUND, err.to_string()))?;

    guard.recalculate_store_orders(&store_id);
    guard.append_audit(
        &claims.actor_name,
        claims.role,
        "WASM_MOD_TOGGLED",
        format!(
            "Mod '{}' ({}) set to {}",
            updated_mod.name,
            updated_mod.id,
            if req.enabled { "ENABLED" } else { "DISABLED" }
        ),
    );
    let ev = guard.record_outbox_event(
        &store_id,
        "mods_updated",
        &updated_mod.id,
        &serde_json::json!({ "mod_id": updated_mod.id, "enabled": req.enabled }),
    );
    drop(guard);

    state.broadcast_sync_event(&ev);
    Ok(Json(updated_mod))
}

async fn get_ledger_handler(
    State(state): State<AppState>,
    AuthenticatedSession(claims): AuthenticatedSession,
) -> Result<Json<LedgerSummaryResponse>, ApiError> {
    require_permission(&claims, Permission::ViewAuditLog)?;
    let guard = state.storage.read().await;
    let store_id = guard.active_store_id();
    let entries = guard.list_journal_entries(&store_id);
    let tb_map = summarize_trial_balance(&entries);

    let mut total_debits = 0i64;
    let mut total_credits = 0i64;
    let mut rows = Vec::new();
    for (account, (d, c)) in tb_map {
        total_debits += d;
        total_credits += c;
        rows.push(TrialBalanceRow {
            account,
            debit_cents: d,
            credit_cents: c,
            net_cents: d - c,
        });
    }

    Ok(Json(LedgerSummaryResponse {
        store_id,
        is_balanced: total_debits == total_credits,
        total_debits_cents: total_debits,
        total_credits_cents: total_credits,
        trial_balance: rows,
        entries,
    }))
}

async fn get_shifts_handler(
    State(state): State<AppState>,
    AuthenticatedSession(claims): AuthenticatedSession,
) -> Result<Json<ShiftsSummaryResponse>, ApiError> {
    require_permission(&claims, Permission::ManageAllOrders)?;
    let guard = state.storage.read().await;
    let store_id = guard.active_store_id();
    let current = guard.current_open_shift(&store_id);
    let x_report = current.as_ref().map(CashDrawerShift::x_report);
    let history = guard.list_shifts(&store_id);

    Ok(Json(ShiftsSummaryResponse {
        current_shift: current,
        x_report,
        history,
    }))
}

async fn open_shift_handler(
    State(state): State<AppState>,
    AuthenticatedSession(claims): AuthenticatedSession,
    Json(req): Json<OpenShiftRequest>,
) -> Result<(StatusCode, Json<CashDrawerShift>), ApiError> {
    require_permission(&claims, Permission::ManageDrawerShift)?;
    let mut guard = state.storage.write().await;
    let store_id = guard.active_store_id();

    if guard.current_open_shift(&store_id).is_some() {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "An open cash drawer shift already exists for this store",
        ));
    }

    let shift = CashDrawerShift::open_new(
        &store_id,
        req.terminal_id.unwrap_or_else(|| "term-primary-01".to_string()),
        &claims.actor_name,
        req.opening_float_cents.max(0),
    );
    guard.save_shift(&shift);

    if shift.opening_float_cents > 0 {
        if let Ok(je) = JournalEntry::new_checked(
            &store_id,
            None,
            Some(shift.id.clone()),
            format!("Opening cash till float ({})", shift.id),
            vec![
                LedgerPosting {
                    account: AccountCode::CashInDrawer,
                    debit_cents: shift.opening_float_cents,
                    credit_cents: 0,
                    memo: "Opening drawer float".to_string(),
                },
                LedgerPosting {
                    account: AccountCode::TillFloatEquity,
                    debit_cents: 0,
                    credit_cents: shift.opening_float_cents,
                    memo: "Store till float equity".to_string(),
                },
            ],
        ) {
            guard.append_journal_entry(&je);
        }
    }

    let _ = state
        .printer
        .write()
        .await
        .kick_cash_drawer("Open Cash Drawer Shift");

    guard.append_audit(
        &claims.actor_name,
        claims.role,
        "SHIFT_OPENED",
        format!(
            "Opened shift '{}' with ${:.2} float",
            shift.id,
            shift.opening_float_cents as f64 / 100.0
        ),
    );
    let ev = guard.record_outbox_event(
        &store_id,
        "shifts_updated",
        &shift.id,
        &serde_json::json!({ "shift_id": shift.id, "action": "opened" }),
    );
    drop(guard);

    state.broadcast_sync_event(&ev);
    Ok((StatusCode::CREATED, Json(shift)))
}

async fn shift_movement_handler(
    State(state): State<AppState>,
    AuthenticatedSession(claims): AuthenticatedSession,
    Json(req): Json<CashMovementRequest>,
) -> Result<Json<CashDrawerShift>, ApiError> {
    require_permission(&claims, Permission::ManageDrawerShift)?;
    let mut guard = state.storage.write().await;
    let store_id = guard.active_store_id();
    let mut shift = guard
        .current_open_shift(&store_id)
        .ok_or_else(|| api_err(StatusCode::BAD_REQUEST, "No open cash drawer shift"))?;

    let mov = shift
        .record_movement(req.kind, req.amount_cents, &req.reason, &claims.actor_name)
        .map_err(|e| api_err(StatusCode::BAD_REQUEST, e.to_string()))?;
    guard.save_shift(&shift);

    if mov.amount_cents > 0 {
        let postings = match mov.kind {
            CashMovementKind::PaidOut | CashMovementKind::SafeDrop => vec![
                LedgerPosting {
                    account: AccountCode::PaidOutExpense,
                    debit_cents: mov.amount_cents,
                    credit_cents: 0,
                    memo: mov.reason.clone(),
                },
                LedgerPosting {
                    account: AccountCode::CashInDrawer,
                    debit_cents: 0,
                    credit_cents: mov.amount_cents,
                    memo: format!("{:?} from drawer", mov.kind),
                },
            ],
            CashMovementKind::PaidIn => vec![
                LedgerPosting {
                    account: AccountCode::CashInDrawer,
                    debit_cents: mov.amount_cents,
                    credit_cents: 0,
                    memo: mov.reason.clone(),
                },
                LedgerPosting {
                    account: AccountCode::TillFloatEquity,
                    debit_cents: 0,
                    credit_cents: mov.amount_cents,
                    memo: "Paid-in cash".to_string(),
                },
            ],
        };
        if let Ok(je) = JournalEntry::new_checked(
            &store_id,
            None,
            Some(shift.id.clone()),
            format!("Cash movement {:?}: {}", mov.kind, mov.reason),
            postings,
        ) {
            guard.append_journal_entry(&je);
        }
    }

    let _ = state
        .printer
        .write()
        .await
        .kick_cash_drawer(&format!("{:?}: {}", mov.kind, mov.reason));

    guard.append_audit(
        &claims.actor_name,
        claims.role,
        "CASH_DRAWER_MOVEMENT",
        format!(
            "Recorded {:?} of ${:.2} ({})",
            mov.kind,
            mov.amount_cents as f64 / 100.0,
            mov.reason
        ),
    );
    let ev = guard.record_outbox_event(
        &store_id,
        "shifts_updated",
        &shift.id,
        &serde_json::json!({ "shift_id": shift.id, "movement_id": mov.id }),
    );
    drop(guard);

    state.broadcast_sync_event(&ev);
    Ok(Json(shift))
}

async fn close_shift_z_handler(
    State(state): State<AppState>,
    AuthenticatedSession(claims): AuthenticatedSession,
    headers: HeaderMap,
    Json(req): Json<CloseShiftZRequest>,
) -> Result<Json<ShiftReport>, ApiError> {
    require_permission(&claims, Permission::ManageDrawerShift)?;
    let approver = require_manager_approval(&state, &claims, &headers, "Close End-of-Day Z-Report")?;

    let mut guard = state.storage.write().await;
    let store_id = guard.active_store_id();
    let mut shift = guard
        .current_open_shift(&store_id)
        .ok_or_else(|| api_err(StatusCode::BAD_REQUEST, "No open cash drawer shift to close"))?;

    let z_report = shift
        .close_z_report(req.counted_cash_cents, &approver)
        .map_err(|e| api_err(StatusCode::BAD_REQUEST, e.to_string()))?;
    guard.save_shift(&shift);

    if let Some(variance) = z_report.over_short_cents {
        if variance != 0 {
            let abs_v = variance.abs();
            let postings = if variance < 0 {
                // Cash short: debit Over/Short expense, credit CashInDrawer
                vec![
                    LedgerPosting {
                        account: AccountCode::TillOverShortExpense,
                        debit_cents: abs_v,
                        credit_cents: 0,
                        memo: "Z-Report cash drawer shortage".to_string(),
                    },
                    LedgerPosting {
                        account: AccountCode::CashInDrawer,
                        debit_cents: 0,
                        credit_cents: abs_v,
                        memo: "Reconcile drawer count".to_string(),
                    },
                ]
            } else {
                // Cash over: debit CashInDrawer, credit Over/Short
                vec![
                    LedgerPosting {
                        account: AccountCode::CashInDrawer,
                        debit_cents: abs_v,
                        credit_cents: 0,
                        memo: "Z-Report cash drawer overage".to_string(),
                    },
                    LedgerPosting {
                        account: AccountCode::TillOverShortExpense,
                        debit_cents: 0,
                        credit_cents: abs_v,
                        memo: "Reconcile drawer overage".to_string(),
                    },
                ]
            };
            if let Ok(je) = JournalEntry::new_checked(
                &store_id,
                None,
                Some(shift.id.clone()),
                format!("Z-Report Variance Reconciliation ({})", shift.id),
                postings,
            ) {
                guard.append_journal_entry(&je);
            }
        }
    }

    guard.append_audit(
        &approver,
        claims.role,
        "SHIFT_CLOSED_Z_REPORT",
        format!(
            "Closed Z-Report on '{}' (Expected: ${:.2}, Counted: ${:.2}, Over/Short: ${:.2})",
            shift.id,
            z_report.expected_drawer_cash_cents as f64 / 100.0,
            req.counted_cash_cents as f64 / 100.0,
            z_report.over_short_cents.unwrap_or(0) as f64 / 100.0
        ),
    );
    let ev = guard.record_outbox_event(
        &store_id,
        "shifts_updated",
        &shift.id,
        &serde_json::json!({ "shift_id": shift.id, "action": "closed_z" }),
    );
    drop(guard);

    state.broadcast_sync_event(&ev);
    Ok(Json(z_report))
}

async fn get_hal_status_handler(
    State(state): State<AppState>,
    AuthenticatedSession(claims): AuthenticatedSession,
) -> Result<Json<HalStatusSnapshot>, ApiError> {
    require_permission(&claims, Permission::ManageAllOrders)?;
    let term = state.terminal.read().await;
    let printer = state.printer.read().await;
    let mut intents = term.list_intents().to_vec();
    intents.reverse();
    intents.truncate(20);
    let mut receipts = printer.recent_jobs().to_vec();
    receipts.reverse();
    receipts.truncate(10);

    Ok(Json(HalStatusSnapshot {
        terminal_serial: term.terminal_serial.clone(),
        printer_model: printer.printer_model.clone(),
        drawer_kick_count: printer.drawer_kick_count,
        payment_intents: intents,
        recent_receipts: receipts,
    }))
}

async fn kick_drawer_handler(
    State(state): State<AppState>,
    AuthenticatedSession(claims): AuthenticatedSession,
) -> Result<Json<RenderedReceipt>, ApiError> {
    require_permission(&claims, Permission::ManageDrawerShift)?;
    let rendered = state
        .printer
        .write()
        .await
        .kick_cash_drawer(&format!("No-Sale Drawer Open by {}", claims.actor_name))
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut guard = state.storage.write().await;
    guard.append_audit(
        &claims.actor_name,
        claims.role,
        "NO_SALE_DRAWER_KICK",
        "Triggered RJ-11 cash drawer pulse",
    );

    Ok(Json(rendered))
}

async fn get_sync_outbox_handler(
    State(state): State<AppState>,
    AuthenticatedSession(claims): AuthenticatedSession,
    Query(q): Query<OutboxQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_permission(&claims, Permission::ManageAllOrders)?;
    let guard = state.storage.read().await;
    let events = guard.list_outbox_since(q.since, q.limit.clamp(1, 500));
    let next_cursor = events.last().map(|e| e.seq).unwrap_or(q.since);
    Ok(Json(serde_json::json!({
        "events": events,
        "next_cursor": next_cursor,
    })))
}

async fn get_audit_handler(
    State(state): State<AppState>,
    AuthenticatedSession(claims): AuthenticatedSession,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_permission(&claims, Permission::ViewAuditLog)?;
    let guard = state.storage.read().await;
    let report = guard.verify_audit_chain();
    let mut entries = guard.list_audit_entries();
    entries.reverse();
    Ok(Json(serde_json::json!({
        "verification": report,
        "entries": entries,
    })))
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws_client(socket, state))
}

async fn handle_ws_client(mut socket: WebSocket, state: AppState) {
    let mut rx = state.events_tx.subscribe();
    let welcome = serde_json::json!({
        "seq": 0,
        "event": "connected",
        "timestamp": Utc::now().to_rfc3339(),
    })
    .to_string();
    let _ = socket.send(Message::Text(welcome)).await;

    while let Ok(msg) = rx.recv().await {
        if socket.send(Message::Text(msg)).await.is_err() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;

    async fn login_token(app: &Router, role: Role, pin: Option<&str>, scoped: Option<&str>) -> String {
        let payload = serde_json::json!({
            "role": role,
            "pin": pin,
            "scoped_order_id": scoped,
        });
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/auth/login")
                    .header("content-type", "application/json")
                    .body(Body::from(payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        json["token"].as_str().unwrap().to_string()
    }

    #[tokio::test]
    async fn test_rbac_customer_isolation_manager_override_idempotency_and_ledger() {
        let storage = PosStorage::new_in_memory();
        let app = build_router(AppState::new(storage));

        // 1. Unauthenticated mutation must return 401 Unauthorized
        let unauth_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/theme/preset/bistro_elegance")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unauth_resp.status(), StatusCode::UNAUTHORIZED);

        // 2. Customer QR token scoped to ord-table-4 CANNOT pay or view ord-patio-8 (403 Forbidden!)
        let cust_tok = login_token(&app, Role::Customer, None, Some("ord-table-4")).await;
        let cross_table_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/orders/ord-patio-8/split/2")
                    .header("authorization", format!("Bearer {cust_tok}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(cross_table_resp.status(), StatusCode::FORBIDDEN);

        // 3. Server (PIN 3333) blocked from voiding an already-Fired item without Manager Override
        let server_tok = login_token(&app, Role::Server, Some("3333"), None).await;
        let blocked_void = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/api/orders/ord-table-4/items/line-t4-1")
                    .header("authorization", format!("Bearer {server_tok}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(blocked_void.status(), StatusCode::FORBIDDEN);

        // Mint Manager Step-Up Override token with Manager PIN 9999 and retry void -> 200 OK!
        let ovr_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/auth/manager-override")
                    .header("authorization", format!("Bearer {server_tok}"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"pin":"9999","action":"Void Fired/Prepared Item"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(ovr_resp.status(), StatusCode::OK);
        let ovr_bytes = axum::body::to_bytes(ovr_resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let ovr_json: serde_json::Value = serde_json::from_slice(&ovr_bytes).unwrap();
        let mgr_token = ovr_json["override_token"].as_str().unwrap();

        let approved_void = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/api/orders/ord-table-4/items/line-t4-1")
                    .header("authorization", format!("Bearer {server_tok}"))
                    .header("x-manager-override", mgr_token)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(approved_void.status(), StatusCode::OK);

        // 4. Idempotency-Key deduplication on payment + Double-Entry Ledger verification
        let owner_tok = login_token(&app, Role::Owner, Some("1111"), None).await;
        let pay_body = r#"{"amount_cents":1500,"tip_cents":300,"method":"card_tap_p2_p_e","payer_label":"Split Guest 1"}"#;
        // Wait: let's check PaymentMethod serde rename_all = "snake_case" -> CardTapP2PE serializes as "card_tap_p2_p_e" or "apple_pay"! Let's test "apple_pay" and check CardTapP2PE rename!
        let pay_body_apple = r#"{"amount_cents":1500,"tip_cents":300,"method":"apple_pay","payer_label":"Split Guest 1"}"#;

        let pay_1 = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/orders/ord-patio-8/pay")
                    .header("authorization", format!("Bearer {owner_tok}"))
                    .header("idempotency-key", "idem-pay-001")
                    .header("content-type", "application/json")
                    .body(Body::from(pay_body_apple))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(pay_1.status(), StatusCode::OK);
        let p1_bytes = axum::body::to_bytes(pay_1.into_body(), usize::MAX)
            .await
            .unwrap();
        let p1_json: serde_json::Value = serde_json::from_slice(&p1_bytes).unwrap();
        assert_eq!(p1_json["payments"].as_array().unwrap().len(), 1);

        // Replay exact same Idempotency-Key -> must NOT add a 2nd payment!
        let pay_2 = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/orders/ord-patio-8/pay")
                    .header("authorization", format!("Bearer {owner_tok}"))
                    .header("idempotency-key", "idem-pay-001")
                    .header("content-type", "application/json")
                    .body(Body::from(pay_body_apple))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(pay_2.status(), StatusCode::OK);
        let p2_bytes = axum::body::to_bytes(pay_2.into_body(), usize::MAX)
            .await
            .unwrap();
        let p2_json: serde_json::Value = serde_json::from_slice(&p2_bytes).unwrap();
        assert_eq!(p2_json["payments"].as_array().unwrap().len(), 1);
        let _ = pay_body;

        // Verify Double-Entry Ledger is 100% balanced
        let ledger_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/ledger")
                    .header("authorization", format!("Bearer {owner_tok}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(ledger_resp.status(), StatusCode::OK);
        let l_bytes = axum::body::to_bytes(ledger_resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let l_json: serde_json::Value = serde_json::from_slice(&l_bytes).unwrap();
        assert_eq!(l_json["is_balanced"], true);
        assert!(l_json["total_debits_cents"].as_i64().unwrap() > 0);

        // 5. Verify static SPA hosting + ESC/POS thermal receipt rendering
        let spa_resp = app
            .clone()
            .oneshot(Request::builder().method("GET").uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(spa_resp.status(), StatusCode::OK);

        let print_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/orders/ord-patio-8/print-receipt")
                    .header("authorization", format!("Bearer {owner_tok}"))
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"kick_drawer":true}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(print_resp.status(), StatusCode::OK);
    }
}

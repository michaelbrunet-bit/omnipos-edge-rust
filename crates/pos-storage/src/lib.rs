use std::fs;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use pos_core::{
    AuthError, CatalogItem, CategoryConfig, IndustryVertical, ItemPrepStatus, ModifierOption,
    ModifierQualifier, OperatorAccount, Order, OrderLineItem, OrderSource, OrderStatus, Role,
    StoreProfile, StoreTheme, TaxMode,
};
use pos_ledger::{CashDrawerShift, JournalEntry, ShiftStatus};
use pos_mod_engine::{ModEngine, ModEngineError, ModManifest};
use rand_core::{OsRng, RngCore};
use redb::{backends::InMemoryBackend, Database, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

const GENESIS_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

// ACID B-Tree Tables inside `pos_db.redb`
const META_TABLE: TableDefinition<&str, &str> = TableDefinition::new("meta");
const STORES_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("stores");
const OPERATORS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("operators");
const CATALOG_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("catalog");
const ORDERS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("orders");
const MODS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("mods");
const LEDGER_TABLE: TableDefinition<u64, &[u8]> = TableDefinition::new("journal_entries");
const SHIFTS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("shifts");
const AUDIT_TABLE: TableDefinition<u64, &[u8]> = TableDefinition::new("audit_chain");
const IDEMPOTENCY_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("idempotency_keys");
const OUTBOX_TABLE: TableDefinition<u64, &[u8]> = TableDefinition::new("sync_outbox");

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("redb database error: {0}")]
    Redb(String),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("entity not found: {0}")]
    NotFound(String),
    #[error("authentication error: {0}")]
    Auth(#[from] AuthError),
    #[error("mod engine error: {0}")]
    ModEngine(#[from] ModEngineError),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogEntry {
    pub sequence: u64,
    pub timestamp: DateTime<Utc>,
    pub store_id: String,
    pub actor_name: String,
    pub role: Role,
    pub action: String,
    pub details: String,
    pub prev_hash: String,
    pub entry_hash: String,
}

impl AuditLogEntry {
    pub fn compute_hash(
        sequence: u64,
        timestamp: &DateTime<Utc>,
        store_id: &str,
        actor_name: &str,
        role: Role,
        action: &str,
        details: &str,
        prev_hash: &str,
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(
            format!(
                "{}|{}|{}|{}|{:?}|{}|{}|{}",
                sequence,
                timestamp.to_rfc3339(),
                store_id,
                actor_name,
                role,
                action,
                details,
                prev_hash
            )
            .as_bytes(),
        );
        hex::encode(hasher.finalize())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditVerificationReport {
    pub valid: bool,
    pub total_entries: usize,
    pub head_hash: String,
    pub broken_at_sequence: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdempotencyRecord {
    pub key: String,
    pub status_code: u16,
    pub response_json: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncEvent {
    pub seq: u64,
    pub store_id: String,
    pub event_type: String,
    pub entity_id: String,
    pub payload_json: String,
    pub created_at: DateTime<Utc>,
}

pub struct PosStorage {
    db: Database,
    pub mod_engine: ModEngine,
}

impl PosStorage {
    /// Creates an isolated in-memory ACID `redb` database (ideal for tests and ephemeral sandboxes).
    pub fn new_in_memory() -> Self {
        let backend = InMemoryBackend::new();
        let db = Database::builder()
            .create_with_backend(backend)
            .expect("Failed to create in-memory redb database");
        Self::from_db(db)
    }

    /// Opens or initializes a durable `redb` ACID B-tree database file (`pos_db.redb`) inside `data_dir`.
    pub fn open_or_init(data_dir: impl Into<PathBuf>) -> Self {
        let dir = data_dir.into();
        let _ = fs::create_dir_all(&dir);
        let db_path = dir.join("pos_db.redb");
        let db = Database::create(&db_path).expect("Failed to open or create pos_db.redb");
        Self::from_db(db)
    }

    fn from_db(db: Database) -> Self {
        let already_seeded = {
            let read_txn = db.begin_read().expect("read txn");
            read_txn
                .open_table(META_TABLE)
                .ok()
                .and_then(|t| t.get("active_store_id").ok().flatten().map(|_| true))
                .unwrap_or(false)
        };

        if already_seeded {
            let mods = Self::read_all_mods(&db);
            let mod_engine = if mods.is_empty() {
                ModEngine::new_with_defaults()
            } else {
                ModEngine::from_manifests(mods)
            };
            return Self { db, mod_engine };
        }

        let mut mod_engine = ModEngine::new_with_defaults();
        let stores = StoreProfile::default_stores();
        let operators = OperatorAccount::default_accounts();

        let mut secret_bytes = [0u8; 32];
        OsRng.fill_bytes(&mut secret_bytes);
        let secret_hex = hex::encode(secret_bytes);

        let write_txn = db.begin_write().expect("initial seed write txn");
        {
            let mut meta = write_txn.open_table(META_TABLE).expect("meta table");
            meta.insert("active_store_id", "store-hospitality")
                .expect("insert active_store_id");
            meta.insert("hmac_secret_hex", secret_hex.as_str())
                .expect("insert hmac_secret_hex");

            let mut stores_tbl = write_txn.open_table(STORES_TABLE).expect("stores table");
            let mut catalog_tbl = write_txn.open_table(CATALOG_TABLE).expect("catalog table");
            let mut orders_tbl = write_txn.open_table(ORDERS_TABLE).expect("orders table");
            let mut shifts_tbl = write_txn.open_table(SHIFTS_TABLE).expect("shifts table");
            let mut ops_tbl = write_txn
                .open_table(OPERATORS_TABLE)
                .expect("operators table");
            let mut mods_tbl = write_txn.open_table(MODS_TABLE).expect("mods table");
            let _ = write_txn.open_table(LEDGER_TABLE).expect("ledger table");
            let _ = write_txn
                .open_table(IDEMPOTENCY_TABLE)
                .expect("idempotency table");
            let _ = write_txn.open_table(AUDIT_TABLE).expect("audit table");
            let _ = write_txn.open_table(OUTBOX_TABLE).expect("outbox table");

            for store in &stores {
                let bytes = serde_json::to_vec(store).expect("serialize store");
                stores_tbl
                    .insert(store.store_id.as_str(), bytes.as_slice())
                    .expect("insert store");

                let cat_items = CatalogItem::catalog_for_store(&store.store_id, store.vertical);
                for item in &cat_items {
                    let key = format!("{}:{}", store.store_id, item.id);
                    let item_bytes = serde_json::to_vec(item).expect("serialize item");
                    catalog_tbl
                        .insert(key.as_str(), item_bytes.as_slice())
                        .expect("insert catalog");
                }

                let mut store_orders = match store.vertical {
                    IndustryVertical::Hospitality | IndustryVertical::Custom => {
                        Self::seed_hospitality_orders(&store.store_id)
                    }
                    IndustryVertical::SalonBarber => {
                        Self::seed_salon_barber_orders(&store.store_id)
                    }
                    IndustryVertical::Retail => Self::seed_retail_orders(&store.store_id),
                };
                for order in &mut store_orders {
                    mod_engine.apply_order_hooks(order);
                    let key = format!("{}:{}", store.store_id, order.id);
                    let ord_bytes = serde_json::to_vec(order).expect("serialize order");
                    orders_tbl
                        .insert(key.as_str(), ord_bytes.as_slice())
                        .expect("insert order");
                }

                // Open default $200.00 cash drawer shift per store
                let shift = CashDrawerShift::open_new(
                    &store.store_id,
                    "term-primary-01",
                    "Marcus Cole (Bartender / Cashier)",
                    20_000,
                );
                let shift_key = format!("{}:{}", store.store_id, shift.id);
                let shift_bytes = serde_json::to_vec(&shift).expect("serialize shift");
                shifts_tbl
                    .insert(shift_key.as_str(), shift_bytes.as_slice())
                    .expect("insert shift");
            }

            for op in &operators {
                let op_bytes = serde_json::to_vec(op).expect("serialize operator");
                ops_tbl
                    .insert(op.id.as_str(), op_bytes.as_slice())
                    .expect("insert operator");
            }

            for m in mod_engine.list_mods() {
                let mod_bytes = serde_json::to_vec(m).expect("serialize mod");
                mods_tbl
                    .insert(m.id.as_str(), mod_bytes.as_slice())
                    .expect("insert mod");
            }
        }
        write_txn.commit().expect("commit seed txn");

        let mut storage = Self { db, mod_engine };
        storage.append_audit(
            "System Kernel",
            Role::Owner,
            "POS_REDB_BOOT_INIT",
            "Initialized ACID redb storage, multi-store partitions, Argon2id accounts, Wasmi sandbox & SHA-256 audit chain",
        );
        storage
    }

    fn read_all_mods(db: &Database) -> Vec<ModManifest> {
        let Ok(read_txn) = db.begin_read() else {
            return Vec::new();
        };
        let Ok(table) = read_txn.open_table(MODS_TABLE) else {
            return Vec::new();
        };
        let mut list = Vec::new();
        if let Ok(iter) = table.iter() {
            for item in iter.flatten() {
                if let Ok(m) = serde_json::from_slice::<ModManifest>(item.1.value()) {
                    list.push(m);
                }
            }
        }
        list
    }

    pub fn persisted_hmac_secret(&self) -> Vec<u8> {
        let read_txn = self.db.begin_read().expect("read txn");
        let table = read_txn.open_table(META_TABLE).expect("meta table");
        if let Ok(Some(val)) = table.get("hmac_secret_hex") {
            if let Ok(decoded) = hex::decode(val.value()) {
                if decoded.len() >= 16 {
                    return decoded;
                }
            }
        }
        b"omnipos-fallback-hmac-secret-key".to_vec()
    }

    pub fn active_store_id(&self) -> String {
        let read_txn = self.db.begin_read().expect("read txn");
        let table = read_txn.open_table(META_TABLE).expect("meta table");
        table
            .get("active_store_id")
            .ok()
            .flatten()
            .map(|v| v.value().to_string())
            .unwrap_or_else(|| "store-hospitality".to_string())
    }

    pub fn list_stores(&self) -> Vec<StoreProfile> {
        let read_txn = self.db.begin_read().expect("read txn");
        let table = read_txn.open_table(STORES_TABLE).expect("stores table");
        let mut out = Vec::new();
        if let Ok(iter) = table.iter() {
            for entry in iter.flatten() {
                if let Ok(p) = serde_json::from_slice::<StoreProfile>(entry.1.value()) {
                    out.push(p);
                }
            }
        }
        // Keep canonical ordering: hospitality, salon, retail, then custom
        out.sort_by_key(|s| match s.store_id.as_str() {
            "store-hospitality" => 0,
            "store-salon" => 1,
            "store-retail" => 2,
            _ => 3,
        });
        out
    }

    pub fn get_store(&self, store_id: &str) -> Option<StoreProfile> {
        let read_txn = self.db.begin_read().ok()?;
        let table = read_txn.open_table(STORES_TABLE).ok()?;
        let guard = table.get(store_id).ok()??;
        serde_json::from_slice(guard.value()).ok()
    }

    pub fn active_store(&self) -> StoreProfile {
        let id = self.active_store_id();
        self.get_store(&id)
            .unwrap_or_else(|| StoreProfile::default_stores().into_iter().next().unwrap())
    }

    /// Non-destructive store switching: switches active partition without overwriting any orders or catalog items!
    pub fn activate_store(&mut self, store_id: &str) -> Result<StoreProfile, StorageError> {
        let profile = self
            .get_store(store_id)
            .ok_or_else(|| StorageError::NotFound(format!("store '{store_id}'")))?;

        let write_txn = self
            .db
            .begin_write()
            .map_err(|e| StorageError::Redb(e.to_string()))?;
        {
            let mut meta = write_txn
                .open_table(META_TABLE)
                .map_err(|e| StorageError::Redb(e.to_string()))?;
            meta.insert("active_store_id", store_id)
                .map_err(|e| StorageError::Redb(e.to_string()))?;
        }
        write_txn
            .commit()
            .map_err(|e| StorageError::Redb(e.to_string()))?;

        self.append_audit(
            "Elena Vance (Store Owner)",
            Role::Owner,
            "STORE_PARTITION_ACTIVATED",
            format!(
                "Activated store partition '{}' ({:?}) with zero data loss across tenants",
                profile.store_id, profile.vertical
            ),
        );
        self.record_outbox_event(
            &profile.store_id,
            "store_activated",
            &profile.store_id,
            &serde_json::json!({ "store_id": profile.store_id, "vertical": profile.vertical }),
        );
        Ok(profile)
    }

    /// Maps vertical selection onto non-destructive store partition activation.
    pub fn switch_vertical(&mut self, vertical: IndustryVertical) -> StoreProfile {
        let target_store_id = match vertical {
            IndustryVertical::Hospitality | IndustryVertical::Custom => "store-hospitality",
            IndustryVertical::SalonBarber => "store-salon",
            IndustryVertical::Retail => "store-retail",
        };
        self.activate_store(target_store_id)
            .expect("built-in store partition exists")
    }

    pub fn update_active_theme(&mut self, new_theme: StoreTheme) -> StoreProfile {
        let mut profile = self.active_store();
        profile.theme = new_theme;
        self.save_store_profile(&profile);
        profile
    }

    pub fn add_category_to_active_store(&mut self, category: CategoryConfig) -> StoreProfile {
        let mut profile = self.active_store();
        if let Some(existing) = profile.categories.iter_mut().find(|c| c.id == category.id) {
            *existing = category;
        } else {
            profile.categories.push(category);
        }
        profile.categories.sort_by_key(|c| c.sort_order);
        self.save_store_profile(&profile);
        profile
    }

    pub fn save_store_profile(&mut self, profile: &StoreProfile) {
        let bytes = serde_json::to_vec(profile).expect("serialize store profile");
        let write_txn = self.db.begin_write().expect("write txn");
        {
            let mut tbl = write_txn.open_table(STORES_TABLE).expect("stores table");
            tbl.insert(profile.store_id.as_str(), bytes.as_slice())
                .expect("save store profile");
        }
        write_txn.commit().expect("commit store profile");
    }

    // --- Operators & Argon2id Auth ---

    pub fn list_operators(&self) -> Vec<OperatorAccount> {
        let read_txn = self.db.begin_read().expect("read txn");
        let tbl = read_txn
            .open_table(OPERATORS_TABLE)
            .expect("operators table");
        let mut ops = Vec::new();
        if let Ok(iter) = tbl.iter() {
            for item in iter.flatten() {
                if let Ok(op) = serde_json::from_slice::<OperatorAccount>(item.1.value()) {
                    ops.push(op);
                }
            }
        }
        ops
    }

    /// Authenticates an operator for `role` (or any manager/owner for step-up override) via Argon2id,
    /// persisting failed_attempts and lockout timestamps atomically in `redb`.
    pub fn authenticate_operator_by_role(
        &mut self,
        role: Role,
        raw_pin: &str,
    ) -> Result<OperatorAccount, AuthError> {
        let mut ops = self.list_operators();
        let target_idx = ops
            .iter()
            .position(|op| op.role == role && op.active)
            .ok_or(AuthError::InvalidCredentials)?;

        let result = ops[target_idx].authenticate_pin(raw_pin);
        // Always persist updated lockout / failed_attempts state!
        let updated_op = ops[target_idx].clone();
        self.save_operator(&updated_op);

        result.map(|_| updated_op)
    }

    /// Authenticates a Manager or Owner PIN (`9999` or `1111`) for Step-Up Override tokens.
    pub fn authenticate_manager_override(
        &mut self,
        raw_pin: &str,
    ) -> Result<OperatorAccount, AuthError> {
        let mut ops = self.list_operators();
        // Check active Manager or Owner accounts
        let candidate_indices: Vec<usize> = ops
            .iter()
            .enumerate()
            .filter(|(_, op)| {
                op.active && op.role.has_permission(pos_core::Permission::ApproveManagerOverride)
            })
            .map(|(idx, _)| idx)
            .collect();

        if candidate_indices.is_empty() {
            return Err(AuthError::InvalidCredentials);
        }

        let mut locked_err: Option<AuthError> = None;
        for idx in candidate_indices {
            match ops[idx].authenticate_pin(raw_pin) {
                Ok(()) => {
                    let approved = ops[idx].clone();
                    self.save_operator(&approved);
                    return Ok(approved);
                }
                Err(AuthError::AccountLocked(until)) => {
                    self.save_operator(&ops[idx]);
                    locked_err = Some(AuthError::AccountLocked(until));
                }
                Err(_) => {
                    self.save_operator(&ops[idx]);
                }
            }
        }

        Err(locked_err.unwrap_or(AuthError::InvalidCredentials))
    }

    pub fn save_operator(&mut self, op: &OperatorAccount) {
        let bytes = serde_json::to_vec(op).expect("serialize operator");
        let write_txn = self.db.begin_write().expect("write txn");
        {
            let mut tbl = write_txn
                .open_table(OPERATORS_TABLE)
                .expect("operators table");
            tbl.insert(op.id.as_str(), bytes.as_slice())
                .expect("save operator");
        }
        write_txn.commit().expect("commit operator");
    }

    // --- Catalog ---

    pub fn list_catalog(&self, store_id: &str) -> Vec<CatalogItem> {
        let prefix = format!("{store_id}:");
        let read_txn = self.db.begin_read().expect("read txn");
        let tbl = read_txn.open_table(CATALOG_TABLE).expect("catalog table");
        let mut items = Vec::new();
        if let Ok(iter) = tbl.range(prefix.as_str()..) {
            for entry in iter.flatten() {
                let k = entry.0.value();
                if !k.starts_with(&prefix) {
                    break;
                }
                if let Ok(item) = serde_json::from_slice::<CatalogItem>(entry.1.value()) {
                    items.push(item);
                }
            }
        }
        items
    }

    pub fn get_catalog_item(&self, store_id: &str, item_id: &str) -> Option<CatalogItem> {
        let key = format!("{store_id}:{item_id}");
        let read_txn = self.db.begin_read().ok()?;
        let tbl = read_txn.open_table(CATALOG_TABLE).ok()?;
        let guard = tbl.get(key.as_str()).ok()??;
        serde_json::from_slice(guard.value()).ok()
    }

    pub fn save_catalog_item(&mut self, item: &CatalogItem) {
        let key = format!("{}:{}", item.store_id, item.id);
        let bytes = serde_json::to_vec(item).expect("serialize catalog item");
        let write_txn = self.db.begin_write().expect("write txn");
        {
            let mut tbl = write_txn.open_table(CATALOG_TABLE).expect("catalog table");
            tbl.insert(key.as_str(), bytes.as_slice())
                .expect("insert catalog item");
        }
        write_txn.commit().expect("commit catalog item");
    }

    // --- Orders & Wasm Mod Recalculation ---

    pub fn list_orders(&self, store_id: &str) -> Vec<Order> {
        let prefix = format!("{store_id}:");
        let read_txn = self.db.begin_read().expect("read txn");
        let tbl = read_txn.open_table(ORDERS_TABLE).expect("orders table");
        let mut orders = Vec::new();
        if let Ok(iter) = tbl.range(prefix.as_str()..) {
            for entry in iter.flatten() {
                let k = entry.0.value();
                if !k.starts_with(&prefix) {
                    break;
                }
                if let Ok(ord) = serde_json::from_slice::<Order>(entry.1.value()) {
                    orders.push(ord);
                }
            }
        }
        orders.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        orders
    }

    pub fn get_order(&self, store_id: &str, order_id: &str) -> Option<Order> {
        let key = format!("{store_id}:{order_id}");
        let read_txn = self.db.begin_read().ok()?;
        let tbl = read_txn.open_table(ORDERS_TABLE).ok()?;
        let guard = tbl.get(key.as_str()).ok()??;
        serde_json::from_slice(guard.value()).ok()
    }

    pub fn save_order_with_hooks(&mut self, order: &mut Order) {
        if order.status != OrderStatus::Paid
            && order.status != OrderStatus::Voided
            && order.status != OrderStatus::Refunded
        {
            self.mod_engine.apply_order_hooks(order);
        }
        self.save_order_raw(order);
        self.persist_mods();
    }

    pub fn save_order_raw(&mut self, order: &Order) {
        let key = format!("{}:{}", order.store_id, order.id);
        let bytes = serde_json::to_vec(order).expect("serialize order");
        let write_txn = self.db.begin_write().expect("write txn");
        {
            let mut tbl = write_txn.open_table(ORDERS_TABLE).expect("orders table");
            tbl.insert(key.as_str(), bytes.as_slice())
                .expect("insert order");
        }
        write_txn.commit().expect("commit order");
    }

    pub fn recalculate_store_orders(&mut self, store_id: &str) {
        let mut orders = self.list_orders(store_id);
        for order in &mut orders {
            if order.status != OrderStatus::Paid
                && order.status != OrderStatus::Voided
                && order.status != OrderStatus::Refunded
            {
                self.mod_engine.apply_order_hooks(order);
                self.save_order_raw(order);
            }
        }
        self.persist_mods();
    }

    pub fn persist_mods(&mut self) {
        let mods = self.mod_engine.list_mods().to_vec();
        let write_txn = self.db.begin_write().expect("write txn");
        {
            let mut tbl = write_txn.open_table(MODS_TABLE).expect("mods table");
            for m in &mods {
                let bytes = serde_json::to_vec(m).expect("serialize mod");
                tbl.insert(m.id.as_str(), bytes.as_slice())
                    .expect("save mod");
            }
        }
        write_txn.commit().expect("commit mods");
    }

    // --- Double-Entry Ledger & Cash Drawer Shifts ---

    pub fn append_journal_entry(&mut self, entry: &JournalEntry) {
        let bytes = serde_json::to_vec(entry).expect("serialize journal entry");
        let write_txn = self.db.begin_write().expect("write txn");
        {
            let mut tbl = write_txn.open_table(LEDGER_TABLE).expect("ledger table");
            let next_seq = tbl
                .last()
                .ok()
                .flatten()
                .map(|(k, _)| k.value() + 1)
                .unwrap_or(1);
            tbl.insert(next_seq, bytes.as_slice())
                .expect("insert journal entry");
        }
        write_txn.commit().expect("commit journal entry");
    }

    pub fn list_journal_entries(&self, store_id: &str) -> Vec<JournalEntry> {
        let read_txn = self.db.begin_read().expect("read txn");
        let tbl = read_txn.open_table(LEDGER_TABLE).expect("ledger table");
        let mut entries = Vec::new();
        if let Ok(iter) = tbl.iter() {
            for row in iter.flatten() {
                if let Ok(je) = serde_json::from_slice::<JournalEntry>(row.1.value()) {
                    if je.store_id == store_id {
                        entries.push(je);
                    }
                }
            }
        }
        entries.reverse();
        entries
    }

    pub fn list_shifts(&self, store_id: &str) -> Vec<CashDrawerShift> {
        let prefix = format!("{store_id}:");
        let read_txn = self.db.begin_read().expect("read txn");
        let tbl = read_txn.open_table(SHIFTS_TABLE).expect("shifts table");
        let mut shifts = Vec::new();
        if let Ok(iter) = tbl.range(prefix.as_str()..) {
            for row in iter.flatten() {
                let k = row.0.value();
                if !k.starts_with(&prefix) {
                    break;
                }
                if let Ok(s) = serde_json::from_slice::<CashDrawerShift>(row.1.value()) {
                    shifts.push(s);
                }
            }
        }
        shifts.sort_by(|a, b| b.opened_at.cmp(&a.opened_at));
        shifts
    }

    pub fn current_open_shift(&self, store_id: &str) -> Option<CashDrawerShift> {
        self.list_shifts(store_id)
            .into_iter()
            .find(|s| s.status == ShiftStatus::Open)
    }

    pub fn save_shift(&mut self, shift: &CashDrawerShift) {
        let key = format!("{}:{}", shift.store_id, shift.id);
        let bytes = serde_json::to_vec(shift).expect("serialize shift");
        let write_txn = self.db.begin_write().expect("write txn");
        {
            let mut tbl = write_txn.open_table(SHIFTS_TABLE).expect("shifts table");
            tbl.insert(key.as_str(), bytes.as_slice())
                .expect("save shift");
        }
        write_txn.commit().expect("commit shift");
    }

    // --- Idempotency Keys & Monotonic Sync Outbox ---

    pub fn get_idempotency(&self, scoped_key: &str) -> Option<IdempotencyRecord> {
        let read_txn = self.db.begin_read().ok()?;
        let tbl = read_txn.open_table(IDEMPOTENCY_TABLE).ok()?;
        let guard = tbl.get(scoped_key).ok()??;
        serde_json::from_slice(guard.value()).ok()
    }

    pub fn save_idempotency(&mut self, scoped_key: &str, status_code: u16, response_json: String) {
        let rec = IdempotencyRecord {
            key: scoped_key.to_string(),
            status_code,
            response_json,
            created_at: Utc::now(),
        };
        let bytes = serde_json::to_vec(&rec).expect("serialize idempotency");
        let write_txn = self.db.begin_write().expect("write txn");
        {
            let mut tbl = write_txn
                .open_table(IDEMPOTENCY_TABLE)
                .expect("idempotency table");
            tbl.insert(scoped_key, bytes.as_slice())
                .expect("insert idempotency");
        }
        write_txn.commit().expect("commit idempotency");
    }

    pub fn record_outbox_event(
        &mut self,
        store_id: &str,
        event_type: &str,
        entity_id: &str,
        payload: &serde_json::Value,
    ) -> SyncEvent {
        let write_txn = self.db.begin_write().expect("write txn");
        let event = {
            let mut tbl = write_txn.open_table(OUTBOX_TABLE).expect("outbox table");
            let seq = tbl
                .last()
                .ok()
                .flatten()
                .map(|(k, _)| k.value() + 1)
                .unwrap_or(1);
            let ev = SyncEvent {
                seq,
                store_id: store_id.to_string(),
                event_type: event_type.to_string(),
                entity_id: entity_id.to_string(),
                payload_json: payload.to_string(),
                created_at: Utc::now(),
            };
            let bytes = serde_json::to_vec(&ev).expect("serialize outbox event");
            tbl.insert(seq, bytes.as_slice())
                .expect("insert outbox event");
            ev
        };
        write_txn.commit().expect("commit outbox event");
        event
    }

    pub fn list_outbox_since(&self, since_seq: u64, limit: usize) -> Vec<SyncEvent> {
        let read_txn = self.db.begin_read().expect("read txn");
        let tbl = read_txn.open_table(OUTBOX_TABLE).expect("outbox table");
        let mut events = Vec::new();
        let start = since_seq.saturating_add(1);
        if let Ok(iter) = tbl.range(start..) {
            for row in iter.flatten().take(limit.max(1)) {
                if let Ok(ev) = serde_json::from_slice::<SyncEvent>(row.1.value()) {
                    events.push(ev);
                }
            }
        }
        events
    }

    // --- Tamper-Evident SHA-256 Audit Chain ---

    pub fn append_audit(
        &mut self,
        actor_name: impl Into<String>,
        role: Role,
        action: impl Into<String>,
        details: impl Into<String>,
    ) -> AuditLogEntry {
        let store_id = self.active_store_id();
        let actor_name = actor_name.into();
        let action = action.into();
        let details = details.into();

        let write_txn = self.db.begin_write().expect("write txn");
        let entry = {
            let mut tbl = write_txn.open_table(AUDIT_TABLE).expect("audit table");
            let (sequence, prev_hash) = match tbl.last().ok().flatten() {
                Some((seq_guard, bytes_guard)) => {
                    let prev: AuditLogEntry =
                        serde_json::from_slice(bytes_guard.value()).expect("valid audit entry");
                    (seq_guard.value() + 1, prev.entry_hash)
                }
                None => (1, GENESIS_HASH.to_string()),
            };
            let timestamp = Utc::now();
            let entry_hash = AuditLogEntry::compute_hash(
                sequence,
                &timestamp,
                &store_id,
                &actor_name,
                role,
                &action,
                &details,
                &prev_hash,
            );
            let e = AuditLogEntry {
                sequence,
                timestamp,
                store_id,
                actor_name,
                role,
                action,
                details,
                prev_hash,
                entry_hash,
            };
            let bytes = serde_json::to_vec(&e).expect("serialize audit");
            tbl.insert(sequence, bytes.as_slice())
                .expect("insert audit");
            e
        };
        write_txn.commit().expect("commit audit");
        entry
    }

    pub fn list_audit_entries(&self) -> Vec<AuditLogEntry> {
        let read_txn = self.db.begin_read().expect("read txn");
        let tbl = read_txn.open_table(AUDIT_TABLE).expect("audit table");
        let mut entries = Vec::new();
        if let Ok(iter) = tbl.iter() {
            for row in iter.flatten() {
                if let Ok(e) = serde_json::from_slice::<AuditLogEntry>(row.1.value()) {
                    entries.push(e);
                }
            }
        }
        entries
    }

    pub fn verify_audit_chain(&self) -> AuditVerificationReport {
        let entries = self.list_audit_entries();
        let mut expected_prev = GENESIS_HASH.to_string();

        for entry in &entries {
            if entry.prev_hash != expected_prev {
                return AuditVerificationReport {
                    valid: false,
                    total_entries: entries.len(),
                    head_hash: entry.entry_hash.clone(),
                    broken_at_sequence: Some(entry.sequence),
                };
            }
            let recomputed = AuditLogEntry::compute_hash(
                entry.sequence,
                &entry.timestamp,
                &entry.store_id,
                &entry.actor_name,
                entry.role,
                &entry.action,
                &entry.details,
                &entry.prev_hash,
            );
            if recomputed != entry.entry_hash {
                return AuditVerificationReport {
                    valid: false,
                    total_entries: entries.len(),
                    head_hash: entry.entry_hash.clone(),
                    broken_at_sequence: Some(entry.sequence),
                };
            }
            expected_prev = entry.entry_hash.clone();
        }

        AuditVerificationReport {
            valid: true,
            total_entries: entries.len(),
            head_hash: expected_prev,
            broken_at_sequence: None,
        }
    }

    #[cfg(test)]
    fn tamper_audit_entry_for_test(&mut self, seq: u64, forged_details: &str) {
        let write_txn = self.db.begin_write().unwrap();
        {
            let mut tbl = write_txn.open_table(AUDIT_TABLE).unwrap();
            let mut entry: AuditLogEntry =
                serde_json::from_slice(tbl.get(seq).unwrap().unwrap().value()).unwrap();
            entry.details = forged_details.to_string();
            let bytes = serde_json::to_vec(&entry).unwrap();
            tbl.insert(seq, bytes.as_slice()).unwrap();
        }
        write_txn.commit().unwrap();
    }

    // --- Seed Orders per Store Partition ---

    fn seed_hospitality_orders(store_id: &str) -> Vec<Order> {
        let mut table_4 = Order::new_for_store(
            store_id,
            "ord-table-4",
            "Table 4 (Window Booth)",
            false,
            4,
            "Sofia Chen",
        );
        table_4.status = OrderStatus::SentToPrep;
        table_4.client_profile_notes = Some("Aloha Pivot Seating Active • Booth VIP".to_string());
        table_4.items = vec![
            OrderLineItem {
                id: "line-t4-1".to_string(),
                catalog_item_id: "item-wagyu-smash-burger".to_string(),
                name: "Double Wagyu Smash Burger".to_string(),
                category: "mains".to_string(),
                quantity: 1,
                unit_price_cents: 1950,
                selected_modifiers: vec![ModifierOption {
                    id: "mod-thick-bacon".to_string(),
                    name: "Applewood Bacon".to_string(),
                    price_delta_cents: 300,
                    qualifier: ModifierQualifier::Extra,
                }],
                seat_number: Some(1),
                notes: Some("Medium-rare".to_string()),
                added_by: "Sofia Chen (Server)".to_string(),
                source: OrderSource::ServerTableside,
                prep_station: "kitchen".to_string(),
                prep_status: ItemPrepStatus::Fired,
                tax_rate_bps: 825,
                tax_mode: TaxMode::Exclusive,
                comped: false,
                comp_reason: None,
                comp_approved_by: None,
            },
            OrderLineItem {
                id: "line-t4-2".to_string(),
                catalog_item_id: "item-truffle-fries".to_string(),
                name: "Black Truffle Parmesan Fries".to_string(),
                category: "appetizers".to_string(),
                quantity: 1,
                unit_price_cents: 1150,
                selected_modifiers: vec![ModifierOption {
                    id: "mod-extra-aioli".to_string(),
                    name: "Garlic Aioli".to_string(),
                    price_delta_cents: 150,
                    qualifier: ModifierQualifier::OnSide,
                }],
                seat_number: Some(2),
                notes: Some("Course 1 Shareable".to_string()),
                added_by: "Customer QR (Table 4)".to_string(),
                source: OrderSource::CustomerSelfService,
                prep_station: "kitchen".to_string(),
                prep_status: ItemPrepStatus::Ready,
                tax_rate_bps: 825,
                tax_mode: TaxMode::Exclusive,
                comped: false,
                comp_reason: None,
                comp_approved_by: None,
            },
            OrderLineItem {
                id: "line-t4-3".to_string(),
                catalog_item_id: "item-yuzu-mezcal-margarita".to_string(),
                name: "Spicy Yuzu Mezcalita".to_string(),
                category: "cocktails".to_string(),
                quantity: 2,
                unit_price_cents: 1400,
                selected_modifiers: vec![],
                seat_number: Some(2),
                notes: None,
                added_by: "Customer QR (Table 4)".to_string(),
                source: OrderSource::CustomerSelfService,
                prep_station: "bar".to_string(),
                prep_status: ItemPrepStatus::Served,
                tax_rate_bps: 825,
                tax_mode: TaxMode::Exclusive,
                comped: false,
                comp_reason: None,
                comp_approved_by: None,
            },
        ];

        let mut bar_tab_1 = Order::new_for_store(
            store_id,
            "ord-bar-alex",
            "Bar Seat 3 • Alex M.",
            true,
            2,
            "Marcus Cole",
        );
        bar_tab_1.status = OrderStatus::Open;
        bar_tab_1.preauth_card = Some("Visa ••4821 (Toast Pre-Auth $50)".to_string());
        bar_tab_1.items = vec![
            OrderLineItem {
                id: "line-b1-1".to_string(),
                catalog_item_id: "item-smoked-old-fashioned".to_string(),
                name: "Smoked Bourbon Old Fashioned".to_string(),
                category: "cocktails".to_string(),
                quantity: 2,
                unit_price_cents: 1500,
                selected_modifiers: vec![ModifierOption {
                    id: "mod-large-rock".to_string(),
                    name: "King Cube Ice".to_string(),
                    price_delta_cents: 0,
                    qualifier: ModifierQualifier::Regular,
                }],
                seat_number: Some(1),
                notes: None,
                added_by: "Marcus Cole (Bartender)".to_string(),
                source: OrderSource::BartenderSpeedRail,
                prep_station: "bar".to_string(),
                prep_status: ItemPrepStatus::Served,
                tax_rate_bps: 825,
                tax_mode: TaxMode::Exclusive,
                comped: false,
                comp_reason: None,
                comp_approved_by: None,
            },
            OrderLineItem {
                id: "line-b1-2".to_string(),
                catalog_item_id: "item-hazy-ipa".to_string(),
                name: "Cloudburst Hazy IPA (16oz)".to_string(),
                category: "draft_beer".to_string(),
                quantity: 2,
                unit_price_cents: 850,
                selected_modifiers: vec![],
                seat_number: Some(2),
                notes: None,
                added_by: "Marcus Cole (Bartender)".to_string(),
                source: OrderSource::BartenderSpeedRail,
                prep_station: "bar".to_string(),
                prep_status: ItemPrepStatus::Served,
                tax_rate_bps: 825,
                tax_mode: TaxMode::Exclusive,
                comped: false,
                comp_reason: None,
                comp_approved_by: None,
            },
        ];

        let mut patio_8 = Order::new_for_store(
            store_id,
            "ord-patio-8",
            "Patio Table 8 (Large Party)",
            false,
            6,
            "Sofia Chen",
        );
        patio_8.status = OrderStatus::CheckRequested;
        patio_8.items = vec![
            OrderLineItem {
                id: "line-p8-1".to_string(),
                catalog_item_id: "item-hot-honey-pepperoni-pizza".to_string(),
                name: "Wood-Fired Hot Honey Soppressata Pizza".to_string(),
                category: "mains".to_string(),
                quantity: 3,
                unit_price_cents: 2100,
                selected_modifiers: vec![ModifierOption {
                    id: "mod-burrata-center".to_string(),
                    name: "Fresh Burrata Center".to_string(),
                    price_delta_cents: 450,
                    qualifier: ModifierQualifier::Regular,
                }],
                seat_number: Some(1),
                notes: None,
                added_by: "Sofia Chen (Server)".to_string(),
                source: OrderSource::ServerTableside,
                prep_station: "kitchen".to_string(),
                prep_status: ItemPrepStatus::Served,
                tax_rate_bps: 825,
                tax_mode: TaxMode::Exclusive,
                comped: false,
                comp_reason: None,
                comp_approved_by: None,
            },
            OrderLineItem {
                id: "line-p8-2".to_string(),
                catalog_item_id: "item-pinot-noir".to_string(),
                name: "Willamette Valley Pinot Noir".to_string(),
                category: "wine".to_string(),
                quantity: 1,
                unit_price_cents: 1300,
                selected_modifiers: vec![ModifierOption {
                    id: "mod-wine-bottle".to_string(),
                    name: "Full Bottle (750ml)".to_string(),
                    price_delta_cents: 3500,
                    qualifier: ModifierQualifier::Regular,
                }],
                seat_number: Some(2),
                notes: None,
                added_by: "Sofia Chen (Server)".to_string(),
                source: OrderSource::ServerTableside,
                prep_station: "bar".to_string(),
                prep_status: ItemPrepStatus::Served,
                tax_rate_bps: 825,
                tax_mode: TaxMode::Exclusive,
                comped: false,
                comp_reason: None,
                comp_approved_by: None,
            },
        ];

        vec![table_4, bar_tab_1, patio_8]
    }

    fn seed_salon_barber_orders(store_id: &str) -> Vec<Order> {
        let mut chair_1 = Order::new_for_store(
            store_id,
            "ord-chair-1",
            "Barber Chair 1 • Jordan Vance (2:00 PM)",
            false,
            1,
            "Devon Brooks (Master Barber)",
        );
        chair_1.status = OrderStatus::SentToPrep;
        chair_1.preauth_card = Some("Amex ••3091 (Booking Card on File)".to_string());
        chair_1.client_profile_notes = Some(
            "Formula: #0.5 mid skin fade, #3 on crown, hard razor part left • Tea-tree sensitivity: None"
                .to_string(),
        );
        chair_1.items = vec![
            OrderLineItem {
                id: "line-s1-1".to_string(),
                catalog_item_id: "salon-executive-skin-fade".to_string(),
                name: "Executive Skin Fade & Hot Towel Finish".to_string(),
                category: "haircuts_fades".to_string(),
                quantity: 1,
                unit_price_cents: 5500,
                selected_modifiers: vec![ModifierOption {
                    id: "mod-razor-part".to_string(),
                    name: "Straight-Razor Hard Part".to_string(),
                    price_delta_cents: 1000,
                    qualifier: ModifierQualifier::Regular,
                }],
                seat_number: Some(1),
                notes: Some("45 min • In Chair".to_string()),
                added_by: "Devon Brooks (Barber)".to_string(),
                source: OrderSource::ServerTableside,
                prep_station: "salon_chair".to_string(),
                prep_status: ItemPrepStatus::Fired,
                tax_rate_bps: 0,
                tax_mode: TaxMode::Exempt,
                comped: false,
                comp_reason: None,
                comp_approved_by: None,
            },
            OrderLineItem {
                id: "line-s1-2".to_string(),
                catalog_item_id: "salon-matte-clay-pomade".to_string(),
                name: "Apothecary Matte Clay Pomade (4oz)".to_string(),
                category: "salon_retail".to_string(),
                quantity: 1,
                unit_price_cents: 2800,
                selected_modifiers: vec![],
                seat_number: Some(1),
                notes: Some("Take-Home Retail Bundle".to_string()),
                added_by: "Jordan Vance (Client Kiosk)".to_string(),
                source: OrderSource::CustomerSelfService,
                prep_station: "counter".to_string(),
                prep_status: ItemPrepStatus::Ready,
                tax_rate_bps: 825,
                tax_mode: TaxMode::Exclusive,
                comped: false,
                comp_reason: None,
                comp_approved_by: None,
            },
        ];

        let mut station_3 = Order::new_for_store(
            store_id,
            "ord-salon-3",
            "Color Station 3 • Claire B. (Balayage)",
            false,
            1,
            "Mila Laurent (Senior Colorist)",
        );
        station_3.status = OrderStatus::Open;
        station_3.preauth_card = Some("Visa ••7740 (Deposit & Card on File)".to_string());
        station_3.client_profile_notes = Some(
            "Color Formula: 30vol clay lightener + 9V/9P gloss toner (20 min processing)"
                .to_string(),
        );
        station_3.items = vec![OrderLineItem {
            id: "line-s3-1".to_string(),
            catalog_item_id: "salon-dimensional-balayage".to_string(),
            name: "Full Dimensional Balayage & Gloss Toner".to_string(),
            category: "color_treatments".to_string(),
            quantity: 1,
            unit_price_cents: 19500,
            selected_modifiers: vec![ModifierOption {
                id: "mod-root-shadow".to_string(),
                name: "Root Shadow Smudge".to_string(),
                price_delta_cents: 3500,
                qualifier: ModifierQualifier::Regular,
            }],
            seat_number: Some(1),
            notes: Some("135 min appointment".to_string()),
            added_by: "Mila Laurent (Colorist)".to_string(),
            source: OrderSource::ServerTableside,
            prep_station: "salon_chair".to_string(),
            prep_status: ItemPrepStatus::Fired,
            tax_rate_bps: 0,
            tax_mode: TaxMode::Exempt,
            comped: false,
            comp_reason: None,
            comp_approved_by: None,
        }];

        vec![chair_1, station_3]
    }

    fn seed_retail_orders(store_id: &str) -> Vec<Order> {
        let mut reg_1 = Order::new_for_store(
            store_id,
            "ord-retail-104",
            "Register 1 • Bag #104 (Maya Lin)",
            true,
            1,
            "Lucas Grant (Lead Cashier)",
        );
        reg_1.status = OrderStatus::Open;
        reg_1.client_profile_notes =
            Some("Foundry VIP Member #8841 • Include gift receipt".to_string());
        reg_1.items = vec![
            OrderLineItem {
                id: "line-r1-1".to_string(),
                catalog_item_id: "ret-selvedge-denim-jacket".to_string(),
                name: "14oz Japanese Selvedge Denim Chore Coat".to_string(),
                category: "apparel".to_string(),
                quantity: 1,
                unit_price_cents: 16800,
                selected_modifiers: vec![ModifierOption {
                    id: "mod-size-m".to_string(),
                    name: "Size: Medium".to_string(),
                    price_delta_cents: 0,
                    qualifier: ModifierQualifier::Regular,
                }],
                seat_number: None,
                notes: Some("UPC: 01942500101".to_string()),
                added_by: "Barcode Scanner (Reg 1)".to_string(),
                source: OrderSource::BartenderSpeedRail,
                prep_station: "retail_register".to_string(),
                prep_status: ItemPrepStatus::Served,
                tax_rate_bps: 825,
                tax_mode: TaxMode::Exclusive,
                comped: false,
                comp_reason: None,
                comp_approved_by: None,
            },
            OrderLineItem {
                id: "line-r1-2".to_string(),
                catalog_item_id: "ret-heavyweight-pocket-tee".to_string(),
                name: "9oz Loopwheel Heavyweight Pocket Tee".to_string(),
                category: "apparel".to_string(),
                quantity: 2,
                unit_price_cents: 4800,
                selected_modifiers: vec![ModifierOption {
                    id: "mod-color-slate".to_string(),
                    name: "Color: Washed Slate".to_string(),
                    price_delta_cents: 0,
                    qualifier: ModifierQualifier::Regular,
                }],
                seat_number: None,
                notes: Some("UPC: 01942500102".to_string()),
                added_by: "Barcode Scanner (Reg 1)".to_string(),
                source: OrderSource::BartenderSpeedRail,
                prep_station: "retail_register".to_string(),
                prep_status: ItemPrepStatus::Served,
                tax_rate_bps: 825,
                tax_mode: TaxMode::Exclusive,
                comped: false,
                comp_reason: None,
                comp_approved_by: None,
            },
        ];

        let mut kiosk_2 = Order::new_for_store(
            store_id,
            "ord-retail-kiosk-2",
            "Self-Checkout Kiosk #2 • Walk-In Bag",
            false,
            1,
            "Self-Checkout Host",
        );
        kiosk_2.status = OrderStatus::CheckRequested;
        kiosk_2.items = vec![OrderLineItem {
            id: "line-rk-1".to_string(),
            catalog_item_id: "ret-goodyear-welt-boots".to_string(),
            name: "Horween Chromexcel Service Boots".to_string(),
            category: "footwear".to_string(),
            quantity: 1,
            unit_price_cents: 29500,
            selected_modifiers: vec![ModifierOption {
                id: "mod-cedar-trees".to_string(),
                name: "Add Cedar Boot Trees".to_string(),
                price_delta_cents: 2800,
                qualifier: ModifierQualifier::Regular,
            }],
            seat_number: None,
            notes: Some("UPC: 01942500201".to_string()),
            added_by: "Customer Self-Scan".to_string(),
            source: OrderSource::CustomerSelfService,
            prep_station: "retail_register".to_string(),
            prep_status: ItemPrepStatus::Served,
            tax_rate_bps: 825,
            tax_mode: TaxMode::Exclusive,
            comped: false,
            comp_reason: None,
            comp_approved_by: None,
        }];

        vec![reg_1, kiosk_2]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redb_audit_hash_chain_verification_and_tamper_detection() {
        let mut storage = PosStorage::new_in_memory();
        storage.append_audit(
            "Elena Vance",
            Role::Owner,
            "THEME_UPDATE",
            "Applied preset bistro_elegance",
        );
        storage.append_audit(
            "Marcus Cole",
            Role::Bartender,
            "ORDER_PAY",
            "Paid $42.00 on Bar Seat 3",
        );

        let report = storage.verify_audit_chain();
        assert!(report.valid);
        assert_eq!(report.total_entries, 3);

        storage.tamper_audit_entry_for_test(2, "FORGED ENTRY IN REDB");
        let tampered_report = storage.verify_audit_chain();
        assert!(!tampered_report.valid);
        assert_eq!(tampered_report.broken_at_sequence, Some(2));
    }

    #[test]
    fn test_non_destructive_multi_store_switching_and_outbox() {
        let mut storage = PosStorage::new_in_memory();

        // Create a brand new order in Hospitality
        let mut custom_order = Order::new_for_store(
            "store-hospitality",
            "ord-vip-99",
            "VIP Mezzanine 99",
            false,
            4,
            "Sofia Chen",
        );
        storage.save_order_with_hooks(&mut custom_order);
        assert!(storage
            .get_order("store-hospitality", "ord-vip-99")
            .is_some());

        // Switch to Salon & Barber, then Retail, then back to Hospitality
        let salon = storage.switch_vertical(IndustryVertical::SalonBarber);
        assert_eq!(salon.store_id, "store-salon");
        assert!(!storage.list_orders("store-salon").is_empty());

        let retail = storage.switch_vertical(IndustryVertical::Retail);
        assert_eq!(retail.store_id, "store-retail");
        assert!(!storage.list_orders("store-retail").is_empty());

        let back = storage.switch_vertical(IndustryVertical::Hospitality);
        assert_eq!(back.store_id, "store-hospitality");
        // Verify zero data loss!
        assert!(storage
            .get_order("store-hospitality", "ord-vip-99")
            .is_some());

        // Verify idempotency and outbox cursor pagination
        storage.save_idempotency("pay:key-123", 200, r#"{"ok":true}"#.to_string());
        let idem = storage.get_idempotency("pay:key-123").expect("cached key");
        assert_eq!(idem.status_code, 200);

        let events = storage.list_outbox_since(0, 50);
        assert_eq!(events.len(), 3);
        let events_after_2 = storage.list_outbox_since(2, 50);
        assert_eq!(events_after_2.len(), 1);
        assert_eq!(events_after_2[0].seq, 3);
    }
}

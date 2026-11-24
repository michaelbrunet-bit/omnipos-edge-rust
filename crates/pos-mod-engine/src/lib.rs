use base64::{engine::general_purpose::STANDARD as B64, Engine};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use pos_core::{ModAdjustment, Order};
use pos_mod_sdk::{
    unpack_ptr_len, ModHookResult, ModLineView, ModOrderContext, POS_MOD_ABI_VERSION,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use wasmi::{Config, Engine as WasmEngine, Linker, Module, Store};

/// Deterministic root seed for the built-in Official POS Mod Registry keypair.
/// Third-party modders can also sign with trusted publisher keys registered in the store.
const OFFICIAL_REGISTRY_SEED: [u8; 32] = *b"omnipos-official-wasm-mod-key-v2";

#[derive(Debug, Error)]
pub enum ModEngineError {
    #[error("Mod '{mod_id}' exceeded Wasm CPU fuel quota (limit: {limit})")]
    FuelExhausted { mod_id: String, limit: u64 },
    #[error("Mod '{mod_id}' attempted unauthorized capability: {capability:?}")]
    UnauthorizedCapability {
        mod_id: String,
        capability: ModCapability,
    },
    #[error("Mod '{mod_id}' failed Ed25519 binary signature verification")]
    InvalidSignature { mod_id: String },
    #[error("Mod '{mod_id}' invalid Wasm module or ABI error: {reason}")]
    WasmRuntimeError { mod_id: String, reason: String },
    #[error("Mod '{0}' not found")]
    NotFound(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModCapability {
    PricingDiscount,
    PricingSurcharge,
    ReceiptCustomize,
    LoyaltyPoints,
    StaffCommission,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModHostConfig {
    /// Categories eligible for `eligible_cents` / `eligible_units` passed into the Wasm guest
    #[serde(default)]
    pub target_categories: Vec<String>,
    /// Secondary categories (e.g., salon service categories when paired with retail)
    #[serde(default)]
    pub secondary_categories: Vec<String>,
    pub label_template: String,
    pub receipt_note_template: Option<String>,
    #[serde(default)]
    pub points_per_dollar: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub runtime: String,
    pub wasm_sha256: String,
    /// Base64-encoded real `.wasm` WebAssembly binary (`\0asm\x01\x00\x00\x00...`)
    pub wasm_base64: String,
    /// Ed25519 publisher public key (hex)
    pub publisher_pubkey_hex: String,
    /// Ed25519 signature over `(id | version | capabilities | wasm_bytes)`
    pub ed25519_signature_hex: String,
    pub capabilities: Vec<ModCapability>,
    pub fuel_limit: u64,
    pub memory_limit_kb: u32,
    pub enabled: bool,
    pub host_config: ModHostConfig,
    pub last_fuel_used: u64,
    pub invocations_count: u64,
}

impl ModManifest {
    pub fn has_capability(&self, cap: ModCapability) -> bool {
        self.capabilities.contains(&cap)
    }

    pub fn decode_wasm_bytes(&self) -> Result<Vec<u8>, ModEngineError> {
        let bytes = B64.decode(&self.wasm_base64).map_err(|e| ModEngineError::WasmRuntimeError {
            mod_id: self.id.clone(),
            reason: format!("invalid base64 wasm payload: {e}"),
        })?;
        if bytes.len() < 8 || &bytes[0..4] != b"\0asm" {
            return Err(ModEngineError::WasmRuntimeError {
                mod_id: self.id.clone(),
                reason: "missing WebAssembly '\\0asm' magic header".to_string(),
            });
        }
        Ok(bytes)
    }

    pub fn signing_payload(
        id: &str,
        version: &str,
        capabilities: &[ModCapability],
        wasm_bytes: &[u8],
    ) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"omnipos-mod-sig-v2:");
        buf.extend_from_slice(id.as_bytes());
        buf.push(b':');
        buf.extend_from_slice(version.as_bytes());
        buf.push(b':');
        let caps_json = serde_json::to_vec(capabilities).unwrap_or_default();
        buf.extend_from_slice(&caps_json);
        buf.push(b':');
        buf.extend_from_slice(wasm_bytes);
        buf
    }

    pub fn verify_signature(&self, wasm_bytes: &[u8]) -> Result<(), ModEngineError> {
        let actual_sha = hex::encode(Sha256::digest(wasm_bytes));
        if actual_sha != self.wasm_sha256 {
            return Err(ModEngineError::InvalidSignature {
                mod_id: self.id.clone(),
            });
        }

        let pub_bytes: [u8; 32] = hex::decode(&self.publisher_pubkey_hex)
            .ok()
            .and_then(|v| v.try_into().ok())
            .ok_or_else(|| ModEngineError::InvalidSignature {
                mod_id: self.id.clone(),
            })?;
        let sig_bytes: [u8; 64] = hex::decode(&self.ed25519_signature_hex)
            .ok()
            .and_then(|v| v.try_into().ok())
            .ok_or_else(|| ModEngineError::InvalidSignature {
                mod_id: self.id.clone(),
            })?;

        let verifying_key = VerifyingKey::from_bytes(&pub_bytes).map_err(|_| {
            ModEngineError::InvalidSignature {
                mod_id: self.id.clone(),
            }
        })?;
        let signature = Signature::from_bytes(&sig_bytes);
        let msg = Self::signing_payload(&self.id, &self.version, &self.capabilities, wasm_bytes);

        verifying_key
            .verify_strict(&msg, &signature)
            .map_err(|_| ModEngineError::InvalidSignature {
                mod_id: self.id.clone(),
            })
    }

    pub fn build_signed_from_wat(
        id: &str,
        name: &str,
        version: &str,
        author: &str,
        description: &str,
        capabilities: Vec<ModCapability>,
        fuel_limit: u64,
        memory_limit_kb: u32,
        enabled: bool,
        host_config: ModHostConfig,
        wat_source: &str,
    ) -> Result<Self, ModEngineError> {
        let wasm_bytes = wat::parse_str(wat_source).map_err(|e| ModEngineError::WasmRuntimeError {
            mod_id: id.to_string(),
            reason: format!("WAT compilation failed: {e}"),
        })?;
        let wasm_sha256 = hex::encode(Sha256::digest(&wasm_bytes));
        let wasm_base64 = B64.encode(&wasm_bytes);

        let signing_key = SigningKey::from_bytes(&OFFICIAL_REGISTRY_SEED);
        let pubkey_hex = hex::encode(signing_key.verifying_key().to_bytes());
        let msg = Self::signing_payload(id, version, &capabilities, &wasm_bytes);
        let sig = signing_key.sign(&msg);

        Ok(Self {
            id: id.to_string(),
            name: name.to_string(),
            version: version.to_string(),
            author: author.to_string(),
            description: description.to_string(),
            runtime: "wasmi-0.40 (WebAssembly Bytecode + Ed25519 Signed)".to_string(),
            wasm_sha256,
            wasm_base64,
            publisher_pubkey_hex: pubkey_hex,
            ed25519_signature_hex: hex::encode(sig.to_bytes()),
            capabilities,
            fuel_limit,
            memory_limit_kb,
            enabled,
            host_config,
            last_fuel_used: 0,
            invocations_count: 0,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookExecutionReport {
    pub mod_id: String,
    pub mod_name: String,
    pub fuel_used: u64,
    pub fuel_limit: u64,
    pub applied_adjustment: Option<ModAdjustment>,
}

pub struct ModEngine {
    engine: WasmEngine,
    mods: Vec<ModManifest>,
}

impl Clone for ModEngine {
    fn clone(&self) -> Self {
        Self::from_manifests(self.mods.clone())
    }
}

impl Default for ModEngine {
    fn default() -> Self {
        Self::new_with_defaults()
    }
}

impl std::fmt::Debug for ModEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModEngine")
            .field("mods_count", &self.mods.len())
            .finish()
    }
}

impl ModEngine {
    fn create_wasm_engine() -> WasmEngine {
        let mut config = Config::default();
        config.consume_fuel(true);
        WasmEngine::new(&config)
    }

    pub fn new_with_defaults() -> Self {
        Self {
            engine: Self::create_wasm_engine(),
            mods: Self::default_mods(),
        }
    }

    pub fn from_manifests(mods: Vec<ModManifest>) -> Self {
        Self {
            engine: Self::create_wasm_engine(),
            mods,
        }
    }

    pub fn list_mods(&self) -> &[ModManifest] {
        &self.mods
    }

    pub fn set_mod_enabled(
        &mut self,
        mod_id: &str,
        enabled: bool,
    ) -> Result<ModManifest, ModEngineError> {
        let m = self
            .mods
            .iter_mut()
            .find(|m| m.id == mod_id)
            .ok_or_else(|| ModEngineError::NotFound(mod_id.to_string()))?;
        m.enabled = enabled;
        Ok(m.clone())
    }

    /// Verifies the Ed25519 signature and Wasm module validity before registering!
    pub fn register_mod(&mut self, manifest: ModManifest) -> Result<ModManifest, ModEngineError> {
        let wasm_bytes = manifest.decode_wasm_bytes()?;
        manifest.verify_signature(&wasm_bytes)?;
        let _ = Module::new(&self.engine, &wasm_bytes).map_err(|e| {
            ModEngineError::WasmRuntimeError {
                mod_id: manifest.id.clone(),
                reason: e.to_string(),
            }
        })?;

        if let Some(existing) = self.mods.iter_mut().find(|m| m.id == manifest.id) {
            *existing = manifest.clone();
        } else {
            self.mods.push(manifest.clone());
        }
        Ok(manifest)
    }

    pub fn apply_order_hooks(&mut self, order: &mut Order) -> Vec<HookExecutionReport> {
        order.mod_adjustments.clear();
        order.receipt_notes.clear();

        let mut reports = Vec::new();

        for idx in 0..self.mods.len() {
            if !self.mods[idx].enabled {
                continue;
            }
            let manifest_snapshot = self.mods[idx].clone();
            match self.execute_single_wasm_mod(&manifest_snapshot, order) {
                Ok(report) => {
                    self.mods[idx].last_fuel_used = report.fuel_used;
                    self.mods[idx].invocations_count += 1;
                    if let Some(adj) = &report.applied_adjustment {
                        if let Some(note) = &adj.receipt_note {
                            order.receipt_notes.push(note.clone());
                        }
                        order.mod_adjustments.push(adj.clone());
                    }
                    reports.push(report);
                }
                Err(_err) => {
                    // Fault isolation: skip trapped/unauthorized mod without freezing checkout!
                    continue;
                }
            }
        }

        reports
    }

    pub fn execute_single_wasm_mod(
        &self,
        manifest: &ModManifest,
        order: &Order,
    ) -> Result<HookExecutionReport, ModEngineError> {
        let wasm_bytes = manifest.decode_wasm_bytes()?;
        manifest.verify_signature(&wasm_bytes)?;

        let module = Module::new(&self.engine, &wasm_bytes).map_err(|e| {
            ModEngineError::WasmRuntimeError {
                mod_id: manifest.id.clone(),
                reason: e.to_string(),
            }
        })?;

        let mut store = Store::new(&self.engine, ());
        store
            .set_fuel(manifest.fuel_limit)
            .map_err(|e| ModEngineError::WasmRuntimeError {
                mod_id: manifest.id.clone(),
                reason: e.to_string(),
            })?;

        let linker = Linker::<()>::new(&self.engine);
        let instance = linker
            .instantiate(&mut store, &module)
            .map_err(|e| ModEngineError::WasmRuntimeError {
                mod_id: manifest.id.clone(),
                reason: e.to_string(),
            })?
            .start(&mut store)
            .map_err(|_| ModEngineError::FuelExhausted {
                mod_id: manifest.id.clone(),
                limit: manifest.fuel_limit,
            })?;

        // Check ABI version export
        let abi_fn = instance
            .get_typed_func::<(), i32>(&store, "pos_abi_version")
            .map_err(|e| ModEngineError::WasmRuntimeError {
                mod_id: manifest.id.clone(),
                reason: format!("missing pos_abi_version export: {e}"),
            })?;
        let abi_ver = abi_fn
            .call(&mut store, ())
            .map_err(|_| ModEngineError::FuelExhausted {
                mod_id: manifest.id.clone(),
                limit: manifest.fuel_limit,
            })?;
        if abi_ver != POS_MOD_ABI_VERSION {
            return Err(ModEngineError::WasmRuntimeError {
                mod_id: manifest.id.clone(),
                reason: format!("unsupported ABI version {abi_ver}"),
            });
        }

        // Mode A: Full JSON Linear-Memory Guest SDK (`alloc` + `on_order_calculate_json`)
        if let (Ok(alloc_fn), Ok(json_fn), Some(memory)) = (
            instance.get_typed_func::<i32, i32>(&store, "alloc"),
            instance.get_typed_func::<(i32, i32), i64>(&store, "on_order_calculate_json"),
            instance.get_memory(&store, "memory"),
        ) {
            let ctx = ModOrderContext {
                abi_version: POS_MOD_ABI_VERSION,
                store_id: order.store_id.clone(),
                order_id: order.id.clone(),
                guest_count: order.guest_count,
                subtotal_cents: order.subtotal_cents(),
                lines: order
                    .items
                    .iter()
                    .filter(|i| !i.comped)
                    .map(|i| ModLineView {
                        catalog_item_id: i.catalog_item_id.clone(),
                        category: i.category.clone(),
                        quantity: i.quantity,
                        line_subtotal_cents: i.line_subtotal_cents(),
                    })
                    .collect(),
            };
            let input_bytes = serde_json::to_vec(&ctx).unwrap_or_default();
            let in_ptr = alloc_fn
                .call(&mut store, input_bytes.len() as i32)
                .map_err(|_| ModEngineError::FuelExhausted {
                    mod_id: manifest.id.clone(),
                    limit: manifest.fuel_limit,
                })?;
            memory
                .write(&mut store, in_ptr as usize, &input_bytes)
                .map_err(|e| ModEngineError::WasmRuntimeError {
                    mod_id: manifest.id.clone(),
                    reason: e.to_string(),
                })?;
            let packed = json_fn
                .call(&mut store, (in_ptr, input_bytes.len() as i32))
                .map_err(|_| ModEngineError::FuelExhausted {
                    mod_id: manifest.id.clone(),
                    limit: manifest.fuel_limit,
                })?;
            let (out_ptr, out_len) = unpack_ptr_len(packed);
            let mut out_buf = vec![0u8; out_len as usize];
            memory
                .read(&store, out_ptr as usize, &mut out_buf)
                .map_err(|e| ModEngineError::WasmRuntimeError {
                    mod_id: manifest.id.clone(),
                    reason: e.to_string(),
                })?;
            let parsed: ModHookResult =
                serde_json::from_slice(&out_buf).map_err(|e| ModEngineError::WasmRuntimeError {
                    mod_id: manifest.id.clone(),
                    reason: e.to_string(),
                })?;

            Self::enforce_capabilities(manifest, parsed.amount_cents, parsed.staff_commission_cents)?;
            let remaining = store.get_fuel().unwrap_or(0);
            let fuel_used = manifest.fuel_limit.saturating_sub(remaining);

            let adj = if parsed.amount_cents != 0 || parsed.receipt_note.is_some() {
                Some(ModAdjustment {
                    mod_id: manifest.id.clone(),
                    mod_name: manifest.name.clone(),
                    label: parsed.label,
                    amount_cents: parsed.amount_cents,
                    receipt_note: if manifest.has_capability(ModCapability::ReceiptCustomize) {
                        parsed.receipt_note
                    } else {
                        None
                    },
                    target_categories: parsed.target_categories,
                    taxable_surcharge_bps: 0,
                })
            } else {
                None
            };

            return Ok(HookExecutionReport {
                mod_id: manifest.id.clone(),
                mod_name: manifest.name.clone(),
                fuel_used,
                fuel_limit: manifest.fuel_limit,
                applied_adjustment: adj,
            });
        }

        // Mode B: Compact Scalar Wasm ABI (`on_order_calculate(subtotal, eligible, secondary_or_guests, units) -> i64`)
        let calc_fn = instance
            .get_typed_func::<(i64, i64, i64, i32), i64>(&store, "on_order_calculate")
            .map_err(|e| ModEngineError::WasmRuntimeError {
                mod_id: manifest.id.clone(),
                reason: format!("missing on_order_calculate export: {e}"),
            })?;

        let subtotal = order.subtotal_cents();
        let eligible_cents: i64 = if manifest.host_config.target_categories.is_empty() {
            subtotal
        } else {
            order
                .items
                .iter()
                .filter(|i| {
                    !i.comped && manifest.host_config.target_categories.contains(&i.category)
                })
                .map(|i| i.line_subtotal_cents())
                .sum()
        };

        let eligible_units: u32 = if manifest.host_config.target_categories.is_empty() {
            order.items.iter().filter(|i| !i.comped).map(|i| i.quantity).sum()
        } else {
            order
                .items
                .iter()
                .filter(|i| {
                    !i.comped && manifest.host_config.target_categories.contains(&i.category)
                })
                .map(|i| i.quantity)
                .sum()
        };

        let secondary_cents_or_guests: i64 =
            if !manifest.host_config.secondary_categories.is_empty() {
                order
                    .items
                    .iter()
                    .filter(|i| {
                        !i.comped
                            && manifest
                                .host_config
                                .secondary_categories
                                .contains(&i.category)
                    })
                    .map(|i| i.line_subtotal_cents())
                    .sum()
            } else {
                i64::from(order.guest_count)
            };

        let amount_cents = calc_fn
            .call(
                &mut store,
                (
                    subtotal,
                    eligible_cents,
                    secondary_cents_or_guests,
                    eligible_units as i32,
                ),
            )
            .map_err(|_| ModEngineError::FuelExhausted {
                mod_id: manifest.id.clone(),
                limit: manifest.fuel_limit,
            })?;

        // Optional Wasm export for staff commission calculation
        let mut commission_cents = 0i64;
        if let Ok(comm_fn) =
            instance.get_typed_func::<(i64, i64), i64>(&store, "calculate_commission")
        {
            let net_retail = (eligible_cents + amount_cents).max(0);
            commission_cents = comm_fn
                .call(&mut store, (secondary_cents_or_guests, net_retail))
                .map_err(|_| ModEngineError::FuelExhausted {
                    mod_id: manifest.id.clone(),
                    limit: manifest.fuel_limit,
                })?;
        }

        Self::enforce_capabilities(manifest, amount_cents, commission_cents)?;

        let remaining = store.get_fuel().unwrap_or(0);
        let fuel_used = manifest.fuel_limit.saturating_sub(remaining);

        let applied_adjustment = if amount_cents != 0 || commission_cents > 0 {
            let mut label = manifest
                .host_config
                .label_template
                .replace("{units}", &eligible_units.to_string());
            if commission_cents > 0 {
                if amount_cents < 0 {
                    label = format!(
                        "{label} • Stylist Comm: ${:.2}",
                        commission_cents as f64 / 100.0
                    );
                } else {
                    label = format!(
                        "Stylist Commission (45% Svc / 10% Ret: ${:.2})",
                        commission_cents as f64 / 100.0
                    );
                }
            }

            let note = if manifest.has_capability(ModCapability::ReceiptCustomize) {
                if commission_cents > 0 {
                    Some(format!(
                        "Stylist '{}' Commission Allocated: ${:.2}",
                        order.assigned_operator,
                        commission_cents as f64 / 100.0
                    ))
                } else {
                    manifest.host_config.receipt_note_template.clone()
                }
            } else {
                None
            };

            Some(ModAdjustment {
                mod_id: manifest.id.clone(),
                mod_name: manifest.name.clone(),
                label,
                amount_cents,
                receipt_note: note,
                target_categories: manifest.host_config.target_categories.clone(),
                taxable_surcharge_bps: 0,
            })
        } else if subtotal > 0
            && manifest.host_config.points_per_dollar > 0
            && manifest.has_capability(ModCapability::LoyaltyPoints)
            && manifest.has_capability(ModCapability::ReceiptCustomize)
        {
            let pts = (subtotal / 100) * i64::from(manifest.host_config.points_per_dollar);
            Some(ModAdjustment {
                mod_id: manifest.id.clone(),
                mod_name: manifest.name.clone(),
                label: format!("Loyalty Points (+{pts} pts)"),
                amount_cents: 0,
                receipt_note: Some(format!("Loyalty Program: This ticket earns {pts} points!")),
                target_categories: vec![],
                taxable_surcharge_bps: 0,
            })
        } else {
            None
        };

        Ok(HookExecutionReport {
            mod_id: manifest.id.clone(),
            mod_name: manifest.name.clone(),
            fuel_used,
            fuel_limit: manifest.fuel_limit,
            applied_adjustment,
        })
    }

    fn enforce_capabilities(
        manifest: &ModManifest,
        amount_cents: i64,
        commission_cents: i64,
    ) -> Result<(), ModEngineError> {
        if amount_cents < 0 && !manifest.has_capability(ModCapability::PricingDiscount) {
            return Err(ModEngineError::UnauthorizedCapability {
                mod_id: manifest.id.clone(),
                capability: ModCapability::PricingDiscount,
            });
        }
        if amount_cents > 0 && !manifest.has_capability(ModCapability::PricingSurcharge) {
            return Err(ModEngineError::UnauthorizedCapability {
                mod_id: manifest.id.clone(),
                capability: ModCapability::PricingSurcharge,
            });
        }
        if commission_cents > 0 && !manifest.has_capability(ModCapability::StaffCommission) {
            return Err(ModEngineError::UnauthorizedCapability {
                mod_id: manifest.id.clone(),
                capability: ModCapability::StaffCommission,
            });
        }
        Ok(())
    }

    pub fn default_mods() -> Vec<ModManifest> {
        // 1. Happy Hour 15% Off Craft Drinks (.wasm compiled from WAT)
        let wat_happy_hour = r#"
            (module
                (memory (export "memory") 1)
                (func (export "pos_abi_version") (result i32) i32.const 1)
                (func (export "on_order_calculate")
                    (param $subtotal i64) (param $eligible i64) (param $aux i64) (param $units i32)
                    (result i64)
                    (if (result i64) (i64.gt_s (local.get $eligible) (i64.const 0))
                        (then
                            ;; -((eligible * 1500 + 5000) / 10000)
                            (i64.sub
                                (i64.const 0)
                                (i64.div_s
                                    (i64.add (i64.mul (local.get $eligible) (i64.const 1500)) (i64.const 5000))
                                    (i64.const 10000)
                                )
                            )
                        )
                        (else (i64.const 0))
                    )
                )
            )
        "#;

        // 2. Large Party 18% Auto-Gratuity for 6+ Guests (.wasm)
        let wat_large_party = r#"
            (module
                (memory (export "memory") 1)
                (func (export "pos_abi_version") (result i32) i32.const 1)
                (func (export "on_order_calculate")
                    (param $subtotal i64) (param $eligible i64) (param $guests i64) (param $units i32)
                    (result i64)
                    (if (result i64)
                        (i32.and
                            (i64.ge_s (local.get $guests) (i64.const 6))
                            (i64.gt_s (local.get $subtotal) (i64.const 0))
                        )
                        (then
                            (i64.div_s
                                (i64.add (i64.mul (local.get $subtotal) (i64.const 1800)) (i64.const 5000))
                                (i64.const 10000)
                            )
                        )
                        (else (i64.const 0))
                    )
                )
            )
        "#;

        // 3. Barber & Salon 15% Retail Bundle + 45% Service / 10% Retail Stylist Commission (.wasm)
        let wat_salon_bundle = r#"
            (module
                (memory (export "memory") 1)
                (func (export "pos_abi_version") (result i32) i32.const 1)
                (func (export "on_order_calculate")
                    (param $subtotal i64) (param $retail i64) (param $service i64) (param $units i32)
                    (result i64)
                    (if (result i64)
                        (i32.and
                            (i64.gt_s (local.get $service) (i64.const 0))
                            (i64.gt_s (local.get $retail) (i64.const 0))
                        )
                        (then
                            (i64.sub
                                (i64.const 0)
                                (i64.div_s
                                    (i64.add (i64.mul (local.get $retail) (i64.const 1500)) (i64.const 5000))
                                    (i64.const 10000)
                                )
                            )
                        )
                        (else (i64.const 0))
                    )
                )
                (func (export "calculate_commission")
                    (param $service i64) (param $net_retail i64) (result i64)
                    (i64.div_s
                        (i64.add
                            (i64.add
                                (i64.mul (local.get $service) (i64.const 4500))
                                (i64.mul (local.get $net_retail) (i64.const 1000))
                            )
                            (i64.const 5000)
                        )
                        (i64.const 10000)
                    )
                )
            )
        "#;

        // 4. Retail Buy 3+ Units Save 10% Bundle (.wasm)
        let wat_retail_bundle = r#"
            (module
                (memory (export "memory") 1)
                (func (export "pos_abi_version") (result i32) i32.const 1)
                (func (export "on_order_calculate")
                    (param $subtotal i64) (param $retail i64) (param $aux i64) (param $units i32)
                    (result i64)
                    (if (result i64)
                        (i32.and
                            (i32.ge_s (local.get $units) (i32.const 3))
                            (i64.gt_s (local.get $retail) (i64.const 0))
                        )
                        (then
                            (i64.sub
                                (i64.const 0)
                                (i64.div_s
                                    (i64.add (i64.mul (local.get $retail) (i64.const 1000)) (i64.const 5000))
                                    (i64.const 10000)
                                )
                            )
                        )
                        (else (i64.const 0))
                    )
                )
            )
        "#;

        // 5. Cross-Vertical Loyalty $5 Off $50+ (.wasm)
        let wat_loyalty = r#"
            (module
                (memory (export "memory") 1)
                (func (export "pos_abi_version") (result i32) i32.const 1)
                (func (export "on_order_calculate")
                    (param $subtotal i64) (param $eligible i64) (param $aux i64) (param $units i32)
                    (result i64)
                    (if (result i64) (i64.ge_s (local.get $subtotal) (i64.const 5000))
                        (then (i64.const -500))
                        (else (i64.const 0))
                    )
                )
            )
        "#;

        vec![
            ModManifest::build_signed_from_wat(
                "mod-happy-hour-craft",
                "Hospitality • Craft Taproom Happy Hour (15% Off Drinks)",
                "2.0.0",
                "POS Official Mods",
                "Compiled WebAssembly (.wasm) plugin: applies 15% off Craft Cocktails and Draft Beers.",
                vec![ModCapability::PricingDiscount, ModCapability::ReceiptCustomize],
                50_000,
                2048,
                true,
                ModHostConfig {
                    target_categories: vec!["cocktails".to_string(), "draft_beer".to_string()],
                    secondary_categories: vec![],
                    label_template: "Happy Hour (15% Off Craft Drinks)".to_string(),
                    receipt_note_template: Some("Happy Hour Applied: Saved 15% on craft drinks!".to_string()),
                    points_per_dollar: 0,
                },
                wat_happy_hour,
            )
            .expect("valid happy hour wasm"),
            ModManifest::build_signed_from_wat(
                "mod-large-party-gratuity",
                "Hospitality • Large Party Auto-Gratuity (18% for 6+ Guests)",
                "2.0.0",
                "Hospitality Ops Pack",
                "Compiled WebAssembly (.wasm) plugin: applies 18% service charge on tables of 6+ guests.",
                vec![ModCapability::PricingSurcharge, ModCapability::ReceiptCustomize],
                50_000,
                1024,
                true,
                ModHostConfig {
                    target_categories: vec![],
                    secondary_categories: vec![],
                    label_template: "Large Party Service Charge (18%)".to_string(),
                    receipt_note_template: Some("Includes 18% service charge for parties of 6+.".to_string()),
                    points_per_dollar: 0,
                },
                wat_large_party,
            )
            .expect("valid large party wasm"),
            ModManifest::build_signed_from_wat(
                "mod-salon-bundle-commission",
                "Barber & Salon • Service + Take-Home Bundle & Stylist Commission",
                "2.0.0",
                "Salon & Grooming Collective",
                "Compiled WebAssembly (.wasm) plugin: grants 15% off take-home products with any service & calculates 45% service + 10% retail stylist commission.",
                vec![
                    ModCapability::PricingDiscount,
                    ModCapability::ReceiptCustomize,
                    ModCapability::StaffCommission,
                ],
                50_000,
                2048,
                true,
                ModHostConfig {
                    target_categories: vec!["salon_retail".to_string()],
                    secondary_categories: vec![
                        "haircuts_fades".to_string(),
                        "beard_grooming".to_string(),
                        "color_treatments".to_string(),
                    ],
                    label_template: "Salon Care Bundle (15% Off Retail)".to_string(),
                    receipt_note_template: None,
                    points_per_dollar: 0,
                },
                wat_salon_bundle,
            )
            .expect("valid salon wasm"),
            ModManifest::build_signed_from_wat(
                "mod-retail-multi-item-bundle",
                "Retail Store • Buy 3+ Units Save 10% Bundle",
                "2.0.0",
                "Retail Commerce Suite",
                "Compiled WebAssembly (.wasm) plugin: applies 10% off when a retail bag reaches 3+ units.",
                vec![ModCapability::PricingDiscount, ModCapability::ReceiptCustomize],
                50_000,
                1536,
                true,
                ModHostConfig {
                    target_categories: vec![
                        "apparel".to_string(),
                        "footwear".to_string(),
                        "accessories".to_string(),
                        "home_goods".to_string(),
                    ],
                    secondary_categories: vec![],
                    label_template: "Retail 3+ Item Bundle (-10% on {units} units)".to_string(),
                    receipt_note_template: Some("Retail Multi-Unit Bundle Applied: Saved 10%!".to_string()),
                    points_per_dollar: 0,
                },
                wat_retail_bundle,
            )
            .expect("valid retail wasm"),
            ModManifest::build_signed_from_wat(
                "mod-craft-loyalty-rewards",
                "Cross-Vertical • Spend & Save Loyalty ($5 Off $50+)",
                "2.1.0",
                "Merchant Loyalty Lab",
                "Compiled WebAssembly (.wasm) plugin: awards 2x loyalty points per dollar and deducts $5.00 at $50+.",
                vec![
                    ModCapability::PricingDiscount,
                    ModCapability::ReceiptCustomize,
                    ModCapability::LoyaltyPoints,
                ],
                50_000,
                2048,
                false,
                ModHostConfig {
                    target_categories: vec![],
                    secondary_categories: vec![],
                    label_template: "VIP $50+ Tier Reward (-$5.00)".to_string(),
                    receipt_note_template: Some("VIP Loyalty: Redeemed $5.00 reward!".to_string()),
                    points_per_dollar: 2,
                },
                wat_loyalty,
            )
            .expect("valid loyalty wasm"),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pos_core::{ItemPrepStatus, OrderLineItem, OrderSource, TaxMode};

    #[test]
    fn test_real_wasm_happy_hour_execution_and_fuel_metering() {
        let mut engine = ModEngine::new_with_defaults();
        let mut order = Order::new("ord-test", "Bar Tab 1", true, 2, "Marcus Cole");
        order.items.push(OrderLineItem {
            id: "line-1".to_string(),
            catalog_item_id: "item-smoked-old-fashioned".to_string(),
            name: "Smoked Bourbon Old Fashioned".to_string(),
            category: "cocktails".to_string(),
            quantity: 2,
            unit_price_cents: 1500,
            selected_modifiers: vec![],
            seat_number: None,
            notes: None,
            added_by: "Marcus".to_string(),
            source: OrderSource::BartenderSpeedRail,
            prep_station: "bar".to_string(),
            prep_status: ItemPrepStatus::Fired,
            tax_rate_bps: 825,
            tax_mode: TaxMode::Exclusive,
            comped: false,
            comp_reason: None,
            comp_approved_by: None,
        });

        let reports = engine.apply_order_hooks(&mut order);
        assert!(!reports.is_empty());
        assert!(reports[0].fuel_used > 0);
        assert_eq!(order.subtotal_cents(), 3000);
        assert_eq!(order.adjustments_total_cents(), -450);
    }

    #[test]
    fn test_ed25519_signature_tamper_rejection() {
        let engine = ModEngine::new_with_defaults();
        let mut tampered = engine.list_mods()[0].clone();
        // Escalate capability without re-signing!
        tampered.capabilities.push(ModCapability::PricingSurcharge);
        let order = Order::new("ord-1", "Tab", true, 1, "Marcus");
        let res = engine.execute_single_wasm_mod(&tampered, &order);
        assert!(matches!(res, Err(ModEngineError::InvalidSignature { .. })));
    }

    #[test]
    fn test_infinite_loop_wasm_traps_on_fuel_exhaustion() {
        let runaway_wat = r#"
            (module
                (memory (export "memory") 1)
                (func (export "pos_abi_version") (result i32) i32.const 1)
                (func (export "on_order_calculate")
                    (param i64) (param i64) (param i64) (param i32) (result i64)
                    (loop $inf
                        br $inf
                    )
                    i64.const 0
                )
            )
        "#;

        let runaway_manifest = ModManifest::build_signed_from_wat(
            "mod-infinite-loop",
            "Runaway Mod",
            "0.1.0",
            "Test",
            "Infinite loop test",
            vec![ModCapability::PricingDiscount],
            5_000,
            1024,
            true,
            ModHostConfig {
                target_categories: vec![],
                secondary_categories: vec![],
                label_template: "Never Reached".to_string(),
                receipt_note_template: None,
                points_per_dollar: 0,
            },
            runaway_wat,
        )
        .unwrap();

        let engine = ModEngine::from_manifests(vec![runaway_manifest.clone()]);
        let order = Order::new("ord-1", "Tab", true, 1, "Marcus");
        let res = engine.execute_single_wasm_mod(&runaway_manifest, &order);
        assert!(matches!(res, Err(ModEngineError::FuelExhausted { .. })));
    }
}

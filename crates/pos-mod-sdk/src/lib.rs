use serde::{Deserialize, Serialize};

pub const POS_MOD_ABI_VERSION: i32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModLineView {
    pub catalog_item_id: String,
    pub category: String,
    pub quantity: u32,
    pub line_subtotal_cents: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModOrderContext {
    pub abi_version: i32,
    pub store_id: String,
    pub order_id: String,
    pub guest_count: u8,
    pub subtotal_cents: i64,
    pub lines: Vec<ModLineView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModHookResult {
    pub amount_cents: i64,
    pub label: String,
    pub receipt_note: Option<String>,
    pub target_categories: Vec<String>,
    pub staff_commission_cents: i64,
}

/// Packs `(ptr, len)` into a single 64-bit WebAssembly return value `(ptr << 32) | len`.
#[inline]
pub const fn pack_ptr_len(ptr: u32, len: u32) -> i64 {
    ((ptr as i64) << 32) | (len as i64)
}

/// Unpacks `(ptr, len)` from a 64-bit WebAssembly return value.
#[inline]
pub const fn unpack_ptr_len(packed: i64) -> (u32, u32) {
    let ptr = ((packed as u64) >> 32) as u32;
    let len = (packed as u64 & 0xFFFF_FFFF) as u32;
    (ptr, len)
}

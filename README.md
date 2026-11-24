# OmniPOS Edge — Cross-Platform Pure-Rust POS & WebAssembly Mod Runtime (`v0.2.0`)

**License:** MIT | **Stack:** 7-Crate Pure-Rust Workspace (`Axum`, `redb`, `wasmi` `0.40`, `ed25519-dalek`, `argon2`) + Tauri v2 + React / TypeScript

> A local-first, cross-platform Point-of-Sale edge runtime designed for high-reliability hospitality, salon, and retail environments featuring ACID embedded storage, double-entry shift accounting, tamper-evident audit chains, and a cryptographically signed WebAssembly plugin sandbox.

---

## Key Engineering Highlights
- **7-Crate Pure-Rust Architecture:** Decoupled domain logic (`pos-core`), double-entry accounting (`pos-ledger`), virtual hardware abstraction (`pos-hal`), guest/host WASM ABI (`pos-mod-sdk`), sandboxed runtime (`pos-mod-engine`), ACID B-tree storage (`pos-storage`), and Axum REST + WebSocket edge hub (`pos-server`).
- **Cryptographically Sandboxed Plugins (`wasmi` + `ed25519-dalek`):** Compiles and executes real `.wasm` guest mods with deterministic CPU fuel metering, linear-memory bounds, and Ed25519 signature verification.
- **Double-Entry Financial Integrity:** `JournalEntry::new_checked` enforces balanced debits and credits across sales, taxes, tips, comps, discounts, refunds, and X/Z cash-drawer shift reconciliations.
- **Zero-PAN Hardware Abstraction (`pos-hal`):** Isolates payment terminals via opaque `P2PE-TOK-*` tokens and emits genuine binary ESC/POS receipt printer + cash drawer streams.
- **Offline-First Edge Sync & Scoped Customer QR:** `redb` ACID persistence paired with `Idempotency-Key` deduplication, sequence-numbered WebSocket deltas, and HMAC-SHA256 order-scoped guest QR sessions.

---

For full module topology, crate breakdowns, multi-vertical store partitioning, and system architecture diagrams, see [`pos_architecture.md`](./pos_architecture.md).

## Local Build & Verification
```bash
# Run Rust workspace checks & tests
cargo test --workspace

# Start the Edge Hub server
cargo run -p pos-server

# Start the React / TypeScript frontend
cd frontend && npm install && npm run dev
```

---

## More Projects, Live Demos & Books by Michael Brunet
- **[Main Studio & Storefront](https://michaelbrunet-bit.github.io/)**
- **[Play 1983 W.O.P.R. Simulator Live](https://michaelbrunet-bit.github.io/GTW-1983-WOPR-Simulator/)**
- **[Play Arcana Tactics Tarot RPG Live](https://michaelbrunet-bit.github.io/arcana-tactics/)**
- **[Books & Survival Field Codex (*After Zero*, *Chronicles of Mike*, *Kronos* & Kids SEL)](https://michaelbrunet-bit.github.io/books/)**
- **[Python/FastAPI & Hybrid RAG Templates (Gumroad)](https://brunetmike.gumroad.com)**
- **[18 Live Cloud Data Extractors (Apify)](https://apify.com/dagnabit)**

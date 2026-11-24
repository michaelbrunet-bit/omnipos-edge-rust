import React, { useEffect, useState } from "react";
import { formatMoney, posApi, PosApiError } from "../api";
import {
  AuditLogEntry,
  AuditVerificationReport,
  CashMovementKind,
  CatalogItem,
  CategoryConfig,
  HalStatusSnapshot,
  LedgerSummaryResponse,
  ModManifest,
  RenderedReceipt,
  ShiftsSummaryResponse,
  StoreTheme,
} from "../types";

interface OwnerConsoleViewProps {
  currency: string;
  draftTheme: StoreTheme;
  themePresets: StoreTheme[];
  categories: CategoryConfig[];
  catalog: CatalogItem[];
  mods: ModManifest[];
  auditReport: AuditVerificationReport;
  recentAudit: AuditLogEntry[];
  onChangeDraftTheme: (theme: StoreTheme) => void;
  onApplyPreset: (presetId: string) => void;
  onSaveTheme: () => void;
  onToggleMod: (modId: string, enabled: boolean) => void;
  onToggleAvailability: (itemId: string) => void;
  onCreateCategory: (cat: CategoryConfig) => Promise<void>;
  onRequireManagerOverride: (
    actionLabel: string,
    callback: (overrideToken: string) => Promise<void>
  ) => void;
  onShowReceipt: (receipt: RenderedReceipt) => void;
  onToast: (msg: string) => void;
  onRefreshBootstrap: () => Promise<void>;
}

export const OwnerConsoleView: React.FC<OwnerConsoleViewProps> = ({
  currency,
  draftTheme,
  themePresets,
  categories,
  catalog,
  mods,
  auditReport,
  recentAudit,
  onChangeDraftTheme,
  onApplyPreset,
  onSaveTheme,
  onToggleMod,
  onToggleAvailability,
  onCreateCategory,
  onRequireManagerOverride,
  onShowReceipt,
  onToast,
  onRefreshBootstrap,
}) => {
  const [ownerTab, setOwnerTab] = useState<
    "theme" | "mods" | "catalog" | "ledger" | "hal_audit"
  >("theme");

  // Custom Category state
  const [catId, setCatId] = useState<string>("");
  const [catLabel, setCatLabel] = useState<string>("");
  const [catEmoji, setCatEmoji] = useState<string>("✨");
  const [catTaxRule, setCatTaxRule] = useState<string>("tax-standard-sales");

  // Ledger, Shifts & HAL state
  const [ledger, setLedger] = useState<LedgerSummaryResponse | null>(null);
  const [shifts, setShifts] = useState<ShiftsSummaryResponse | null>(null);
  const [hal, setHal] = useState<HalStatusSnapshot | null>(null);

  const [movementKind, setMovementKind] =
    useState<CashMovementKind>("paid_out");
  const [movementDollars, setMovementDollars] = useState<string>("15.00");
  const [movementReason, setMovementReason] = useState<string>(
    "Fresh garnishes / courier tip"
  );
  const [countedDollars, setCountedDollars] = useState<string>("");
  const [newFloatDollars, setNewFloatDollars] = useState<string>("200.00");

  const loadLedgerAndShifts = async () => {
    try {
      const [l, s] = await Promise.all([
        posApi.getLedger().catch(() => null),
        posApi.getShifts().catch(() => null),
      ]);
      if (l) setLedger(l);
      if (s) {
        setShifts(s);
        if (s.x_report) {
          setCountedDollars(
            (s.x_report.expected_drawer_cash_cents / 100).toFixed(2)
          );
        }
      }
    } catch {
      // Ignore if current persona lacks ViewAuditLog
    }
  };

  const loadHal = async () => {
    const h = await posApi.getHalStatus().catch(() => null);
    if (h) setHal(h);
  };

  useEffect(() => {
    if (ownerTab === "ledger") {
      loadLedgerAndShifts();
    } else if (ownerTab === "hal_audit") {
      loadHal();
    }
  }, [ownerTab]);

  const handleAddCategorySubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!catId.trim() || !catLabel.trim()) return;
    await onCreateCategory({
      id: catId.trim().toLowerCase().replace(/[^a-z0-9_]/g, "_"),
      label: catLabel.trim(),
      icon_emoji: catEmoji.trim() || "🏷️",
      sort_order: categories.length + 1,
      default_tax_rule_id: catTaxRule,
    });
    setCatId("");
    setCatLabel("");
  };

  const handleRecordMovement = async (e: React.FormEvent) => {
    e.preventDefault();
    const cents = Math.round(parseFloat(movementDollars || "0") * 100);
    if (cents <= 0) return;
    try {
      await posApi.recordShiftMovement(movementKind, cents, movementReason);
      onToast(`Recorded ${movementKind.replace("_", " ")} (${formatMoney(cents, currency)})`);
      await loadLedgerAndShifts();
      await onRefreshBootstrap();
    } catch (err) {
      onToast(err instanceof Error ? err.message : "Shift movement failed");
    }
  };

  const handleCloseZReport = async (overrideToken?: string) => {
    const cents = Math.round(parseFloat(countedDollars || "0") * 100);
    try {
      const report = await posApi.closeShiftZ(cents, overrideToken);
      onToast(
        `Closed Z-Report! Over/Short: ${formatMoney(
          report.over_short_cents || 0,
          currency
        )}`
      );
      await loadLedgerAndShifts();
      await onRefreshBootstrap();
    } catch (err) {
      if (
        err instanceof PosApiError &&
        err.code === "MANAGER_OVERRIDE_REQUIRED"
      ) {
        onRequireManagerOverride(
          err.requiredAction || "Close End-of-Day Z-Report",
          async (tok) => {
            await handleCloseZReport(tok);
          }
        );
        return;
      }
      onToast(err instanceof Error ? err.message : "Failed to close Z-Report");
    }
  };

  const handleOpenNewShift = async () => {
    const cents = Math.round(parseFloat(newFloatDollars || "200") * 100);
    try {
      await posApi.openShift(cents);
      onToast(`Opened new Cash Drawer Shift (${formatMoney(cents, currency)} float)`);
      await loadLedgerAndShifts();
      await onRefreshBootstrap();
    } catch (err) {
      onToast(err instanceof Error ? err.message : "Failed to open shift");
    }
  };

  const handleKickDrawer = async () => {
    try {
      const job = await posApi.kickCashDrawer();
      onShowReceipt(job);
      await loadHal();
    } catch (err) {
      onToast(err instanceof Error ? err.message : "Drawer kick denied");
    }
  };

  return (
    <main
      style={{
        padding: "1.25rem",
        display: "flex",
        flexDirection: "column",
        gap: "1rem",
      }}
    >
      <div
        className="pos-card"
        style={{
          display: "flex",
          flexWrap: "wrap",
          gap: "0.5rem",
          justifyContent: "space-between",
          alignItems: "center",
        }}
      >
        <div>
          <h2 style={{ margin: 0 }}>
            👑 Store Owner Console & Multi-Store Control Plane
          </h2>
          <span className="pos-muted" style={{ fontSize: "0.84rem" }}>
            ACID redb multi-store partitions • Real Wasmi + Ed25519 mods • Double-entry accounting ledger • P2PE & ESC/POS HAL
          </span>
        </div>
        <div style={{ display: "flex", gap: "0.4rem", flexWrap: "wrap" }}>
          <button
            className={`pos-btn ${
              ownerTab === "theme" ? "pos-btn-primary" : ""
            }`}
            onClick={() => setOwnerTab("theme")}
          >
            🎨 Store Theme
          </button>
          <button
            className={`pos-btn ${
              ownerTab === "mods" ? "pos-btn-primary" : ""
            }`}
            onClick={() => setOwnerTab("mods")}
          >
            🧩 Wasm Sandbox ({mods.length})
          </button>
          <button
            className={`pos-btn ${
              ownerTab === "catalog" ? "pos-btn-primary" : ""
            }`}
            onClick={() => setOwnerTab("catalog")}
          >
            📋 Categories & Catalog
          </button>
          <button
            className={`pos-btn ${
              ownerTab === "ledger" ? "pos-btn-primary" : ""
            }`}
            onClick={() => setOwnerTab("ledger")}
          >
            📊 Ledger & X/Z Shifts
          </button>
          <button
            className={`pos-btn ${
              ownerTab === "hal_audit" ? "pos-btn-primary" : ""
            }`}
            onClick={() => setOwnerTab("hal_audit")}
          >
            🖨️ Hardware & Audit ({auditReport.total_entries})
          </button>
        </div>
      </div>

      {/* TAB 1: THEME STUDIO */}
      {ownerTab === "theme" && (
        <div className="pos-layout" style={{ padding: 0 }}>
          <div
            className="pos-card"
            style={{ display: "flex", flexDirection: "column", gap: "1rem" }}
          >
            <div>
              <h3 style={{ margin: "0 0 0.35rem 0" }}>
                1-Click Store Theme & Industry Presets
              </h3>
              <div
                style={{
                  display: "grid",
                  gridTemplateColumns: "repeat(auto-fit, minmax(200px, 1fr))",
                  gap: "0.6rem",
                }}
              >
                {themePresets.map((preset) => (
                  <button
                    key={preset.preset_id}
                    className="pos-btn"
                    style={{
                      justifyContent: "flex-start",
                      padding: "0.7rem",
                      borderColor:
                        draftTheme.preset_id === preset.preset_id
                          ? "var(--pos-primary)"
                          : undefined,
                    }}
                    onClick={() => onApplyPreset(preset.preset_id)}
                  >
                    <span
                      style={{
                        width: 22,
                        height: 22,
                        borderRadius: "50%",
                        backgroundColor: preset.primary_color,
                        border: `3px solid ${preset.background_color}`,
                      }}
                    />
                    <div style={{ textAlign: "left" }}>
                      <div style={{ fontWeight: 700 }}>{preset.store_name}</div>
                      <div className="pos-muted" style={{ fontSize: "0.72rem" }}>
                        {preset.vertical.toUpperCase()} •{" "}
                        {preset.color_scheme.toUpperCase()}
                      </div>
                    </div>
                  </button>
                ))}
              </div>
            </div>

            <div
              style={{
                display: "grid",
                gridTemplateColumns: "repeat(auto-fit, minmax(220px, 1fr))",
                gap: "0.85rem",
              }}
            >
              <div>
                <label className="pos-muted" style={{ fontSize: "0.8rem" }}>
                  Store Name
                </label>
                <input
                  className="pos-input"
                  value={draftTheme.store_name}
                  onChange={(e) =>
                    onChangeDraftTheme({
                      ...draftTheme,
                      store_name: e.target.value,
                    })
                  }
                />
              </div>
              <div>
                <label className="pos-muted" style={{ fontSize: "0.8rem" }}>
                  Tagline
                </label>
                <input
                  className="pos-input"
                  value={draftTheme.tagline}
                  onChange={(e) =>
                    onChangeDraftTheme({
                      ...draftTheme,
                      tagline: e.target.value,
                    })
                  }
                />
              </div>
              <div>
                <label className="pos-muted" style={{ fontSize: "0.8rem" }}>
                  Logo Monogram
                </label>
                <input
                  className="pos-input"
                  value={draftTheme.logo_text}
                  onChange={(e) =>
                    onChangeDraftTheme({
                      ...draftTheme,
                      logo_text: e.target.value,
                    })
                  }
                />
              </div>
              <div>
                <label className="pos-muted" style={{ fontSize: "0.8rem" }}>
                  Corner Radius Style
                </label>
                <select
                  className="pos-select"
                  value={draftTheme.border_radius}
                  onChange={(e) =>
                    onChangeDraftTheme({
                      ...draftTheme,
                      border_radius: e.target
                        .value as StoreTheme["border_radius"],
                    })
                  }
                >
                  <option value="sharp">Sharp (4px)</option>
                  <option value="rounded">Rounded (12px)</option>
                  <option value="pill">Pill / Soft (22px)</option>
                </select>
              </div>
              <div>
                <label className="pos-muted" style={{ fontSize: "0.8rem" }}>
                  UI Touch Density
                </label>
                <select
                  className="pos-select"
                  value={draftTheme.ui_density}
                  onChange={(e) =>
                    onChangeDraftTheme({
                      ...draftTheme,
                      ui_density: e.target.value as StoreTheme["ui_density"],
                    })
                  }
                >
                  <option value="compact">Compact (High-Speed Bar / Retail)</option>
                  <option value="comfortable">Comfortable (Dining / Salon)</option>
                  <option value="spacious">Spacious (Customer Kiosk)</option>
                </select>
              </div>
              <div>
                <label className="pos-muted" style={{ fontSize: "0.8rem" }}>
                  Typography Font Stack
                </label>
                <select
                  className="pos-select"
                  value={draftTheme.font_family}
                  onChange={(e) =>
                    onChangeDraftTheme({
                      ...draftTheme,
                      font_family: e.target.value,
                    })
                  }
                >
                  <option value="'Inter', system-ui, sans-serif">
                    Inter / Modern Sans
                  </option>
                  <option value="'Georgia', 'Times New Roman', serif">
                    Georgia / Fine Dining Serif
                  </option>
                  <option value="'Space Grotesk', system-ui, sans-serif">
                    Space Grotesk / Studio Display
                  </option>
                </select>
              </div>
            </div>

            <div
              style={{
                display: "grid",
                gridTemplateColumns: "repeat(auto-fit, minmax(150px, 1fr))",
                gap: "0.75rem",
              }}
            >
              {(
                [
                  ["primary_color", "Primary Brand"],
                  ["secondary_color", "Secondary"],
                  ["accent_color", "Accent / Pay"],
                  ["background_color", "Background"],
                  ["surface_color", "Card Surface"],
                  ["text_color", "Primary Text"],
                ] as const
              ).map(([key, label]) => (
                <div key={key}>
                  <label className="pos-muted" style={{ fontSize: "0.78rem" }}>
                    {label} ({draftTheme[key]})
                  </label>
                  <div
                    style={{
                      display: "flex",
                      gap: "0.4rem",
                      alignItems: "center",
                    }}
                  >
                    <input
                      type="color"
                      value={draftTheme[key]}
                      onChange={(e) =>
                        onChangeDraftTheme({
                          ...draftTheme,
                          [key]: e.target.value,
                        })
                      }
                      style={{
                        width: 42,
                        height: 36,
                        border: "none",
                        background: "transparent",
                        cursor: "pointer",
                      }}
                    />
                    <input
                      className="pos-input"
                      value={draftTheme[key]}
                      onChange={(e) =>
                        onChangeDraftTheme({
                          ...draftTheme,
                          [key]: e.target.value,
                        })
                      }
                    />
                  </div>
                </div>
              ))}
            </div>

            <div style={{ display: "flex", justifyContent: "flex-end" }}>
              <button className="pos-btn pos-btn-primary" onClick={onSaveTheme}>
                💾 Save & Broadcast Theme to All Terminals
              </button>
            </div>
          </div>

          <div className="pos-card">
            <h3 style={{ marginTop: 0 }}>Receipt & Scoped CSS Overrides</h3>
            <div
              style={{
                display: "flex",
                flexDirection: "column",
                gap: "0.75rem",
              }}
            >
              <div>
                <label className="pos-muted" style={{ fontSize: "0.8rem" }}>
                  Receipt Header
                </label>
                <input
                  className="pos-input"
                  value={draftTheme.receipt_header}
                  onChange={(e) =>
                    onChangeDraftTheme({
                      ...draftTheme,
                      receipt_header: e.target.value,
                    })
                  }
                />
              </div>
              <div>
                <label className="pos-muted" style={{ fontSize: "0.8rem" }}>
                  Receipt Footer
                </label>
                <input
                  className="pos-input"
                  value={draftTheme.receipt_footer}
                  onChange={(e) =>
                    onChangeDraftTheme({
                      ...draftTheme,
                      receipt_footer: e.target.value,
                    })
                  }
                />
              </div>
              <div>
                <label className="pos-muted" style={{ fontSize: "0.8rem" }}>
                  Custom CSS Override (Validated by Rust Security Guard)
                </label>
                <textarea
                  className="pos-input"
                  rows={4}
                  placeholder="/* Optional store CSS rules */"
                  value={draftTheme.custom_css || ""}
                  onChange={(e) =>
                    onChangeDraftTheme({
                      ...draftTheme,
                      custom_css: e.target.value ? e.target.value : null,
                    })
                  }
                />
              </div>
            </div>
          </div>
        </div>
      )}

      {/* TAB 2: WASM MOD SANDBOX */}
      {ownerTab === "mods" && (
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "repeat(auto-fit, minmax(350px, 1fr))",
            gap: "1rem",
          }}
        >
          {mods.map((mod) => {
            const fuelPct = Math.min(
              100,
              Math.round((mod.last_fuel_used / mod.fuel_limit) * 100)
            );
            return (
              <div
                key={mod.id}
                className="pos-card"
                style={{
                  display: "flex",
                  flexDirection: "column",
                  justifyContent: "space-between",
                  gap: "0.8rem",
                }}
              >
                <div>
                  <div
                    style={{
                      display: "flex",
                      justifyContent: "space-between",
                      alignItems: "flex-start",
                      gap: "0.5rem",
                    }}
                  >
                    <div>
                      <h3 style={{ margin: 0 }}>{mod.name}</h3>
                      <span className="pos-muted" style={{ fontSize: "0.76rem" }}>
                        v{mod.version} • {mod.runtime}
                      </span>
                    </div>
                    <span className="pos-badge">
                      {mod.enabled ? "🟢 ACTIVE" : "⚪ DISABLED"}
                    </span>
                  </div>

                  <p style={{ fontSize: "0.85rem" }}>{mod.description}</p>

                  <div
                    style={{
                      display: "flex",
                      gap: "0.35rem",
                      flexWrap: "wrap",
                      marginBottom: "0.55rem",
                    }}
                  >
                    {mod.capabilities.map((cap) => (
                      <span key={cap} className="pos-badge">
                        🔒 {cap}
                      </span>
                    ))}
                  </div>

                  <div className="pos-muted" style={{ fontSize: "0.74rem" }}>
                    <div>
                      Wasm SHA-256: <code>{mod.wasm_sha256.slice(0, 22)}...</code>
                    </div>
                    <div>
                      Ed25519 Signature:{" "}
                      <code>{mod.ed25519_signature_hex.slice(0, 22)}...</code> ✅
                    </div>
                    <div>
                      Wasmi Fuel Meter: {mod.last_fuel_used.toLocaleString()} /{" "}
                      {mod.fuel_limit.toLocaleString()} instructions ({fuelPct}%) •{" "}
                      {mod.invocations_count} runs
                    </div>
                  </div>
                </div>

                <button
                  className={`pos-btn ${
                    mod.enabled ? "pos-btn-secondary" : "pos-btn-primary"
                  }`}
                  onClick={() => onToggleMod(mod.id, !mod.enabled)}
                >
                  {mod.enabled ? "Disable .wasm Plugin" : "Enable .wasm Plugin"}
                </button>
              </div>
            );
          })}
        </div>
      )}

      {/* TAB 3: DATA-DRIVEN CATEGORIES & CATALOG */}
      {ownerTab === "catalog" && (
        <div style={{ display: "flex", flexDirection: "column", gap: "1rem" }}>
          <div className="pos-card">
            <h3 style={{ marginTop: 0 }}>
              ➕ Add Custom Data-Driven Category (Zero Rust Recompile Needed)
            </h3>
            <form
              onSubmit={handleAddCategorySubmit}
              style={{
                display: "flex",
                flexWrap: "wrap",
                gap: "0.55rem",
                alignItems: "center",
              }}
            >
              <input
                className="pos-input"
                style={{ width: 170 }}
                placeholder="Category ID (e.g. cigars)"
                value={catId}
                onChange={(e) => setCatId(e.target.value)}
              />
              <input
                className="pos-input"
                style={{ width: 210 }}
                placeholder="Display Label (e.g. Rare Cigars)"
                value={catLabel}
                onChange={(e) => setCatLabel(e.target.value)}
              />
              <input
                className="pos-input"
                style={{ width: 75 }}
                placeholder="Emoji"
                value={catEmoji}
                onChange={(e) => setCatEmoji(e.target.value)}
              />
              <select
                className="pos-select"
                style={{ width: 210 }}
                value={catTaxRule}
                onChange={(e) => setCatTaxRule(e.target.value)}
              >
                <option value="tax-standard-sales">Standard Sales Tax (8.25%)</option>
                <option value="tax-service-exempt">Service Tax Exempt (0%)</option>
                <option value="tax-vat-inclusive">VAT Inclusive (10%)</option>
              </select>
              <button type="submit" className="pos-btn pos-btn-primary">
                + Create Category
              </button>
            </form>
          </div>

          <div className="pos-card">
            <h3 style={{ marginTop: 0 }}>
              Real-Time Catalog, Stock Countdowns & 86 Control
            </h3>
            <div className="pos-grid-catalog">
              {catalog.map((item) => (
                <div key={item.id} className="pos-item-card">
                  <div>
                    <div
                      style={{
                        display: "flex",
                        justifyContent: "space-between",
                      }}
                    >
                      <strong>
                        {item.badge_emoji} {item.name}
                      </strong>
                      <span>{formatMoney(item.price_cents, currency)}</span>
                    </div>
                    <div
                      className="pos-muted"
                      style={{ fontSize: "0.76rem", marginTop: "0.25rem" }}
                    >
                      SKU: {item.sku} • Cat: <code>{item.category}</code> •{" "}
                      {item.stock_count !== null
                        ? `Stock: ${item.stock_count}`
                        : `Duration: ${item.duration_minutes || 0}m`}
                    </div>
                  </div>
                  <button
                    className={`pos-btn ${
                      item.available ? "pos-btn-accent" : "pos-btn-secondary"
                    }`}
                    onClick={() => onToggleAvailability(item.id)}
                  >
                    {item.available
                      ? "✅ Available (Click to 86)"
                      : "🚫 86'd / Sold Out (Click to Restore)"}
                  </button>
                </div>
              ))}
            </div>
          </div>
        </div>
      )}

      {/* TAB 4: DOUBLE-ENTRY LEDGER & CASH DRAWER SHIFTS (X / Z REPORTS) */}
      {ownerTab === "ledger" && (
        <div className="pos-layout" style={{ padding: 0 }}>
          <div style={{ display: "flex", flexDirection: "column", gap: "1rem" }}>
            <div className="pos-card">
              <div
                style={{
                  display: "flex",
                  justifyContent: "space-between",
                  alignItems: "center",
                  marginBottom: "0.75rem",
                }}
              >
                <div>
                  <h3 style={{ margin: 0 }}>
                    📊 Double-Entry General Ledger & Trial Balance
                  </h3>
                  <span className="pos-muted" style={{ fontSize: "0.8rem" }}>
                    Every payment, tip, discount, comp, refund, and drawer drop posts balanced debits & credits.
                  </span>
                </div>
                {ledger && (
                  <span className="pos-badge">
                    {ledger.is_balanced
                      ? "✅ BALANCED (Debits == Credits)"
                      : "⚠️ UNBALANCED"}
                  </span>
                )}
              </div>

              {!ledger || ledger.trial_balance.length === 0 ? (
                <p className="pos-muted">
                  No ledger postings in this store yet — ring up a payment or cash movement to see live double-entry postings!
                </p>
              ) : (
                <>
                  <div
                    style={{
                      display: "flex",
                      flexDirection: "column",
                      gap: "0.35rem",
                      marginBottom: "0.9rem",
                    }}
                  >
                    {ledger.trial_balance.map((row) => (
                      <div
                        key={row.account}
                        style={{
                          display: "flex",
                          justifyContent: "space-between",
                          padding: "0.4rem 0.6rem",
                          backgroundColor: "var(--pos-bg)",
                          borderRadius: "var(--pos-radius)",
                          fontSize: "0.82rem",
                        }}
                      >
                        <strong>{row.account.replace(/_/g, " ").toUpperCase()}</strong>
                        <span>
                          DR {formatMoney(row.debit_cents, currency)} | CR{" "}
                          {formatMoney(row.credit_cents, currency)}
                        </span>
                      </div>
                    ))}
                    <div
                      style={{
                        display: "flex",
                        justifyContent: "space-between",
                        fontWeight: 800,
                        paddingTop: "0.35rem",
                      }}
                    >
                      <span>TOTAL TRIAL BALANCE</span>
                      <span>
                        DR {formatMoney(ledger.total_debits_cents, currency)} == CR{" "}
                        {formatMoney(ledger.total_credits_cents, currency)}
                      </span>
                    </div>
                  </div>

                  <h4 style={{ margin: "0 0 0.4rem 0" }}>
                    Recent Journal Entries ({ledger.entries.length})
                  </h4>
                  <div
                    style={{
                      display: "flex",
                      flexDirection: "column",
                      gap: "0.45rem",
                      maxHeight: 260,
                      overflowY: "auto",
                    }}
                  >
                    {ledger.entries.slice(0, 12).map((je) => (
                      <div
                        key={je.id}
                        style={{
                          padding: "0.5rem",
                          backgroundColor: "var(--pos-bg)",
                          borderRadius: "var(--pos-radius)",
                          fontSize: "0.78rem",
                        }}
                      >
                        <strong>{je.description}</strong>{" "}
                        <span className="pos-muted">({je.id})</span>
                      </div>
                    ))}
                  </div>
                </>
              )}
            </div>
          </div>

          {/* Right Column: Cash Drawer Shifts, X-Report & Z-Report */}
          <div className="pos-card" style={{ display: "flex", flexDirection: "column", gap: "0.85rem" }}>
            <h3 style={{ margin: 0 }}>💵 Cash Drawer Shift (X-Report & Z-Report)</h3>

            {shifts?.current_shift && shifts.x_report ? (
              <>
                <div
                  style={{
                    backgroundColor: "var(--pos-bg)",
                    padding: "0.7rem",
                    borderRadius: "var(--pos-radius)",
                    fontSize: "0.83rem",
                    display: "flex",
                    flexDirection: "column",
                    gap: "0.3rem",
                  }}
                >
                  <div style={{ fontWeight: 800 }}>
                    {shifts.x_report.report_type} ({shifts.current_shift.id})
                  </div>
                  <div style={{ display: "flex", justifyContent: "space-between" }}>
                    <span className="pos-muted">Opening Float</span>
                    <span>
                      {formatMoney(shifts.x_report.opening_float_cents, currency)}
                    </span>
                  </div>
                  <div style={{ display: "flex", justifyContent: "space-between" }}>
                    <span className="pos-muted">Cash Tenders</span>
                    <span>
                      +{formatMoney(shifts.x_report.cash_sales_cents, currency)}
                    </span>
                  </div>
                  <div style={{ display: "flex", justifyContent: "space-between" }}>
                    <span className="pos-muted">Cash Refunds</span>
                    <span>
                      -{formatMoney(shifts.x_report.cash_refunds_cents, currency)}
                    </span>
                  </div>
                  <div style={{ display: "flex", justifyContent: "space-between" }}>
                    <span className="pos-muted">Net Paid-In / Paid-Out / Drops</span>
                    <span>
                      {formatMoney(shifts.x_report.net_paid_in_out_cents, currency)}
                    </span>
                  </div>
                  <div
                    style={{
                      display: "flex",
                      justifyContent: "space-between",
                      fontWeight: 800,
                      borderTop: "1px solid rgba(148,163,184,0.2)",
                      paddingTop: "0.3rem",
                    }}
                  >
                    <span>Expected Cash in Till</span>
                    <span>
                      {formatMoney(
                        shifts.x_report.expected_drawer_cash_cents,
                        currency
                      )}
                    </span>
                  </div>
                </div>

                <form
                  onSubmit={handleRecordMovement}
                  style={{
                    display: "flex",
                    flexDirection: "column",
                    gap: "0.45rem",
                  }}
                >
                  <strong style={{ fontSize: "0.84rem" }}>
                    Paid-In / Paid-Out / Safe Drop:
                  </strong>
                  <div style={{ display: "flex", gap: "0.4rem" }}>
                    <select
                      className="pos-select"
                      value={movementKind}
                      onChange={(e) =>
                        setMovementKind(e.target.value as CashMovementKind)
                      }
                    >
                      <option value="paid_out">Paid Out</option>
                      <option value="safe_drop">Safe Blind Drop</option>
                      <option value="paid_in">Paid In</option>
                    </select>
                    <input
                      className="pos-input"
                      style={{ width: 110 }}
                      value={movementDollars}
                      onChange={(e) => setMovementDollars(e.target.value)}
                    />
                  </div>
                  <input
                    className="pos-input"
                    placeholder="Reason..."
                    value={movementReason}
                    onChange={(e) => setMovementReason(e.target.value)}
                  />
                  <button type="submit" className="pos-btn pos-btn-secondary">
                    Record Cash Movement + Kick Drawer
                  </button>
                </form>

                <div
                  style={{
                    borderTop: "1px solid rgba(148,163,184,0.22)",
                    paddingTop: "0.65rem",
                    display: "flex",
                    flexDirection: "column",
                    gap: "0.45rem",
                  }}
                >
                  <strong style={{ fontSize: "0.84rem" }}>
                    End-of-Day Z-Report Blind Count Closeout:
                  </strong>
                  <div style={{ display: "flex", gap: "0.4rem" }}>
                    <input
                      className="pos-input"
                      placeholder="Counted Cash ($)"
                      value={countedDollars}
                      onChange={(e) => setCountedDollars(e.target.value)}
                    />
                    <button
                      type="button"
                      className="pos-btn pos-btn-primary"
                      onClick={() => handleCloseZReport()}
                    >
                      Close Z-Report
                    </button>
                  </div>
                </div>
              </>
            ) : (
              <div style={{ display: "flex", flexDirection: "column", gap: "0.6rem" }}>
                <p className="pos-muted" style={{ margin: 0 }}>
                  Current shift is closed. Open a new Cash Drawer Shift to accept cash tenders:
                </p>
                <div style={{ display: "flex", gap: "0.45rem" }}>
                  <input
                    className="pos-input"
                    value={newFloatDollars}
                    onChange={(e) => setNewFloatDollars(e.target.value)}
                    placeholder="Opening Float ($)"
                  />
                  <button
                    className="pos-btn pos-btn-primary"
                    onClick={handleOpenNewShift}
                  >
                    + Open Till Shift
                  </button>
                </div>
              </div>
            )}
          </div>
        </div>
      )}

      {/* TAB 5: HARDWARE ABSTRACTION (POS-HAL) & SECURITY AUDIT CHAIN */}
      {ownerTab === "hal_audit" && (
        <div style={{ display: "flex", flexDirection: "column", gap: "1rem" }}>
          {hal && (
            <div className="pos-card">
              <div
                style={{
                  display: "flex",
                  justifyContent: "space-between",
                  alignItems: "center",
                  flexWrap: "wrap",
                  gap: "0.5rem",
                  marginBottom: "0.75rem",
                }}
              >
                <div>
                  <h3 style={{ margin: 0 }}>
                    🖨️ Hardware Abstraction Layer (`pos-hal`) Simulator
                  </h3>
                  <span className="pos-muted" style={{ fontSize: "0.8rem" }}>
                    P2PE Terminal: <code>{hal.terminal_serial}</code> • Printer:{" "}
                    <code>{hal.printer_model}</code> • RJ-11 Drawer Kicks:{" "}
                    <strong>{hal.drawer_kick_count}</strong>
                  </span>
                </div>
                <button className="pos-btn pos-btn-secondary" onClick={handleKickDrawer}>
                  ⚡ Trigger No-Sale Cash Drawer Kick
                </button>
              </div>

              <div
                style={{
                  display: "grid",
                  gridTemplateColumns: "repeat(auto-fit, minmax(280px, 1fr))",
                  gap: "0.75rem",
                }}
              >
                <div
                  style={{
                    backgroundColor: "var(--pos-bg)",
                    padding: "0.65rem",
                    borderRadius: "var(--pos-radius)",
                    fontSize: "0.8rem",
                  }}
                >
                  <strong>Recent P2PE Terminal Intents ({hal.payment_intents.length})</strong>
                  {hal.payment_intents.slice(0, 5).map((pi) => (
                    <div key={pi.id} style={{ padding: "0.25rem 0" }}>
                      <code>{pi.p2pe_token}</code> • {pi.status.toUpperCase()} •{" "}
                      {formatMoney(pi.captured_cents || pi.authorized_cents, currency)}
                    </div>
                  ))}
                </div>

                <div
                  style={{
                    backgroundColor: "var(--pos-bg)",
                    padding: "0.65rem",
                    borderRadius: "var(--pos-radius)",
                    fontSize: "0.8rem",
                  }}
                >
                  <strong>Recent ESC/POS Print Jobs ({hal.recent_receipts.length})</strong>
                  {hal.recent_receipts.slice(0, 5).map((rcpt) => (
                    <div
                      key={rcpt.id}
                      style={{
                        display: "flex",
                        justifyContent: "space-between",
                        padding: "0.22rem 0",
                      }}
                    >
                      <span>
                        {rcpt.id} ({rcpt.byte_length}B)
                      </span>
                      <button
                        className="pos-btn"
                        style={{ padding: "0.1rem 0.4rem", fontSize: "0.7rem" }}
                        onClick={() => onShowReceipt(rcpt)}
                      >
                        Inspect ESC/POS
                      </button>
                    </div>
                  ))}
                </div>
              </div>
            </div>
          )}

          <div className="pos-card">
            <div
              style={{
                display: "flex",
                justifyContent: "space-between",
                alignItems: "center",
                flexWrap: "wrap",
                marginBottom: "0.85rem",
              }}
            >
              <div>
                <h3 style={{ margin: 0 }}>
                  🔐 SHA-256 Hash-Chained Security Audit Ledger (redb ACID)
                </h3>
                <span className="pos-muted" style={{ fontSize: "0.82rem" }}>
                  Chain Integrity:{" "}
                  <strong>{auditReport.valid ? "VERIFIED ✅" : "BROKEN ⚠️"}</strong>
                </span>
              </div>
              <span className="pos-badge">
                Head Hash: {auditReport.head_hash.slice(0, 16)}...
              </span>
            </div>

            <div
              style={{ display: "flex", flexDirection: "column", gap: "0.5rem" }}
            >
              {recentAudit.map((entry) => (
                <div
                  key={entry.sequence}
                  style={{
                    padding: "0.65rem",
                    borderRadius: "var(--pos-radius)",
                    backgroundColor: "var(--pos-bg)",
                    fontSize: "0.83rem",
                    display: "flex",
                    justifyContent: "space-between",
                    flexWrap: "wrap",
                    gap: "0.5rem",
                  }}
                >
                  <div>
                    <strong>#{entry.sequence}</strong> •{" "}
                    <span className="pos-badge">{entry.action}</span> •{" "}
                    <strong>{entry.actor_name}</strong> ({entry.role}) •{" "}
                    <code>{entry.store_id}</code>
                    <div style={{ marginTop: "0.2rem" }}>{entry.details}</div>
                  </div>
                  <div
                    className="pos-muted"
                    style={{ fontSize: "0.74rem", textAlign: "right" }}
                  >
                    <div>{new Date(entry.timestamp).toLocaleTimeString()}</div>
                    <div>
                      <code>
                        {entry.prev_hash.slice(0, 10)} →{" "}
                        {entry.entry_hash.slice(0, 10)}
                      </code>
                    </div>
                  </div>
                </div>
              ))}
            </div>
          </div>
        </div>
      )}
    </main>
  );
};

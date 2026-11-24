import React, { useMemo, useState } from "react";
import { formatMoney } from "../api";
import {
  CatalogItem,
  CategoryConfig,
  IndustryVertical,
  ModifierQualifier,
  OrderView,
  PaymentMethod,
  Role,
  SplitCheckShare,
} from "../types";

const QUALIFIERS: { id: ModifierQualifier; label: string }[] = [
  { id: "regular", label: "Regular" },
  { id: "no", label: "NO" },
  { id: "light", label: "LIGHT" },
  { id: "extra", label: "EXTRA" },
  { id: "on_side", label: "ON SIDE" },
];

interface OperatorPosViewProps {
  currentVertical: IndustryVertical;
  activeRole: Role;
  currency: string;
  categories: CategoryConfig[];
  catalog: CatalogItem[];
  orders: OrderView[];
  activeOrder: OrderView | undefined;
  customSplitShares: SplitCheckShare[];
  splitMode: "none" | "seat" | "equal";
  splitWays: number;
  onSelectOrder: (orderId: string) => void;
  onCreateOrder: (name: string, guests: number, isBarTab: boolean) => void;
  onAddItem: (
    item: CatalogItem,
    modifierIds: string[],
    qualifier: ModifierQualifier,
    seat: number,
    note: string
  ) => Promise<void>;
  onRemoveLineItem: (lineId: string) => void;
  onCompLineItem: (lineId: string) => void;
  onAdvanceItemPrep: (lineId: string) => void;
  onBindPreauth: () => void;
  onFireOrder: () => void;
  onSetSplitMode: (mode: "none" | "seat" | "equal") => void;
  onSetSplitWays: (ways: number) => void;
  onPayOrder: (
    method: PaymentMethod,
    customPrincipalCents?: number,
    payerLabel?: string
  ) => void;
  onRefundPayment: (paymentId: string, amountCents: number) => void;
  onPrintReceipt: (kickDrawer: boolean) => void;
  onMintQrLink: () => void;
  onToast: (msg: string) => void;
}

export const OperatorPosView: React.FC<OperatorPosViewProps> = ({
  currentVertical,
  activeRole,
  currency,
  categories,
  catalog,
  orders,
  activeOrder,
  customSplitShares,
  splitMode,
  splitWays,
  onSelectOrder,
  onCreateOrder,
  onAddItem,
  onRemoveLineItem,
  onCompLineItem,
  onAdvanceItemPrep,
  onBindPreauth,
  onFireOrder,
  onSetSplitMode,
  onSetSplitWays,
  onPayOrder,
  onRefundPayment,
  onPrintReceipt,
  onMintQrLink,
  onToast,
}) => {
  const [selectedCategory, setSelectedCategory] = useState<string>("all");
  const [selectedSeat, setSelectedSeat] = useState<number>(1);
  const [modifierQualifier, setModifierQualifier] =
    useState<ModifierQualifier>("regular");
  const [itemNote, setItemNote] = useState<string>("");
  const [barcodeQuery, setBarcodeQuery] = useState<string>("");
  const [selectedModifiersByItem, setSelectedModifiersByItem] = useState<
    Record<string, string[]>
  >({});
  const [newOrderName, setNewOrderName] = useState<string>("");
  const [newOrderGuests, setNewOrderGuests] = useState<number>(2);

  const filteredCatalog = useMemo(() => {
    return catalog.filter((item) => {
      if (selectedCategory !== "all" && item.category !== selectedCategory) {
        return false;
      }
      if (barcodeQuery.trim()) {
        const q = barcodeQuery.trim().toLowerCase();
        return (
          item.sku.toLowerCase().includes(q) ||
          item.name.toLowerCase().includes(q) ||
          (item.barcode_upc || "").toLowerCase().includes(q)
        );
      }
      return true;
    });
  }, [catalog, selectedCategory, barcodeQuery]);

  const toggleModifierSelection = (itemId: string, modId: string) => {
    setSelectedModifiersByItem((prev) => {
      const current = prev[itemId] || [];
      return {
        ...prev,
        [itemId]: current.includes(modId)
          ? current.filter((id) => id !== modId)
          : [...current, modId],
      };
    });
  };

  const handleBarcodeScanSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!barcodeQuery.trim()) return;
    const q = barcodeQuery.trim().toLowerCase();
    const matched = catalog.find(
      (i) =>
        (i.barcode_upc && i.barcode_upc.toLowerCase() === q) ||
        i.sku.toLowerCase() === q ||
        i.name.toLowerCase().includes(q)
    );
    if (matched) {
      await onAddItem(
        matched,
        selectedModifiersByItem[matched.id] || [],
        modifierQualifier,
        selectedSeat,
        itemNote
      );
      setBarcodeQuery("");
      setItemNote("");
    } else {
      onToast(`No SKU/Barcode found matching "${barcodeQuery}"`);
    }
  };

  return (
    <main className="pos-layout">
      <section style={{ display: "flex", flexDirection: "column", gap: "1rem" }}>
        {/* Active Tables / Bar Tabs / Salon Chairs / Retail Bags */}
        <div className="pos-card">
          <div
            style={{
              display: "flex",
              justifyContent: "space-between",
              alignItems: "center",
              flexWrap: "wrap",
              gap: "0.75rem",
              marginBottom: "0.85rem",
            }}
          >
            <div>
              <h3 style={{ margin: 0 }}>
                {currentVertical === "salon_barber"
                  ? "💈 Barber & Salon Chair Board • Active Appointments & Walk-Ins"
                  : currentVertical === "retail"
                  ? "🛍️ Active Register Bags & Clienteling Carts"
                  : activeRole === "bartender"
                  ? "🍸 Toast-Style Pre-Authorized Bar Tabs & Speed Rail"
                  : "🍽️ Aloha-Style Pivot Seating Floor Plan & Tables"}
              </h3>
              <span className="pos-muted" style={{ fontSize: "0.8rem" }}>
                {currentVertical === "salon_barber"
                  ? "Tax-exempt chair services + taxable apothecary retail bundles & Wasm stylist commissions"
                  : currentVertical === "retail"
                  ? "Scan UPC barcodes, monitor real-time SKU stock countdowns & apply multi-unit Wasm promos"
                  : "Aloha Pivot Seat Splitting & Conversational Modifiers + Toast P2PE Pre-Auth & Live Stock Countdowns"}
              </span>
            </div>

            <div style={{ display: "flex", gap: "0.45rem", alignItems: "center" }}>
              <input
                className="pos-input"
                style={{ width: 195 }}
                placeholder={
                  currentVertical === "salon_barber"
                    ? "Chair / Client Name..."
                    : currentVertical === "retail"
                    ? "Customer / Bag Label..."
                    : "Table or Tab Name..."
                }
                value={newOrderName}
                onChange={(e) => setNewOrderName(e.target.value)}
              />
              {currentVertical === "hospitality" && (
                <select
                  className="pos-select"
                  style={{ width: 105 }}
                  value={newOrderGuests}
                  onChange={(e) => setNewOrderGuests(Number(e.target.value))}
                >
                  {[1, 2, 3, 4, 5, 6, 8].map((g) => (
                    <option key={g} value={g}>
                      {g} {g === 1 ? "Guest" : "Guests"}
                    </option>
                  ))}
                </select>
              )}
              <button
                className="pos-btn pos-btn-primary"
                onClick={() => {
                  onCreateOrder(
                    newOrderName,
                    newOrderGuests,
                    activeRole === "bartender"
                  );
                  setNewOrderName("");
                }}
              >
                + New{" "}
                {currentVertical === "salon_barber"
                  ? "Appointment"
                  : currentVertical === "retail"
                  ? "Bag"
                  : activeRole === "bartender"
                  ? "Tab"
                  : "Table"}
              </button>
            </div>
          </div>

          <div
            style={{
              display: "grid",
              gridTemplateColumns: "repeat(auto-fill, minmax(225px, 1fr))",
              gap: "0.65rem",
            }}
          >
            {orders.map((ord) => {
              const isSelected = activeOrder?.id === ord.id;
              return (
                <button
                  key={ord.id}
                  onClick={() => onSelectOrder(ord.id)}
                  className="pos-btn"
                  style={{
                    flexDirection: "column",
                    alignItems: "flex-start",
                    padding: "0.75rem",
                    borderWidth: isSelected ? 2 : 1,
                    borderColor: isSelected
                      ? "var(--pos-primary)"
                      : "rgba(148,163,184,0.25)",
                  }}
                >
                  <div
                    style={{
                      width: "100%",
                      display: "flex",
                      justifyContent: "space-between",
                      alignItems: "center",
                    }}
                  >
                    <strong>{ord.table_or_tab_name}</strong>
                    <span className="pos-badge">
                      {ord.status.replace(/_/g, " ")}
                    </span>
                  </div>
                  {ord.preauth_card && (
                    <div
                      style={{
                        fontSize: "0.72rem",
                        color: "var(--pos-accent)",
                        marginTop: "0.2rem",
                      }}
                    >
                      💳 {ord.preauth_card}
                    </div>
                  )}
                  <div
                    className="pos-muted"
                    style={{
                      fontSize: "0.78rem",
                      width: "100%",
                      display: "flex",
                      justifyContent: "space-between",
                      marginTop: "0.25rem",
                    }}
                  >
                    <span>
                      {ord.assigned_operator.split(" ")[0]} • {ord.items.length}{" "}
                      items
                    </span>
                    <strong style={{ color: "var(--pos-text)" }}>
                      {formatMoney(ord.totals.remaining_balance_cents, currency)}
                    </strong>
                  </div>
                </button>
              );
            })}
          </div>
        </div>

        {/* Data-Driven Categories & Entry Controls */}
        <div className="pos-card">
          <div
            style={{
              display: "flex",
              flexWrap: "wrap",
              gap: "0.45rem",
              marginBottom: "0.8rem",
            }}
          >
            <button
              className={`pos-btn ${
                selectedCategory === "all" ? "pos-btn-primary" : ""
              }`}
              onClick={() => setSelectedCategory("all")}
            >
              All ({catalog.length})
            </button>
            {categories.map((cat) => (
              <button
                key={cat.id}
                className={`pos-btn ${
                  selectedCategory === cat.id ? "pos-btn-primary" : ""
                }`}
                onClick={() => setSelectedCategory(cat.id)}
              >
                {cat.icon_emoji} {cat.label}
              </button>
            ))}
          </div>

          <div
            style={{
              display: "flex",
              flexWrap: "wrap",
              gap: "0.75rem",
              alignItems: "center",
              justifyContent: "space-between",
            }}
          >
            {currentVertical === "hospitality" && (
              <div
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: "0.3rem",
                  flexWrap: "wrap",
                }}
              >
                <span className="pos-muted" style={{ fontSize: "0.8rem" }}>
                  Aloha Pivot Seat:
                </span>
                {[1, 2, 3, 4].map((seat) => (
                  <button
                    key={seat}
                    className={`pos-btn ${
                      selectedSeat === seat ? "pos-btn-secondary" : ""
                    }`}
                    style={{ padding: "0.3rem 0.6rem", fontSize: "0.78rem" }}
                    onClick={() => setSelectedSeat(seat)}
                  >
                    Seat {seat}
                  </button>
                ))}
              </div>
            )}

            {currentVertical !== "retail" && (
              <div
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: "0.3rem",
                  flexWrap: "wrap",
                }}
              >
                <span className="pos-muted" style={{ fontSize: "0.8rem" }}>
                  Mod Qualifier:
                </span>
                {QUALIFIERS.map((q) => (
                  <button
                    key={q.id}
                    className={`pos-btn ${
                      modifierQualifier === q.id ? "pos-btn-accent" : ""
                    }`}
                    style={{ padding: "0.28rem 0.55rem", fontSize: "0.75rem" }}
                    onClick={() => setModifierQualifier(q.id)}
                  >
                    {q.label}
                  </button>
                ))}
              </div>
            )}

            {currentVertical === "retail" && (
              <form
                onSubmit={handleBarcodeScanSubmit}
                style={{
                  display: "flex",
                  gap: "0.45rem",
                  flex: 1,
                  minWidth: 260,
                }}
              >
                <input
                  className="pos-input"
                  placeholder="🔫 Scan or type UPC Barcode / SKU (e.g. 01942500101 or APP-102)..."
                  value={barcodeQuery}
                  onChange={(e) => setBarcodeQuery(e.target.value)}
                />
                <button type="submit" className="pos-btn pos-btn-primary">
                  Scan & Add
                </button>
              </form>
            )}

            <input
              className="pos-input"
              style={{ maxWidth: 260 }}
              placeholder={
                currentVertical === "salon_barber"
                  ? "Service note (e.g. #1.5 guard)..."
                  : currentVertical === "retail"
                  ? "Bag note (e.g. Gift wrap)..."
                  : "Kitchen/Bar note (e.g. Allergy)..."
              }
              value={itemNote}
              onChange={(e) => setItemNote(e.target.value)}
            />
          </div>
        </div>

        {/* Catalog Grid */}
        <div className="pos-grid-catalog">
          {filteredCatalog.map((item) => {
            const selectedMods = selectedModifiersByItem[item.id] || [];
            return (
              <div
                key={item.id}
                className="pos-item-card"
                style={{ opacity: item.available ? 1 : 0.55 }}
              >
                <div>
                  <div
                    style={{
                      display: "flex",
                      justifyContent: "space-between",
                      alignItems: "center",
                      gap: "0.4rem",
                    }}
                  >
                    <span style={{ fontSize: "1.45rem" }}>{item.badge_emoji}</span>
                    <div
                      style={{
                        display: "flex",
                        gap: "0.35rem",
                        alignItems: "center",
                      }}
                    >
                      {item.tax_mode === "exempt" && (
                        <span className="pos-badge">Tax Exempt</span>
                      )}
                      {item.duration_minutes ? (
                        <span className="pos-badge">⏱️ {item.duration_minutes}m</span>
                      ) : null}
                      {item.stock_count !== null && (
                        <span
                          className="pos-badge"
                          style={{
                            color:
                              item.stock_count <= 5
                                ? "var(--pos-primary)"
                                : undefined,
                          }}
                        >
                          📦 {item.stock_count} left
                        </span>
                      )}
                      <span style={{ fontWeight: 800, fontSize: "1.02rem" }}>
                        {formatMoney(item.price_cents, currency)}
                      </span>
                    </div>
                  </div>
                  <h4 style={{ margin: "0.4rem 0 0.2rem 0" }}>{item.name}</h4>
                  {item.barcode_upc && (
                    <div
                      className="pos-muted"
                      style={{ fontSize: "0.72rem", marginBottom: "0.25rem" }}
                    >
                      SKU: {item.sku} • UPC: <code>{item.barcode_upc}</code>
                    </div>
                  )}
                  <p
                    className="pos-muted"
                    style={{ fontSize: "0.78rem", margin: "0 0 0.5rem 0" }}
                  >
                    {item.description}
                  </p>

                  {item.modifiers.length > 0 && (
                    <div
                      style={{
                        display: "flex",
                        flexWrap: "wrap",
                        gap: "0.3rem",
                        marginBottom: "0.45rem",
                      }}
                    >
                      {item.modifiers.map((mod) => {
                        const active = selectedMods.includes(mod.id);
                        return (
                          <button
                            key={mod.id}
                            type="button"
                            className={`pos-btn ${
                              active ? "pos-btn-secondary" : ""
                            }`}
                            style={{
                              fontSize: "0.72rem",
                              padding: "0.2rem 0.45rem",
                            }}
                            onClick={() =>
                              toggleModifierSelection(item.id, mod.id)
                            }
                          >
                            {active && modifierQualifier !== "regular"
                              ? `${modifierQualifier.toUpperCase()}: `
                              : ""}
                            {mod.name}
                            {mod.price_delta_cents > 0
                              ? ` (+${formatMoney(
                                  mod.price_delta_cents,
                                  currency
                                )})`
                              : ""}
                          </button>
                        );
                      })}
                    </div>
                  )}
                </div>

                <button
                  className="pos-btn pos-btn-primary"
                  disabled={
                    !item.available ||
                    activeOrder?.status === "paid" ||
                    activeOrder?.status === "refunded"
                  }
                  onClick={async () => {
                    await onAddItem(
                      item,
                      selectedMods,
                      modifierQualifier,
                      selectedSeat,
                      itemNote
                    );
                    setItemNote("");
                  }}
                >
                  {item.available
                    ? currentVertical === "salon_barber"
                      ? "+ Add to Appointment"
                      : currentVertical === "retail"
                      ? "+ Scan to Bag"
                      : `+ Add (Seat ${selectedSeat})`
                    : "86'd / Sold Out"}
                </button>
              </div>
            );
          })}
        </div>
      </section>

      {/* Right Column: Live Ticket, Comp/Void/Refund, ESC/POS Print & Signed QR */}
      {activeOrder && (
        <aside
          className="pos-card"
          style={{ display: "flex", flexDirection: "column", gap: "0.8rem" }}
        >
          <div
            style={{
              display: "flex",
              justifyContent: "space-between",
              alignItems: "flex-start",
              borderBottom: "1px solid rgba(148,163,184,0.2)",
              paddingBottom: "0.65rem",
              gap: "0.5rem",
            }}
          >
            <div>
              <h3 style={{ margin: 0 }}>{activeOrder.table_or_tab_name}</h3>
              <span className="pos-muted" style={{ fontSize: "0.78rem" }}>
                Operator: {activeOrder.assigned_operator}
              </span>
            </div>
            <div style={{ display: "flex", gap: "0.35rem", flexWrap: "wrap" }}>
              <button
                className="pos-btn"
                style={{ padding: "0.22rem 0.48rem", fontSize: "0.73rem" }}
                title="Mint HMAC-SHA256 Signed Customer QR Link"
                onClick={onMintQrLink}
              >
                📱 QR Link
              </button>
              <button
                className="pos-btn"
                style={{ padding: "0.22rem 0.48rem", fontSize: "0.73rem" }}
                title="Render ESC/POS Thermal Receipt"
                onClick={() => onPrintReceipt(false)}
              >
                🧾 Print
              </button>
              <span className="pos-badge">
                {activeOrder.status.toUpperCase().replace(/_/g, " ")}
              </span>
            </div>
          </div>

          <div
            style={{
              display: "flex",
              flexDirection: "column",
              gap: "0.45rem",
              backgroundColor: "var(--pos-bg)",
              padding: "0.6rem",
              borderRadius: "var(--pos-radius)",
              fontSize: "0.8rem",
            }}
          >
            <div
              style={{
                display: "flex",
                justifyContent: "space-between",
                alignItems: "center",
              }}
            >
              <span>
                💳{" "}
                {activeOrder.preauth_card ? (
                  <strong style={{ color: "var(--pos-accent)" }}>
                    {activeOrder.preauth_card}
                  </strong>
                ) : (
                  <span className="pos-muted">No P2PE Pre-Auth Bound</span>
                )}
              </span>
              {!activeOrder.preauth_card && (
                <button
                  className="pos-btn"
                  style={{ padding: "0.2rem 0.5rem", fontSize: "0.73rem" }}
                  onClick={onBindPreauth}
                >
                  + Bind P2PE Hold
                </button>
              )}
            </div>

            {activeOrder.client_profile_notes && (
              <div
                style={{
                  color: "var(--pos-primary)",
                  fontWeight: 600,
                  fontSize: "0.78rem",
                }}
              >
                📋 {activeOrder.client_profile_notes}
              </div>
            )}
          </div>

          {/* Line Items */}
          <div
            style={{
              display: "flex",
              flexDirection: "column",
              gap: "0.55rem",
              maxHeight: 290,
              overflowY: "auto",
            }}
          >
            {activeOrder.items.length === 0 ? (
              <p className="pos-muted">No items on this ticket yet.</p>
            ) : (
              activeOrder.items.map((line) => {
                const modDelta = line.selected_modifiers.reduce(
                  (acc, m) => acc + m.price_delta_cents,
                  0
                );
                const grossLine =
                  (line.unit_price_cents + modDelta) * line.quantity;
                const displayTotal = line.comped ? 0 : grossLine;

                return (
                  <div
                    key={line.id}
                    style={{
                      padding: "0.55rem",
                      borderRadius: "var(--pos-radius)",
                      backgroundColor: "var(--pos-bg)",
                      display: "flex",
                      justifyContent: "space-between",
                      alignItems: "flex-start",
                      gap: "0.5rem",
                      opacity: line.comped ? 0.78 : 1,
                    }}
                  >
                    <div>
                      <div style={{ fontWeight: 600, fontSize: "0.88rem" }}>
                        {line.quantity}x {line.name}{" "}
                        {line.comped && (
                          <span
                            className="pos-badge"
                            style={{ color: "var(--pos-accent)" }}
                          >
                            COMPED
                          </span>
                        )}
                        {line.seat_number &&
                        currentVertical === "hospitality" ? (
                          <span className="pos-badge">
                            Seat {line.seat_number}
                          </span>
                        ) : null}
                      </div>
                      {line.selected_modifiers.length > 0 && (
                        <div className="pos-muted" style={{ fontSize: "0.75rem" }}>
                          +{" "}
                          {line.selected_modifiers
                            .map((m) =>
                              m.qualifier && m.qualifier !== "regular"
                                ? `${m.qualifier.toUpperCase()} ${m.name}`
                                : m.name
                            )
                            .join(", ")}
                        </div>
                      )}
                      {line.comp_reason && (
                        <div
                          style={{
                            fontSize: "0.72rem",
                            color: "var(--pos-accent)",
                          }}
                        >
                          Comp: {line.comp_reason} ({line.comp_approved_by})
                        </div>
                      )}
                      {line.notes && (
                        <div
                          style={{
                            fontSize: "0.74rem",
                            color: "var(--pos-primary)",
                          }}
                        >
                          Note: "{line.notes}"
                        </div>
                      )}
                      <div
                        style={{
                          display: "flex",
                          alignItems: "center",
                          gap: "0.35rem",
                          marginTop: "0.25rem",
                        }}
                      >
                        <button
                          className="pos-btn"
                          style={{
                            padding: "0.12rem 0.45rem",
                            fontSize: "0.7rem",
                          }}
                          onClick={() => onAdvanceItemPrep(line.id)}
                        >
                          Stage: {line.prep_status.toUpperCase()} ➔
                        </button>
                        {activeOrder.status !== "paid" &&
                          activeOrder.status !== "refunded" && (
                            <button
                              className="pos-btn"
                              style={{
                                padding: "0.12rem 0.45rem",
                                fontSize: "0.7rem",
                              }}
                              title="Manager-approved Comp toggle"
                              onClick={() => onCompLineItem(line.id)}
                            >
                              {line.comped ? "Un-Comp" : "🎁 Comp"}
                            </button>
                          )}
                      </div>
                    </div>

                    <div
                      style={{
                        display: "flex",
                        alignItems: "center",
                        gap: "0.35rem",
                      }}
                    >
                      <strong>{formatMoney(displayTotal, currency)}</strong>
                      {activeOrder.status !== "paid" &&
                        activeOrder.status !== "refunded" && (
                          <button
                            className="pos-btn"
                            style={{
                              padding: "0.2rem 0.45rem",
                              fontSize: "0.74rem",
                            }}
                            title={
                              line.prep_status !== "held"
                                ? "Void Fired Item (Requires Manager Override)"
                                : "Void Held Item"
                            }
                            onClick={() => onRemoveLineItem(line.id)}
                          >
                            ✕
                          </button>
                        )}
                    </div>
                  </div>
                );
              })
            )}
          </div>

          {activeOrder.items.some((i) => i.prep_status === "held") && (
            <button
              className="pos-btn pos-btn-secondary"
              onClick={onFireOrder}
            >
              🔥 Fire Held Items to Prep Station
            </button>
          )}

          {/* Totals */}
          <div
            style={{
              borderTop: "1px solid rgba(148,163,184,0.22)",
              paddingTop: "0.6rem",
              display: "flex",
              flexDirection: "column",
              gap: "0.32rem",
              fontSize: "0.88rem",
            }}
          >
            {activeOrder.totals.comped_total_cents > 0 && (
              <div
                style={{
                  display: "flex",
                  justifyContent: "space-between",
                  color: "var(--pos-accent)",
                }}
              >
                <span>Manager Comps</span>
                <span>
                  -{formatMoney(activeOrder.totals.comped_total_cents, currency)}
                </span>
              </div>
            )}
            <div style={{ display: "flex", justifyContent: "space-between" }}>
              <span className="pos-muted">Subtotal</span>
              <span>
                {formatMoney(activeOrder.totals.subtotal_cents, currency)}
              </span>
            </div>

            {activeOrder.mod_adjustments.map((adj) => (
              <div
                key={adj.mod_id}
                style={{
                  display: "flex",
                  justifyContent: "space-between",
                  color:
                    adj.amount_cents < 0
                      ? "var(--pos-accent)"
                      : "var(--pos-primary)",
                  fontWeight: 600,
                }}
              >
                <span>🧩 {adj.label}</span>
                <span>{formatMoney(adj.amount_cents, currency)}</span>
              </div>
            ))}

            <div style={{ display: "flex", justifyContent: "space-between" }}>
              <span className="pos-muted">
                Line-Item Tax (Taxable:{" "}
                {formatMoney(activeOrder.totals.taxable_amount_cents, currency)})
              </span>
              <span>{formatMoney(activeOrder.totals.tax_cents, currency)}</span>
            </div>

            {activeOrder.totals.paid_principal_cents > 0 && (
              <div
                style={{
                  display: "flex",
                  justifyContent: "space-between",
                  color: "var(--pos-accent)",
                }}
              >
                <span>Paid Principal</span>
                <span>
                  -
                  {formatMoney(
                    activeOrder.totals.paid_principal_cents,
                    currency
                  )}
                </span>
              </div>
            )}

            {activeOrder.totals.refunded_cents > 0 && (
              <div
                style={{
                  display: "flex",
                  justifyContent: "space-between",
                  color: "#ef4444",
                }}
              >
                <span>Refunded</span>
                <span>
                  {formatMoney(activeOrder.totals.refunded_cents, currency)}
                </span>
              </div>
            )}

            <div
              style={{
                display: "flex",
                justifyContent: "space-between",
                fontSize: "1.14rem",
                fontWeight: 800,
                marginTop: "0.2rem",
              }}
            >
              <span>Balance Due</span>
              <span>
                {formatMoney(
                  activeOrder.totals.remaining_balance_cents,
                  currency
                )}
              </span>
            </div>
          </div>

          {/* Refunds Section when Payments Exist */}
          {activeOrder.payments.length > 0 &&
            activeOrder.status !== "refunded" && (
              <div
                style={{
                  backgroundColor: "var(--pos-bg)",
                  padding: "0.5rem",
                  borderRadius: "var(--pos-radius)",
                  fontSize: "0.78rem",
                }}
              >
                <div style={{ fontWeight: 700, marginBottom: "0.25rem" }}>
                  Captured Tenders:
                </div>
                {activeOrder.payments.map((p) => (
                  <div
                    key={p.id}
                    style={{
                      display: "flex",
                      justifyContent: "space-between",
                      alignItems: "center",
                      padding: "0.18rem 0",
                    }}
                  >
                    <span>
                      {formatMoney(p.amount_cents, currency)} ({p.token_reference})
                    </span>
                    <button
                      className="pos-btn"
                      style={{ padding: "0.14rem 0.45rem", fontSize: "0.7rem" }}
                      onClick={() => onRefundPayment(p.id, p.amount_cents)}
                    >
                      ↩ Refund
                    </button>
                  </div>
                ))}
              </div>
            )}

          {/* Split & Tender */}
          <div
            style={{
              borderTop: "1px solid rgba(148,163,184,0.22)",
              paddingTop: "0.6rem",
              display: "flex",
              flexDirection: "column",
              gap: "0.5rem",
            }}
          >
            <div
              style={{
                display: "flex",
                justifyContent: "space-between",
                alignItems: "center",
                flexWrap: "wrap",
                gap: "0.3rem",
              }}
            >
              <span className="pos-muted" style={{ fontSize: "0.78rem" }}>
                Split Check:
              </span>
              <div style={{ display: "flex", gap: "0.25rem" }}>
                <button
                  className={`pos-btn ${
                    splitMode === "none" ? "pos-btn-secondary" : ""
                  }`}
                  style={{ padding: "0.22rem 0.5rem", fontSize: "0.74rem" }}
                  onClick={() => onSetSplitMode("none")}
                >
                  Full
                </button>
                <button
                  className={`pos-btn ${
                    splitMode === "seat" ? "pos-btn-secondary" : ""
                  }`}
                  style={{ padding: "0.22rem 0.5rem", fontSize: "0.74rem" }}
                  onClick={() => onSetSplitMode("seat")}
                >
                  🪑 By Seat
                </button>
                <button
                  className={`pos-btn ${
                    splitMode === "equal" ? "pos-btn-secondary" : ""
                  }`}
                  style={{ padding: "0.22rem 0.5rem", fontSize: "0.74rem" }}
                  onClick={() => onSetSplitMode("equal")}
                >
                  ➗ Equal
                </button>
              </div>
            </div>

            {splitMode === "seat" && activeOrder.split_by_seat.length > 0 && (
              <div
                style={{
                  backgroundColor: "var(--pos-bg)",
                  padding: "0.55rem",
                  borderRadius: "var(--pos-radius)",
                  fontSize: "0.8rem",
                }}
              >
                {activeOrder.split_by_seat.map((s) => (
                  <div
                    key={s.share_index}
                    style={{
                      display: "flex",
                      justifyContent: "space-between",
                      alignItems: "center",
                      padding: "0.2rem 0",
                    }}
                  >
                    <span>{s.label}</span>
                    <div
                      style={{
                        display: "flex",
                        gap: "0.4rem",
                        alignItems: "center",
                      }}
                    >
                      <strong>{formatMoney(s.total_cents, currency)}</strong>
                      <button
                        className="pos-btn pos-btn-accent"
                        style={{ padding: "0.18rem 0.45rem", fontSize: "0.72rem" }}
                        disabled={activeOrder.totals.remaining_balance_cents <= 0}
                        onClick={() =>
                          onPayOrder(
                            "card_tap_p2_p_e",
                            s.total_cents,
                            `${s.label} (${activeOrder.table_or_tab_name})`
                          )
                        }
                      >
                        Pay Seat
                      </button>
                    </div>
                  </div>
                ))}
              </div>
            )}

            {splitMode === "equal" && (
              <div
                style={{
                  backgroundColor: "var(--pos-bg)",
                  padding: "0.55rem",
                  borderRadius: "var(--pos-radius)",
                  fontSize: "0.8rem",
                }}
              >
                <div
                  style={{
                    display: "flex",
                    justifyContent: "space-between",
                    alignItems: "center",
                    marginBottom: "0.35rem",
                  }}
                >
                  <strong>Equal Ways:</strong>
                  <div style={{ display: "flex", gap: "0.25rem" }}>
                    {[2, 3, 4].map((w) => (
                      <button
                        key={w}
                        className={`pos-btn ${
                          splitWays === w ? "pos-btn-primary" : ""
                        }`}
                        style={{ padding: "0.15rem 0.45rem", fontSize: "0.72rem" }}
                        onClick={() => onSetSplitWays(w)}
                      >
                        {w}-Way
                      </button>
                    ))}
                  </div>
                </div>
                {customSplitShares.map((s) => (
                  <div
                    key={s.share_index}
                    style={{
                      display: "flex",
                      justifyContent: "space-between",
                      alignItems: "center",
                      padding: "0.2rem 0",
                    }}
                  >
                    <span>{s.label}</span>
                    <div
                      style={{
                        display: "flex",
                        gap: "0.4rem",
                        alignItems: "center",
                      }}
                    >
                      <strong>{formatMoney(s.total_cents, currency)}</strong>
                      <button
                        className="pos-btn pos-btn-accent"
                        style={{ padding: "0.18rem 0.45rem", fontSize: "0.72rem" }}
                        disabled={activeOrder.totals.remaining_balance_cents <= 0}
                        onClick={() =>
                          onPayOrder(
                            "card_tap_p2_p_e",
                            s.total_cents,
                            `${s.label} (${activeOrder.table_or_tab_name})`
                          )
                        }
                      >
                        Pay Share
                      </button>
                    </div>
                  </div>
                ))}
              </div>
            )}

            <div
              style={{
                display: "grid",
                gridTemplateColumns: "1fr 1fr",
                gap: "0.5rem",
              }}
            >
              <button
                className="pos-btn pos-btn-accent"
                disabled={activeOrder.totals.remaining_balance_cents <= 0}
                onClick={() => onPayOrder("card_tap_p2_p_e")}
              >
                💳{" "}
                {activeOrder.preauth_card
                  ? "Capture Pre-Auth"
                  : "P2PE Card Tap"}
              </button>
              <button
                className="pos-btn pos-btn-primary"
                disabled={activeOrder.totals.remaining_balance_cents <= 0}
                onClick={() => onPayOrder("cash")}
              >
                💵 Cash + Drawer Kick
              </button>
            </div>
          </div>
        </aside>
      )}
    </main>
  );
};

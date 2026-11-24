import React, { useMemo, useState } from "react";
import { formatMoney } from "../api";
import {
  CatalogItem,
  CategoryConfig,
  IndustryVertical,
  OrderView,
  PaymentMethod,
  SessionInfo,
  StoreTheme,
} from "../types";

interface CustomerSelfServiceViewProps {
  theme: StoreTheme;
  currentVertical: IndustryVertical;
  currency: string;
  categories: CategoryConfig[];
  catalog: CatalogItem[];
  orders: OrderView[];
  activeOrder: OrderView;
  session: SessionInfo | null;
  tipPercent: number;
  onSetTipPercent: (pct: number) => void;
  onSelectOrder: (orderId: string) => void;
  onAddItem: (item: CatalogItem, modifierIds: string[]) => Promise<void>;
  onPayOrder: (
    method: PaymentMethod,
    customPrincipalCents?: number,
    payerLabel?: string
  ) => void;
  onPrintReceipt: () => void;
}

export const CustomerSelfServiceView: React.FC<CustomerSelfServiceViewProps> = ({
  theme,
  currentVertical,
  currency,
  categories,
  catalog,
  orders,
  activeOrder,
  session,
  tipPercent,
  onSetTipPercent,
  onSelectOrder,
  onAddItem,
  onPayOrder,
  onPrintReceipt,
}) => {
  const [customerTab, setCustomerTab] = useState<"menu" | "pay">("menu");
  const [selectedCategory, setSelectedCategory] = useState<string>("all");
  const [selectedModifiersByItem, setSelectedModifiersByItem] = useState<
    Record<string, string[]>
  >({});

  const availableCatalog = useMemo(() => {
    return catalog.filter(
      (i) =>
        i.available &&
        (selectedCategory === "all" || i.category === selectedCategory)
    );
  }, [catalog, selectedCategory]);

  const toggleMod = (itemId: string, modId: string) => {
    setSelectedModifiersByItem((prev) => {
      const current = prev[itemId] || [];
      return {
        ...prev,
        [itemId]: current.includes(modId)
          ? current.filter((x) => x !== modId)
          : [...current, modId],
      };
    });
  };

  const isStrictlyScopedQr =
    session?.role === "customer" && Boolean(session.scoped_order_id);

  return (
    <main
      style={{
        maxWidth: 960,
        width: "100%",
        margin: "0 auto",
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
          justifyContent: "space-between",
          alignItems: "center",
          gap: "0.85rem",
        }}
      >
        <div>
          <span className="pos-badge">
            📱{" "}
            {isStrictlyScopedQr
              ? `HMAC-Signed QR Session • Scoped Strictly to ${session?.scoped_order_id}`
              : "Customer Self-Service (iOS / Android / Kiosk)"}
          </span>
          <h2 style={{ margin: "0.35rem 0 0.15rem 0" }}>
            Welcome to {theme.store_name}
          </h2>
          <div className="pos-muted" style={{ fontSize: "0.86rem" }}>
            Active Ticket: <strong>{activeOrder.table_or_tab_name}</strong> •{" "}
            {activeOrder.assigned_operator}
          </div>
        </div>

        <div
          style={{
            display: "flex",
            gap: "0.5rem",
            alignItems: "center",
            flexWrap: "wrap",
          }}
        >
          {!isStrictlyScopedQr && (
            <select
              className="pos-select"
              style={{ width: 230 }}
              value={activeOrder.id}
              onChange={(e) => onSelectOrder(e.target.value)}
            >
              {orders.map((o) => (
                <option key={o.id} value={o.id}>
                  Session: {o.table_or_tab_name}
                </option>
              ))}
            </select>
          )}

          <button
            className={`pos-btn ${
              customerTab === "menu" ? "pos-btn-primary" : ""
            }`}
            onClick={() => setCustomerTab("menu")}
          >
            📖 Browse & Add
          </button>
          <button
            className={`pos-btn ${
              customerTab === "pay" ? "pos-btn-accent" : ""
            }`}
            onClick={() => setCustomerTab("pay")}
          >
            💳 Pay Check (
            {formatMoney(activeOrder.totals.remaining_balance_cents, currency)})
          </button>
        </div>
      </div>

      {customerTab === "menu" ? (
        <>
          <div className="pos-card">
            <div style={{ display: "flex", flexWrap: "wrap", gap: "0.45rem" }}>
              <button
                className={`pos-btn ${
                  selectedCategory === "all" ? "pos-btn-primary" : ""
                }`}
                onClick={() => setSelectedCategory("all")}
              >
                All Menu
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
          </div>

          <div className="pos-grid-catalog">
            {availableCatalog.map((item) => {
              const selectedMods = selectedModifiersByItem[item.id] || [];
              return (
                <div key={item.id} className="pos-item-card">
                  <div>
                    <div
                      style={{
                        display: "flex",
                        justifyContent: "space-between",
                        alignItems: "center",
                      }}
                    >
                      <span style={{ fontSize: "1.6rem" }}>{item.badge_emoji}</span>
                      <span style={{ fontWeight: 800, fontSize: "1.05rem" }}>
                        {formatMoney(item.price_cents, currency)}
                      </span>
                    </div>
                    <h4 style={{ margin: "0.4rem 0 0.25rem 0" }}>{item.name}</h4>
                    <p
                      className="pos-muted"
                      style={{ fontSize: "0.82rem", margin: "0 0 0.6rem 0" }}
                    >
                      {item.description}
                    </p>
                    {item.modifiers.length > 0 && (
                      <div
                        style={{
                          display: "flex",
                          flexWrap: "wrap",
                          gap: "0.3rem",
                          marginBottom: "0.5rem",
                        }}
                      >
                        {item.modifiers.map((mod) => {
                          const active = selectedMods.includes(mod.id);
                          return (
                            <button
                              key={mod.id}
                              className={`pos-btn ${
                                active ? "pos-btn-secondary" : ""
                              }`}
                              style={{
                                fontSize: "0.74rem",
                                padding: "0.22rem 0.5rem",
                              }}
                              onClick={() => toggleMod(item.id, mod.id)}
                            >
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
                      activeOrder.status === "paid" ||
                      activeOrder.status === "refunded"
                    }
                    onClick={() => onAddItem(item, selectedMods)}
                  >
                    + Add to {activeOrder.table_or_tab_name}
                  </button>
                </div>
              );
            })}
          </div>
        </>
      ) : (
        <div
          className="pos-card"
          style={{ display: "flex", flexDirection: "column", gap: "1rem" }}
        >
          <div
            style={{
              textAlign: "center",
              borderBottom: "1px solid rgba(148,163,184,0.2)",
              paddingBottom: "0.75rem",
            }}
          >
            <div style={{ fontWeight: 800, letterSpacing: "0.04em" }}>
              {theme.receipt_header}
            </div>
            <div className="pos-muted" style={{ fontSize: "0.84rem" }}>
              Ticket: {activeOrder.table_or_tab_name}
            </div>
          </div>

          <div
            style={{ display: "flex", flexDirection: "column", gap: "0.5rem" }}
          >
            {activeOrder.items.map((line) => {
              const modDelta = line.selected_modifiers.reduce(
                (acc, m) => acc + m.price_delta_cents,
                0
              );
              const total = line.comped
                ? 0
                : (line.unit_price_cents + modDelta) * line.quantity;
              return (
                <div
                  key={line.id}
                  style={{
                    display: "flex",
                    justifyContent: "space-between",
                    padding: "0.45rem 0",
                    borderBottom: "1px dashed rgba(148,163,184,0.18)",
                  }}
                >
                  <div>
                    <strong>
                      {line.quantity}x {line.name}
                    </strong>{" "}
                    {line.comped && <span className="pos-badge">COMPED</span>}
                    {line.seat_number && currentVertical === "hospitality" ? (
                      <span className="pos-badge">Seat {line.seat_number}</span>
                    ) : null}
                  </div>
                  <strong>{formatMoney(total, currency)}</strong>
                </div>
              );
            })}
          </div>

          <div
            style={{ display: "flex", flexDirection: "column", gap: "0.35rem" }}
          >
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
                <span>{adj.label}</span>
                <span>{formatMoney(adj.amount_cents, currency)}</span>
              </div>
            ))}
            <div style={{ display: "flex", justifyContent: "space-between" }}>
              <span className="pos-muted">Tax</span>
              <span>{formatMoney(activeOrder.totals.tax_cents, currency)}</span>
            </div>
            <div
              style={{
                display: "flex",
                justifyContent: "space-between",
                fontSize: "1.2rem",
                fontWeight: 800,
              }}
            >
              <span>Remaining Balance</span>
              <span>
                {formatMoney(
                  activeOrder.totals.remaining_balance_cents,
                  currency
                )}
              </span>
            </div>
          </div>

          {currentVertical !== "retail" && (
            <div>
              <div style={{ fontWeight: 700, marginBottom: "0.45rem" }}>
                Add a Gratuity / Tip for {activeOrder.assigned_operator}:
              </div>
              <div style={{ display: "flex", gap: "0.5rem", flexWrap: "wrap" }}>
                {[15, 18, 20, 25, 0].map((pct) => (
                  <button
                    key={pct}
                    className={`pos-btn ${
                      tipPercent === pct ? "pos-btn-primary" : ""
                    }`}
                    onClick={() => onSetTipPercent(pct)}
                  >
                    {pct === 0
                      ? "No Tip"
                      : `${pct}% (${formatMoney(
                          Math.round(
                            (activeOrder.totals.remaining_balance_cents * pct) /
                              100
                          ),
                          currency
                        )})`}
                  </button>
                ))}
              </div>
            </div>
          )}

          <div
            style={{
              display: "grid",
              gridTemplateColumns: "repeat(auto-fit, minmax(190px, 1fr))",
              gap: "0.65rem",
            }}
          >
            <button
              className="pos-btn pos-btn-accent"
              disabled={activeOrder.totals.remaining_balance_cents <= 0}
              onClick={() => onPayOrder("apple_pay")}
            >
              🍎 Apple Pay (Full Balance)
            </button>
            <button
              className="pos-btn pos-btn-primary"
              disabled={activeOrder.totals.remaining_balance_cents <= 0}
              onClick={() => onPayOrder("google_pay")}
            >
              🤖 Google Pay / Tap Card
            </button>
            {activeOrder.split_by_seat.length > 1 &&
              activeOrder.totals.remaining_balance_cents > 0 && (
                <button
                  className="pos-btn pos-btn-secondary"
                  onClick={() =>
                    onPayOrder(
                      "card_tap_p2_p_e",
                      activeOrder.split_by_seat[0].total_cents,
                      `Customer (${activeOrder.split_by_seat[0].label})`
                    )
                  }
                >
                  🪑 Pay {activeOrder.split_by_seat[0].label} (
                  {formatMoney(
                    activeOrder.split_by_seat[0].total_cents,
                    currency
                  )}
                  )
                </button>
              )}
            <button className="pos-btn" onClick={onPrintReceipt}>
              🧾 View Digital Receipt
            </button>
          </div>

          <div
            className="pos-muted"
            style={{
              textAlign: "center",
              fontSize: "0.82rem",
              marginTop: "0.5rem",
            }}
          >
            {theme.receipt_footer}
          </div>
        </div>
      )}
    </main>
  );
};

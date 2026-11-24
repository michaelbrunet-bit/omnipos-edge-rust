import React from "react";
import {
  IndustryVertical,
  Role,
  SessionInfo,
  StoreSummaryView,
  StoreTheme,
} from "../types";

interface HeaderBarProps {
  theme: StoreTheme;
  supportedPlatforms: string[];
  stores: StoreSummaryView[];
  activeStoreId: string;
  currentVertical: IndustryVertical;
  activeRole: Role;
  session: SessionInfo | null;
  offlineQueueCount: number;
  onActivateStore: (storeId: string) => void;
  onSelectRole: (role: Role) => void;
  onOpenPinModal: () => void;
}

export const HeaderBar: React.FC<HeaderBarProps> = ({
  theme,
  supportedPlatforms,
  stores,
  activeStoreId,
  currentVertical,
  activeRole,
  session,
  offlineQueueCount,
  onActivateStore,
  onSelectRole,
  onOpenPinModal,
}) => {
  const roleLabels = {
    bartender:
      currentVertical === "salon_barber"
        ? "💈 Front Desk & Express"
        : currentVertical === "retail"
        ? "🏪 Fast Cashier / Scanner"
        : "🍸 Bartender (Toast Tabs)",
    server:
      currentVertical === "salon_barber"
        ? "✂️ Barber / Stylist Chair"
        : currentVertical === "retail"
        ? "🛍️ Clienteling Associate"
        : "🍽️ Server (Aloha Pivot)",
    customer:
      currentVertical === "salon_barber"
        ? "📱 Client Check-In & Pay"
        : currentVertical === "retail"
        ? "📱 Self-Checkout Kiosk"
        : "📱 Customer QR Order & Pay",
    owner: "👑 Store Owner Studio",
  };

  return (
    <header className="pos-header">
      <div className="pos-brand">
        <div className="pos-logo-badge">{theme.logo_text}</div>
        <div>
          <div style={{ fontWeight: 800, fontSize: "1.12rem" }}>
            {theme.store_name}
          </div>
          <div className="pos-muted" style={{ fontSize: "0.76rem" }}>
            {theme.tagline} • Targets: {supportedPlatforms.join(" • ")}
          </div>
        </div>
      </div>

      {/* Non-Destructive Multi-Store Partition Switcher */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: "0.35rem",
          flexWrap: "wrap",
          backgroundColor: "var(--pos-bg)",
          padding: "0.3rem",
          borderRadius: "var(--pos-radius)",
        }}
      >
        <span
          className="pos-muted"
          style={{ fontSize: "0.73rem", padding: "0 0.35rem", fontWeight: 700 }}
        >
          STORE PARTITION:
        </span>
        {stores.map((st) => {
          const icon =
            st.vertical === "salon_barber"
              ? "💈"
              : st.vertical === "retail"
              ? "🛍️"
              : "🥃";
          const shortLabel =
            st.vertical === "salon_barber"
              ? "Barber & Salon"
              : st.vertical === "retail"
              ? "Retail Store"
              : "Hospitality (Aloha+Toast)";
          return (
            <button
              key={st.store_id}
              className={`pos-btn ${
                activeStoreId === st.store_id ? "pos-btn-primary" : ""
              }`}
              style={{ padding: "0.32rem 0.6rem", fontSize: "0.76rem" }}
              disabled={session?.role === "customer"}
              title={
                session?.role === "customer"
                  ? "Customer QR sessions are locked to their store & table"
                  : `Switch to ${st.store_name} (zero data loss)`
              }
              onClick={() => onActivateStore(st.store_id)}
            >
              {icon} {shortLabel}
            </button>
          );
        })}
      </div>

      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: "0.45rem",
          flexWrap: "wrap",
        }}
      >
        {offlineQueueCount > 0 && (
          <span
            className="pos-badge"
            style={{
              backgroundColor: "#d97706",
              color: "#fff",
              fontWeight: 700,
            }}
          >
            📡 Offline Outbox: {offlineQueueCount} queued
          </span>
        )}

        <button
          className="pos-btn"
          style={{ padding: "0.32rem 0.6rem", fontSize: "0.76rem" }}
          onClick={onOpenPinModal}
          title="Switch authenticated Argon2id operator PIN session"
        >
          🔐{" "}
          {session
            ? `${session.actor_name.split(" (")[0]} (${session.role.toUpperCase()})`
            : "PIN Sign In"}
        </button>

        <nav className="pos-role-switcher" aria-label="POS Role Switcher">
          <button
            className={`pos-btn ${
              activeRole === "bartender" ? "pos-btn-primary" : ""
            }`}
            onClick={() => onSelectRole("bartender")}
          >
            {roleLabels.bartender}
          </button>
          <button
            className={`pos-btn ${
              activeRole === "server" ? "pos-btn-primary" : ""
            }`}
            onClick={() => onSelectRole("server")}
          >
            {roleLabels.server}
          </button>
          <button
            className={`pos-btn ${
              activeRole === "customer" ? "pos-btn-accent" : ""
            }`}
            onClick={() => onSelectRole("customer")}
          >
            {roleLabels.customer}
          </button>
          <button
            className={`pos-btn ${
              activeRole === "owner" ? "pos-btn-secondary" : ""
            }`}
            onClick={() => onSelectRole("owner")}
          >
            {roleLabels.owner}
          </button>
        </nav>
      </div>
    </header>
  );
};

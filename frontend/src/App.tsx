import React, { useCallback, useEffect, useMemo, useState } from "react";
import {
  formatMoney,
  getActiveSession,
  posApi,
  PosApiError,
  setActiveSession,
} from "./api";
import { HeaderBar } from "./components/HeaderBar";
import { ManagerOverrideModal } from "./components/ManagerOverrideModal";
import { PinLoginModal } from "./components/PinLoginModal";
import { ReceiptPreviewModal } from "./components/ReceiptPreviewModal";
import { CustomerSelfServiceView } from "./features/CustomerSelfServiceView";
import { OperatorPosView } from "./features/OperatorPosView";
import { OwnerConsoleView } from "./features/OwnerConsoleView";
import {
  flushOfflineQueue,
  loadOfflineQueue,
  resolveWsUrl,
} from "./offlineQueue";
import { ThemeProvider } from "./theme/ThemeProvider";
import {
  BootstrapResponse,
  CatalogItem,
  CategoryConfig,
  IndustryVertical,
  ModifierQualifier,
  OrderSource,
  OrderView,
  PaymentMethod,
  RenderedReceipt,
  Role,
  SessionInfo,
  SplitCheckShare,
  StoreTheme,
} from "./types";

const DEMO_PIN_BY_ROLE: Record<string, string> = {
  owner: "1111",
  manager: "9999",
  bartender: "2222",
  server: "3333",
};

export const App: React.FC = () => {
  const [data, setData] = useState<BootstrapResponse | null>(null);
  const [loading, setLoading] = useState<boolean>(true);
  const [error, setError] = useState<string | null>(null);
  const [toast, setToast] = useState<string | null>(null);

  const [session, setSession] = useState<SessionInfo | null>(() =>
    getActiveSession()
  );
  const [activeRole, setActiveRole] = useState<Role>(
    () => getActiveSession()?.role || "bartender"
  );
  const [selectedOrderId, setSelectedOrderId] = useState<string>("");

  const [splitMode, setSplitMode] = useState<"none" | "seat" | "equal">("none");
  const [splitWays, setSplitWays] = useState<number>(2);
  const [customSplitShares, setCustomSplitShares] = useState<SplitCheckShare[]>(
    []
  );
  const [tipPercent, setTipPercent] = useState<number>(20);
  const [draftTheme, setDraftTheme] = useState<StoreTheme | null>(null);

  // Modals & Offline Outbox count
  const [pinModalOpen, setPinModalOpen] = useState<boolean>(false);
  const [overridePrompt, setOverridePrompt] = useState<{
    action: string;
    onApproved: (overrideToken: string) => Promise<void>;
  } | null>(null);
  const [previewReceipt, setPreviewReceipt] = useState<RenderedReceipt | null>(
    null
  );
  const [previewQrUrl, setPreviewQrUrl] = useState<string | null>(null);
  const [offlineCount, setOfflineCount] = useState<number>(
    () => loadOfflineQueue().length
  );

  const showToast = useCallback((msg: string) => {
    setToast(msg);
    setOfflineCount(loadOfflineQueue().length);
    setTimeout(() => {
      setToast((prev) => (prev === msg ? null : prev));
    }, 3600);
  }, []);

  const refreshState = useCallback(async () => {
    try {
      const res = await posApi.bootstrap();
      setData(res);
      setDraftTheme((prev) =>
        activeRole === "owner" && prev ? prev : res.theme
      );
      setSelectedOrderId((prev) => {
        if (prev && res.orders.some((o) => o.id === prev)) {
          return prev;
        }
        return res.orders[0]?.id || "";
      });
      setOfflineCount(loadOfflineQueue().length);
      setError(null);
    } catch (err) {
      setError(
        err instanceof Error
          ? err.message
          : "Unable to connect to POS Rust Edge Server"
      );
    } finally {
      setLoading(false);
    }
  }, [activeRole]);

  // Boot: adopt signed QR URL token if present, or establish initial signed Operator session
  useEffect(() => {
    const init = async () => {
      const params = new URLSearchParams(window.location.search);
      const qrToken = params.get("qr_token");
      const qrOrder = params.get("order");
      if (qrToken && qrOrder) {
        const qrSess: SessionInfo = {
          token: qrToken,
          actor_id: `qr-guest-${qrOrder}`,
          actor_name: "QR Guest",
          role: "customer",
          store_id: "store-hospitality",
          scoped_order_id: qrOrder,
          exp_unix: Math.floor(Date.now() / 1000) + 14400,
        };
        setActiveSession(qrSess);
        setSession(qrSess);
        setActiveRole("customer");
        setSelectedOrderId(qrOrder);
      } else if (!getActiveSession()) {
        try {
          const s = await posApi.login("bartender", "2222");
          setSession(s);
          setActiveRole(s.role);
        } catch {
          // Proceed to bootstrap even if initial auto-login deferred
        }
      }
      await refreshState();
    };
    init();
  }, [refreshState]);

  // Sequence-numbered WebSocket delta listener + Offline Outbox auto-flush
  useEffect(() => {
    let ws: WebSocket | null = null;
    try {
      ws = new WebSocket(resolveWsUrl());
      ws.onopen = async () => {
        const tok = getActiveSession()?.token || null;
        const { flushed } = await flushOfflineQueue(tok);
        if (flushed > 0) {
          showToast(`Synced ${flushed} queued offline action(s) to Edge Hub!`);
        }
        setOfflineCount(loadOfflineQueue().length);
      };
      ws.onmessage = () => {
        refreshState();
      };
    } catch {
      // Ignore offline WS errors
    }

    const onOnline = async () => {
      const tok = getActiveSession()?.token || null;
      const { flushed } = await flushOfflineQueue(tok);
      if (flushed > 0) {
        showToast(`Synced ${flushed} queued offline action(s)!`);
        await refreshState();
      }
    };
    window.addEventListener("online", onOnline);
    return () => {
      window.removeEventListener("online", onOnline);
      ws?.close();
    };
  }, [refreshState, showToast]);

  const activeOrder: OrderView | undefined = useMemo(() => {
    if (!data) return undefined;
    return (
      data.orders.find((o) => o.id === selectedOrderId) || data.orders[0]
    );
  }, [data, selectedOrderId]);

  useEffect(() => {
    if (!activeOrder || splitMode !== "equal") {
      setCustomSplitShares([]);
      return;
    }
    posApi
      .splitOrder(activeOrder.id, splitWays)
      .then(setCustomSplitShares)
      .catch(() => setCustomSplitShares([]));
  }, [activeOrder, splitMode, splitWays]);

  const requireManagerOverride = useCallback(
    (
      actionLabel: string,
      callback: (overrideToken: string) => Promise<void>
    ) => {
      setOverridePrompt({ action: actionLabel, onApproved: callback });
    },
    []
  );

  const handleAuthenticate = async (
    role: Role,
    pin?: string,
    scopedOrderId?: string
  ) => {
    const s = await posApi.login(role, pin, scopedOrderId);
    setSession(s);
    setActiveRole(s.role === "manager" ? "owner" : s.role);
    showToast(`Signed session active: ${s.actor_name}`);
    await refreshState();
  };

  const handleSelectRole = async (targetRole: Role) => {
    if (!data) return;
    try {
      if (targetRole === "customer") {
        const orderToScope = activeOrder?.id || data.orders[0]?.id;
        if (orderToScope) {
          const s = await posApi.login("customer", undefined, orderToScope);
          setSession(s);
        }
      } else {
        const pin = DEMO_PIN_BY_ROLE[targetRole] || "2222";
        const s = await posApi.login(targetRole, pin);
        setSession(s);
      }
      setActiveRole(targetRole);
      if (targetRole === "owner") {
        setDraftTheme(data.theme);
      }
      await refreshState();
    } catch (err) {
      showToast(err instanceof Error ? err.message : "Role switch requires PIN");
      setPinModalOpen(true);
    }
  };

  const handleActivateStore = async (storeId: string) => {
    try {
      await posApi.activateStore(storeId);
      setSplitMode("none");
      showToast(`Activated store partition '${storeId}' (zero data loss)!`);
      await refreshState();
    } catch (err) {
      showToast(err instanceof Error ? err.message : "Failed to switch store");
    }
  };

  const handleCreateOrder = async (
    name: string,
    guests: number,
    isBarTab: boolean
  ) => {
    if (!data) return;
    const v = data.theme.vertical;
    const defaultPrefix =
      v === "salon_barber"
        ? "Chair Appt"
        : v === "retail"
        ? "Retail Bag"
        : isBarTab
        ? "Bar Tab"
        : "Table";
    const label = name.trim() || `${defaultPrefix} #${data.orders.length + 1}`;
    try {
      const created = await posApi.createOrder({
        table_or_tab_name: label,
        is_bar_tab: isBarTab,
        guest_count: guests,
        assigned_operator:
          v === "salon_barber"
            ? "Devon Brooks (Master Barber)"
            : v === "retail"
            ? "Lucas Grant (Cashier)"
            : isBarTab
            ? "Marcus Cole"
            : "Sofia Chen",
      });
      setSelectedOrderId(created.id);
      showToast(`Opened ${created.table_or_tab_name}`);
      await refreshState();
    } catch (err) {
      showToast(err instanceof Error ? err.message : "Failed to open ticket");
    }
  };

  const handleAddItem = async (
    item: CatalogItem,
    modifierIds: string[],
    qualifier: ModifierQualifier = "regular",
    seat = 1,
    note = ""
  ) => {
    if (!activeOrder || !data) return;
    const source: OrderSource =
      activeRole === "bartender"
        ? "bartender_speed_rail"
        : activeRole === "server"
        ? "server_tableside"
        : "customer_self_service";

    try {
      await posApi.addOrderItem(activeOrder.id, {
        catalog_item_id: item.id,
        quantity: 1,
        modifier_ids: modifierIds,
        modifier_qualifier: qualifier,
        seat_number:
          data.theme.vertical === "hospitality" && activeRole === "server"
            ? seat
            : 1,
        notes: note.trim() ? note.trim() : null,
        added_by: session?.actor_name || activeRole,
        source,
      });
      showToast(`Added ${item.name} to ${activeOrder.table_or_tab_name}`);
      await refreshState();
    } catch (err) {
      showToast(err instanceof Error ? err.message : "Failed to add item");
    }
  };

  const handleRemoveLineItem = async (
    lineId: string,
    overrideToken?: string
  ) => {
    if (!activeOrder) return;
    try {
      await posApi.removeOrderItem(activeOrder.id, lineId, overrideToken);
      showToast("Item voided from ticket");
      await refreshState();
    } catch (err) {
      if (
        err instanceof PosApiError &&
        err.code === "MANAGER_OVERRIDE_REQUIRED"
      ) {
        requireManagerOverride(
          err.requiredAction || "Void Fired/Prepared Item",
          async (tok) => {
            await handleRemoveLineItem(lineId, tok);
          }
        );
        return;
      }
      showToast(err instanceof Error ? err.message : "Failed to void item");
    }
  };

  const handleCompLineItem = async (
    lineId: string,
    overrideToken?: string
  ) => {
    if (!activeOrder) return;
    try {
      await posApi.compOrderItem(
        activeOrder.id,
        lineId,
        "VIP Guest Courtesy",
        overrideToken
      );
      showToast("Updated item Comp status");
      await refreshState();
    } catch (err) {
      if (
        err instanceof PosApiError &&
        err.code === "MANAGER_OVERRIDE_REQUIRED"
      ) {
        requireManagerOverride(
          err.requiredAction || "Comp Line Item",
          async (tok) => {
            await handleCompLineItem(lineId, tok);
          }
        );
        return;
      }
      showToast(err instanceof Error ? err.message : "Failed to comp item");
    }
  };

  const handleRefundPayment = async (
    paymentId: string,
    amountCents: number,
    overrideToken?: string
  ) => {
    if (!activeOrder) return;
    try {
      await posApi.refundOrder(
        activeOrder.id,
        {
          payment_id: paymentId,
          amount_cents: amountCents,
          reason: "Customer return / adjustment",
        },
        overrideToken
      );
      showToast(
        `Refunded ${formatMoney(amountCents, data?.theme.currency_symbol)} & posted reversal journal!`
      );
      await refreshState();
    } catch (err) {
      if (
        err instanceof PosApiError &&
        err.code === "MANAGER_OVERRIDE_REQUIRED"
      ) {
        requireManagerOverride(
          err.requiredAction || "Issue Payment Refund",
          async (tok) => {
            await handleRefundPayment(paymentId, amountCents, tok);
          }
        );
        return;
      }
      showToast(err instanceof Error ? err.message : "Refund failed");
    }
  };

  const handlePayOrder = async (
    method: PaymentMethod,
    customPrincipalCents?: number,
    payerLabel?: string
  ) => {
    if (!activeOrder || !data) return;
    const principal =
      customPrincipalCents ?? activeOrder.totals.remaining_balance_cents;
    if (principal <= 0) {
      showToast("This ticket is already paid in full.");
      return;
    }
    const tipCents =
      activeRole === "customer" ? Math.round((principal * tipPercent) / 100) : 0;
    try {
      await posApi.payOrder(activeOrder.id, {
        amount_cents: principal,
        tip_cents: tipCents,
        method,
        payer_label:
          payerLabel || session?.actor_name || `Operator (${activeRole})`,
      });
      showToast(
        `Captured ${formatMoney(
          principal + tipCents,
          data.theme.currency_symbol
        )} & posted balanced ledger entry!`
      );
      await refreshState();
    } catch (err) {
      showToast(err instanceof Error ? err.message : "Payment failed");
    }
  };

  const handlePrintReceipt = async (kickDrawer: boolean) => {
    if (!activeOrder) return;
    try {
      const rendered = await posApi.printOrderReceipt(
        activeOrder.id,
        kickDrawer
      );
      setPreviewQrUrl(null);
      setPreviewReceipt(rendered);
    } catch (err) {
      showToast(err instanceof Error ? err.message : "Receipt print error");
    }
  };

  const handleMintQrLink = async () => {
    if (!activeOrder) return;
    try {
      const res = await posApi.mintOrderQrLink(activeOrder.id);
      const fullUrl = `${window.location.origin}${res.qr_url_path}`;
      setPreviewReceipt(null);
      setPreviewQrUrl(fullUrl);
    } catch (err) {
      showToast(err instanceof Error ? err.message : "QR link error");
    }
  };

  if (loading && !data) {
    return (
      <div style={{ padding: "2.5rem", textAlign: "center" }}>
        <h2>Booting POS Edge Runtime...</h2>
      </div>
    );
  }

  if (error || !data) {
    return (
      <div style={{ padding: "2.5rem", maxWidth: 640, margin: "0 auto" }}>
        <h2>POS Edge Connection Notice</h2>
        <p>{error}</p>
        <button className="pos-btn pos-btn-primary" onClick={refreshState}>
          Retry Connection
        </button>
      </div>
    );
  }

  const activeTheme =
    draftTheme && activeRole === "owner" ? draftTheme : data.theme;
  const currency = activeTheme.currency_symbol || "$";
  const currentVertical: IndustryVertical = data.theme.vertical;

  return (
    <ThemeProvider theme={activeTheme}>
      <HeaderBar
        theme={activeTheme}
        supportedPlatforms={data.supported_platforms}
        stores={data.stores}
        activeStoreId={data.active_store_id}
        currentVertical={currentVertical}
        activeRole={activeRole}
        session={session}
        offlineQueueCount={offlineCount}
        onActivateStore={handleActivateStore}
        onSelectRole={handleSelectRole}
        onOpenPinModal={() => setPinModalOpen(true)}
      />

      {toast && (
        <div
          style={{
            position: "fixed",
            bottom: 20,
            right: 20,
            zIndex: 1000,
            backgroundColor: "var(--pos-primary)",
            color: "#fff",
            padding: "0.75rem 1.2rem",
            borderRadius: "var(--pos-radius)",
            fontWeight: 700,
            boxShadow: "0 10px 25px rgba(0,0,0,0.35)",
          }}
        >
          {toast}
        </div>
      )}

      <PinLoginModal
        isOpen={pinModalOpen}
        operators={data.operators}
        initialRole={activeRole}
        scopedOrderId={activeOrder?.id || ""}
        onClose={() => setPinModalOpen(false)}
        onAuthenticate={handleAuthenticate}
      />

      <ManagerOverrideModal
        requiredAction={overridePrompt?.action || null}
        onCancel={() => setOverridePrompt(null)}
        onApproved={async (tok, mgrName) => {
          const cb = overridePrompt?.onApproved;
          setOverridePrompt(null);
          showToast(`Manager Override signed by ${mgrName}`);
          if (cb) {
            await cb(tok);
          }
        }}
      />

      <ReceiptPreviewModal
        receipt={previewReceipt}
        qrUrl={previewQrUrl}
        onClose={() => {
          setPreviewReceipt(null);
          setPreviewQrUrl(null);
        }}
      />

      {(activeRole === "bartender" || activeRole === "server") && (
        <OperatorPosView
          currentVertical={currentVertical}
          activeRole={activeRole}
          currency={currency}
          categories={data.categories}
          catalog={data.catalog}
          orders={data.orders}
          activeOrder={activeOrder}
          customSplitShares={customSplitShares}
          splitMode={splitMode}
          splitWays={splitWays}
          onSelectOrder={setSelectedOrderId}
          onCreateOrder={handleCreateOrder}
          onAddItem={handleAddItem}
          onRemoveLineItem={(lineId) => handleRemoveLineItem(lineId)}
          onCompLineItem={(lineId) => handleCompLineItem(lineId)}
          onAdvanceItemPrep={async (lineId) => {
            if (!activeOrder) return;
            await posApi.advanceItemPrep(activeOrder.id, lineId);
            await refreshState();
          }}
          onBindPreauth={async () => {
            if (!activeOrder) return;
            const last4 = Math.floor(1000 + Math.random() * 9000);
            await posApi.bindPreauthCard(activeOrder.id, `Visa ••${last4}`);
            showToast(`Bound P2PE Pre-Auth to ${activeOrder.table_or_tab_name}`);
            await refreshState();
          }}
          onFireOrder={async () => {
            if (!activeOrder) return;
            await posApi.fireOrder(activeOrder.id);
            showToast(`Fired held items for ${activeOrder.table_or_tab_name}!`);
            await refreshState();
          }}
          onSetSplitMode={setSplitMode}
          onSetSplitWays={setSplitWays}
          onPayOrder={handlePayOrder}
          onRefundPayment={(pid, amt) => handleRefundPayment(pid, amt)}
          onPrintReceipt={handlePrintReceipt}
          onMintQrLink={handleMintQrLink}
          onToast={showToast}
        />
      )}

      {activeRole === "customer" && activeOrder && (
        <CustomerSelfServiceView
          theme={activeTheme}
          currentVertical={currentVertical}
          currency={currency}
          categories={data.categories}
          catalog={data.catalog}
          orders={data.orders}
          activeOrder={activeOrder}
          session={session}
          tipPercent={tipPercent}
          onSetTipPercent={setTipPercent}
          onSelectOrder={setSelectedOrderId}
          onAddItem={(item, modIds) => handleAddItem(item, modIds)}
          onPayOrder={handlePayOrder}
          onPrintReceipt={() => handlePrintReceipt(false)}
        />
      )}

      {activeRole === "owner" && draftTheme && (
        <OwnerConsoleView
          currency={currency}
          draftTheme={draftTheme}
          themePresets={data.theme_presets}
          categories={data.categories}
          catalog={data.catalog}
          mods={data.mods}
          auditReport={data.audit_report}
          recentAudit={data.recent_audit}
          onChangeDraftTheme={setDraftTheme}
          onApplyPreset={async (presetId) => {
            const updated = await posApi.applyThemePreset(presetId);
            setDraftTheme(updated);
            showToast(`Applied theme preset: ${updated.store_name}`);
            await refreshState();
          }}
          onSaveTheme={async () => {
            const saved = await posApi.updateTheme(draftTheme);
            setDraftTheme(saved);
            showToast("Store theme saved & broadcast to all terminals!");
            await refreshState();
          }}
          onToggleMod={async (modId, enabled) => {
            const updated = await posApi.toggleMod(modId, enabled);
            showToast(
              `${updated.name} ${updated.enabled ? "ENABLED" : "DISABLED"}!`
            );
            await refreshState();
          }}
          onToggleAvailability={async (itemId) => {
            const updated = await posApi.toggleCatalogAvailability(itemId);
            showToast(
              `${updated.name} is now ${
                updated.available ? "Available" : "86'd"
              }`
            );
            await refreshState();
          }}
          onCreateCategory={async (cat: CategoryConfig) => {
            await posApi.createCategory(cat);
            showToast(`Added custom category: ${cat.label}`);
            await refreshState();
          }}
          onRequireManagerOverride={requireManagerOverride}
          onShowReceipt={(r) => {
            setPreviewQrUrl(null);
            setPreviewReceipt(r);
          }}
          onToast={showToast}
          onRefreshBootstrap={refreshState}
        />
      )}
    </ThemeProvider>
  );
};

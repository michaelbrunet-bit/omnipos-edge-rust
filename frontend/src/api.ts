import {
  enqueueOfflineMutation,
  generateIdempotencyKey,
  resolveApiBase,
} from "./offlineQueue";
import {
  BootstrapResponse,
  CashDrawerShift,
  CashMovementKind,
  CatalogItem,
  CategoryConfig,
  HalStatusSnapshot,
  IndustryVertical,
  LedgerSummaryResponse,
  ModifierQualifier,
  ModManifest,
  OrderSource,
  OrderView,
  PaymentMethod,
  RenderedReceipt,
  Role,
  SessionInfo,
  ShiftReport,
  ShiftsSummaryResponse,
  SplitCheckShare,
  StoreTheme,
} from "./types";

const SESSION_STORAGE_KEY = "pos_active_session_v2";

export class PosApiError extends Error {
  status: number;
  code?: string;
  requiredAction?: string;

  constructor(
    message: string,
    status: number,
    code?: string,
    requiredAction?: string
  ) {
    super(message);
    this.name = "PosApiError";
    this.status = status;
    this.code = code;
    this.requiredAction = requiredAction;
  }
}

let activeSession: SessionInfo | null = (() => {
  try {
    const raw = sessionStorage.getItem(SESSION_STORAGE_KEY);
    return raw ? (JSON.parse(raw) as SessionInfo) : null;
  } catch {
    return null;
  }
})();

export function getActiveSession(): SessionInfo | null {
  return activeSession;
}

export function setActiveSession(session: SessionInfo | null): void {
  activeSession = session;
  try {
    if (session) {
      sessionStorage.setItem(SESSION_STORAGE_KEY, JSON.stringify(session));
    } else {
      sessionStorage.removeItem(SESSION_STORAGE_KEY);
    }
  } catch {
    // Ignore storage errors
  }
}

export function formatMoney(cents: number, symbol = "$"): string {
  const negative = cents < 0;
  const abs = Math.abs(cents) / 100;
  return `${negative ? "-" : ""}${symbol}${abs.toFixed(2)}`;
}

interface RequestOptions extends RequestInit {
  managerOverrideToken?: string;
  queueIfOffline?: boolean;
}

async function requestJson<T>(
  path: string,
  options?: RequestOptions
): Promise<T> {
  const method = (options?.method || "GET").toUpperCase();
  const isMutation = method !== "GET" && method !== "HEAD";
  const idemKey = isMutation ? generateIdempotencyKey() : undefined;

  const headers: Record<string, string> = {
    "Content-Type": "application/json",
  };
  if (activeSession?.token) {
    headers["Authorization"] = `Bearer ${activeSession.token}`;
  }
  if (idemKey) {
    headers["Idempotency-Key"] = idemKey;
  }
  if (options?.managerOverrideToken) {
    headers["X-Manager-Override"] = options.managerOverrideToken;
  }

  let res: Response;
  try {
    res = await fetch(`${resolveApiBase()}${path}`, {
      ...options,
      method,
      headers: {
        ...headers,
        ...(options?.headers as Record<string, string> | undefined),
      },
    });
  } catch (networkErr) {
    if (isMutation && options?.queueIfOffline && idemKey) {
      enqueueOfflineMutation({
        idempotencyKey: idemKey,
        path,
        method,
        body: typeof options.body === "string" ? options.body : null,
      });
      throw new PosApiError(
        "Terminal offline — action safely queued in local Sync Outbox with Idempotency-Key!",
        0,
        "QUEUED_OFFLINE"
      );
    }
    throw networkErr;
  }

  if (!res.ok) {
    const errBody = await res
      .json()
      .catch(() => ({ error: res.statusText || `HTTP ${res.status}` }));
    throw new PosApiError(
      errBody.error || `HTTP ${res.status}`,
      res.status,
      errBody.code,
      errBody.required_action
    );
  }
  return res.json();
}

export const posApi = {
  bootstrap: () => requestJson<BootstrapResponse>("/bootstrap"),

  login: async (role: Role, pin?: string, scoped_order_id?: string) => {
    const res = await requestJson<SessionInfo>("/auth/login", {
      method: "POST",
      body: JSON.stringify({ role, pin, scoped_order_id }),
    });
    setActiveSession(res);
    return res;
  },

  requestManagerOverride: (pin: string, action: string) =>
    requestJson<{
      override_token: string;
      manager_name: string;
      approved_action: string;
      exp_unix: number;
    }>("/auth/manager-override", {
      method: "POST",
      body: JSON.stringify({ pin, action }),
    }),

  mintOrderQrLink: (orderId: string) =>
    requestJson<{
      order_id: string;
      token: string;
      qr_url_path: string;
      exp_unix: number;
    }>(`/orders/${orderId}/qr-link`, {
      method: "POST",
    }),

  switchVertical: (vertical: IndustryVertical) =>
    requestJson<StoreTheme>(`/vertical/${vertical}`, {
      method: "POST",
    }),

  activateStore: (storeId: string) =>
    requestJson<{ store_id: string; theme: StoreTheme }>(
      `/stores/${storeId}/activate`,
      {
        method: "POST",
      }
    ),

  updateTheme: (theme: StoreTheme) =>
    requestJson<StoreTheme>("/theme", {
      method: "PUT",
      body: JSON.stringify(theme),
    }),

  applyThemePreset: (presetId: string) =>
    requestJson<StoreTheme>(`/theme/preset/${presetId}`, {
      method: "POST",
    }),

  createCategory: (category: CategoryConfig) =>
    requestJson<unknown>("/catalog/categories", {
      method: "POST",
      body: JSON.stringify(category),
    }),

  toggleCatalogAvailability: (itemId: string) =>
    requestJson<CatalogItem>(`/catalog/${itemId}/toggle-availability`, {
      method: "PATCH",
    }),

  createCatalogItem: (item: CatalogItem) =>
    requestJson<CatalogItem>("/catalog", {
      method: "POST",
      body: JSON.stringify(item),
    }),

  createOrder: (payload: {
    table_or_tab_name: string;
    is_bar_tab: boolean;
    guest_count: number;
    assigned_operator: string;
    preauth_card?: string | null;
    client_profile_notes?: string | null;
  }) =>
    requestJson<OrderView>("/orders", {
      method: "POST",
      body: JSON.stringify(payload),
      queueIfOffline: true,
    }),

  addOrderItem: (
    orderId: string,
    payload: {
      catalog_item_id: string;
      quantity: number;
      modifier_ids: string[];
      modifier_qualifier?: ModifierQualifier;
      seat_number: number | null;
      notes: string | null;
      added_by: string;
      source: OrderSource;
    }
  ) =>
    requestJson<OrderView>(`/orders/${orderId}/items`, {
      method: "POST",
      body: JSON.stringify(payload),
      queueIfOffline: true,
    }),

  removeOrderItem: (
    orderId: string,
    lineId: string,
    managerOverrideToken?: string
  ) =>
    requestJson<OrderView>(`/orders/${orderId}/items/${lineId}`, {
      method: "DELETE",
      managerOverrideToken,
    }),

  compOrderItem: (
    orderId: string,
    lineId: string,
    reason: string,
    managerOverrideToken?: string
  ) =>
    requestJson<OrderView>(`/orders/${orderId}/items/${lineId}/comp`, {
      method: "POST",
      body: JSON.stringify({ reason }),
      managerOverrideToken,
    }),

  advanceItemPrep: (orderId: string, lineId: string) =>
    requestJson<OrderView>(`/orders/${orderId}/items/${lineId}/advance-prep`, {
      method: "POST",
      queueIfOffline: true,
    }),

  bindPreauthCard: (orderId: string, card_label: string, hold_cents = 5000) =>
    requestJson<OrderView>(`/orders/${orderId}/preauth`, {
      method: "POST",
      body: JSON.stringify({ card_label, hold_cents }),
    }),

  updateOrderNotes: (orderId: string, client_profile_notes: string) =>
    requestJson<OrderView>(`/orders/${orderId}/notes`, {
      method: "PATCH",
      body: JSON.stringify({ client_profile_notes }),
    }),

  fireOrder: (orderId: string) =>
    requestJson<OrderView>(`/orders/${orderId}/fire`, {
      method: "POST",
      queueIfOffline: true,
    }),

  requestCheck: (orderId: string) =>
    requestJson<OrderView>(`/orders/${orderId}/request-check`, {
      method: "POST",
    }),

  splitOrder: (orderId: string, ways: number) =>
    requestJson<SplitCheckShare[]>(`/orders/${orderId}/split/${ways}`),

  payOrder: (
    orderId: string,
    payload: {
      amount_cents: number;
      tip_cents: number;
      method: PaymentMethod;
      payer_label: string;
    }
  ) =>
    requestJson<OrderView>(`/orders/${orderId}/pay`, {
      method: "POST",
      body: JSON.stringify(payload),
      queueIfOffline: true,
    }),

  refundOrder: (
    orderId: string,
    payload: {
      payment_id: string;
      amount_cents: number;
      reason: string;
    },
    managerOverrideToken?: string
  ) =>
    requestJson<OrderView>(`/orders/${orderId}/refund`, {
      method: "POST",
      body: JSON.stringify(payload),
      managerOverrideToken,
    }),

  printOrderReceipt: (orderId: string, kick_drawer = false) =>
    requestJson<RenderedReceipt>(`/orders/${orderId}/print-receipt`, {
      method: "POST",
      body: JSON.stringify({ kick_drawer }),
    }),

  toggleMod: (modId: string, enabled: boolean) =>
    requestJson<ModManifest>(`/mods/${modId}/toggle`, {
      method: "POST",
      body: JSON.stringify({ enabled }),
    }),

  getLedger: () => requestJson<LedgerSummaryResponse>("/ledger"),

  getShifts: () => requestJson<ShiftsSummaryResponse>("/shifts"),

  openShift: (opening_float_cents: number) =>
    requestJson<CashDrawerShift>("/shifts/open", {
      method: "POST",
      body: JSON.stringify({ opening_float_cents }),
    }),

  recordShiftMovement: (
    kind: CashMovementKind,
    amount_cents: number,
    reason: string
  ) =>
    requestJson<CashDrawerShift>("/shifts/movement", {
      method: "POST",
      body: JSON.stringify({ kind, amount_cents, reason }),
    }),

  closeShiftZ: (counted_cash_cents: number, managerOverrideToken?: string) =>
    requestJson<ShiftReport>("/shifts/close-z", {
      method: "POST",
      body: JSON.stringify({ counted_cash_cents }),
      managerOverrideToken,
    }),

  getHalStatus: () => requestJson<HalStatusSnapshot>("/hal/status"),

  kickCashDrawer: () =>
    requestJson<RenderedReceipt>("/hal/kick-drawer", {
      method: "POST",
    }),
};

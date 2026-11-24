const HUB_OVERRIDE_KEY = "pos_hub_origin_override";
const OFFLINE_QUEUE_KEY = "pos_offline_mutation_queue_v2";

export interface OfflineQueuedMutation {
  id: string;
  idempotencyKey: string;
  path: string;
  method: string;
  body: string | null;
  createdAt: string;
}

/**
 * Resolves the POS Edge Hub API base URL dynamically:
 * 1. Explicit Tauri / mobile handheld override stored in localStorage
 * 2. Build-time VITE_POS_HUB_URL env var
 * 3. Relative `/api` (works automatically over LAN IP, QR phone links, and Vite dev proxy)
 */
export function resolveApiBase(): string {
  try {
    const saved = localStorage.getItem(HUB_OVERRIDE_KEY)?.trim();
    if (saved) {
      return `${saved.replace(/\/+$/, "")}/api`;
    }
  } catch {
    // Ignore storage restrictions
  }
  const envUrl = (import.meta as unknown as { env?: Record<string, string> }).env
    ?.VITE_POS_HUB_URL;
  if (envUrl) {
    return `${envUrl.replace(/\/+$/, "")}/api`;
  }
  return "/api";
}

export function resolveWsUrl(): string {
  try {
    const saved = localStorage.getItem(HUB_OVERRIDE_KEY)?.trim();
    if (saved) {
      const wsOrigin = saved.replace(/^http/i, "ws").replace(/\/+$/, "");
      return `${wsOrigin}/ws`;
    }
  } catch {
    // Ignore storage restrictions
  }
  const envUrl = (import.meta as unknown as { env?: Record<string, string> }).env
    ?.VITE_POS_HUB_URL;
  if (envUrl) {
    return `${envUrl.replace(/^http/i, "ws").replace(/\/+$/, "")}/ws`;
  }
  const proto = window.location.protocol === "https:" ? "wss:" : "ws:";
  return `${proto}//${window.location.host}/ws`;
}

export function generateIdempotencyKey(): string {
  if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
    return crypto.randomUUID();
  }
  return `idem-${Date.now()}-${Math.random().toString(36).slice(2, 11)}`;
}

export function loadOfflineQueue(): OfflineQueuedMutation[] {
  try {
    const raw = localStorage.getItem(OFFLINE_QUEUE_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed) ? parsed : [];
  } catch {
    return [];
  }
}

export function saveOfflineQueue(items: OfflineQueuedMutation[]): void {
  try {
    localStorage.setItem(OFFLINE_QUEUE_KEY, JSON.stringify(items));
  } catch {
    // Ignore storage quota errors
  }
}

export function enqueueOfflineMutation(
  mutation: Omit<OfflineQueuedMutation, "id" | "createdAt">
): OfflineQueuedMutation {
  const queue = loadOfflineQueue();
  const entry: OfflineQueuedMutation = {
    ...mutation,
    id: generateIdempotencyKey(),
    createdAt: new Date().toISOString(),
  };
  queue.push(entry);
  saveOfflineQueue(queue);
  return entry;
}

export async function flushOfflineQueue(
  token: string | null
): Promise<{ flushed: number; remaining: number }> {
  const queue = loadOfflineQueue();
  if (queue.length === 0 || !token) {
    return { flushed: 0, remaining: queue.length };
  }

  const apiBase = resolveApiBase();
  const remaining: OfflineQueuedMutation[] = [];
  let flushed = 0;

  for (const item of queue) {
    try {
      const res = await fetch(`${apiBase}${item.path}`, {
        method: item.method,
        headers: {
          "Content-Type": "application/json",
          Authorization: `Bearer ${token}`,
          "Idempotency-Key": item.idempotencyKey,
        },
        body: item.body ?? undefined,
      });
      if (res.ok || (res.status >= 400 && res.status < 500)) {
        flushed += 1;
      } else {
        remaining.push(item);
      }
    } catch {
      remaining.push(item);
    }
  }

  saveOfflineQueue(remaining);
  return { flushed, remaining: remaining.length };
}

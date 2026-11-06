import { create } from "zustand";

const KEY = "shardx.list-order.v1";
const EMPTY: readonly string[] = [];
function load(): Record<string, string[]> {
  try {
    const raw: unknown = JSON.parse(localStorage.getItem(KEY) ?? "{}");
    if (!raw || typeof raw !== "object" || Array.isArray(raw)) return {};
    return Object.fromEntries(Object.entries(raw).filter(([, v]) => Array.isArray(v) && v.every((id) => typeof id === "string")));
  } catch { return {}; }
}

/** This computer's view order; object records and team data are never rewritten. */
export const useListOrderStore = create<{
  scope: string;
  orders: Record<string, string[]>;
  setScope: (scope: string) => void;
  save: (kind: string, ids: string[]) => void;
}>((set, get) => ({
  scope: "personal", orders: load(),
  setScope: (scope) => set({ scope }),
  save: (kind, ids) => {
    const { scope, orders } = get();
    const next = { ...orders, [JSON.stringify([scope, kind])]: ids };
    try { localStorage.setItem(KEY, JSON.stringify(next)); } catch { /* private mode */ }
    set({ orders: next });
  },
}));

export function listOrder(kind: string): readonly string[] {
  const { scope, orders } = useListOrderStore.getState();
  return orders[JSON.stringify([scope, kind])] ?? EMPTY;
}

export function useListOrder(kind: string): readonly string[] {
  return useListOrderStore((s) => s.orders[JSON.stringify([s.scope, kind])] ?? EMPTY);
}

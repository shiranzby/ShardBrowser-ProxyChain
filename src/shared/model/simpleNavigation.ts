import { create } from "zustand";

const KEY = "shardx.simpleNavigation";
const ITEMS_KEY = "shardx.simpleNavigation.items";
export const COLLAPSIBLE_PAGES = ["proxies", "fingerprints", "extensions", "bookmarks", "automation", "proxyshard"] as const;
export type CollapsiblePage = typeof COLLAPSIBLE_PAGES[number];
const DEFAULT_ITEMS: CollapsiblePage[] = ["extensions", "bookmarks", "automation", "proxyshard"];
export function readCollapsedPages(): CollapsiblePage[] {
  try {
    const raw: unknown = JSON.parse(localStorage.getItem(ITEMS_KEY) ?? "null");
    return Array.isArray(raw) ? COLLAPSIBLE_PAGES.filter((id) => raw.includes(id)) : DEFAULT_ITEMS;
  } catch { return DEFAULT_ITEMS; }
}
function read(): boolean {
  try { return localStorage.getItem(KEY) === "true"; } catch { return false; }
}

/** Presentation only, per window. Settings commits it after Save succeeds;
 * no profile, proxy, permission, command or navigation state is rewritten. */
export const useSimpleNavigation = create<{ enabled: boolean; items: CollapsiblePage[]; setEnabled: (enabled: boolean) => void; setItems: (items: CollapsiblePage[]) => void }>((set) => ({
  enabled: read(),
  items: readCollapsedPages(),
  setItems: (value) => {
    const items = COLLAPSIBLE_PAGES.filter((id) => value.includes(id));
    try { localStorage.setItem(ITEMS_KEY, JSON.stringify(items)); } catch { /* current window */ }
    set({ items });
  },
  setEnabled: (enabled) => {
    try { localStorage.setItem(KEY, String(enabled)); } catch { /* applies for this window */ }
    set({ enabled });
  },
}));

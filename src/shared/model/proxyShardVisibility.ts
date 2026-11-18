import { create } from "zustand";

const KEY = "shyshard.hideProxyShard";

function read(): boolean {
  try {
    const value = localStorage.getItem(KEY);
    return value === null ? true : value === "true";
  } catch {
    return true;
  }
}

/** Presentation preference. Legacy ProxyShard data and commands remain intact. */
export const useProxyShardVisibility = create<{
  hidden: boolean;
  setHidden: (hidden: boolean) => void;
}>((set) => ({
  hidden: read(),
  setHidden: (hidden) => {
    try { localStorage.setItem(KEY, String(hidden)); } catch { /* current window */ }
    set({ hidden });
  },
}));

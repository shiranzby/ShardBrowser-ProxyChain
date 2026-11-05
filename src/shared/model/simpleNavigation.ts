import { create } from "zustand";

const KEY = "shardx.simpleNavigation";
function read(): boolean {
  try { return localStorage.getItem(KEY) === "true"; } catch { return false; }
}

/** Presentation only, per window. Settings commits it after Save succeeds;
 * no profile, proxy, permission, command or navigation state is rewritten. */
export const useSimpleNavigation = create<{ enabled: boolean; setEnabled: (enabled: boolean) => void }>((set) => ({
  enabled: read(),
  setEnabled: (enabled) => {
    try { localStorage.setItem(KEY, String(enabled)); } catch { /* applies for this window */ }
    set({ enabled });
  },
}));

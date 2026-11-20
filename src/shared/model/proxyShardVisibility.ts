import { create } from "zustand";

const KEY = "shyshard.hideProxyShard";

const TRIAL_KEY = "shyshard.hideProxyShardTrial";

function read(key: string): boolean {
  try {
    const value = localStorage.getItem(key);
    return value === null ? true : value === "true";
  } catch {
    return true;
  }
}

/** Presentation preference. Legacy ProxyShard data and commands remain intact. */
export const useProxyShardVisibility = create<{
  hidden: boolean;
  trialHidden: boolean;
  setTrialHidden: (hidden: boolean) => void;
  setHidden: (hidden: boolean) => void;
}>((set) => ({
  hidden: read(KEY),
  trialHidden: read(TRIAL_KEY),
  setTrialHidden: (trialHidden) => {
    try { localStorage.setItem(TRIAL_KEY, String(trialHidden)); } catch { /* current window */ }
    set({ trialHidden });
  },
  setHidden: (hidden) => {
    try { localStorage.setItem(KEY, String(hidden)); } catch { /* current window */ }
    set({ hidden });
  },
}));

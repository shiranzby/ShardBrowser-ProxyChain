import { create } from "zustand";
import { onSpaceChange, spaceKey } from "../../entities/space";

export type GroupId = "pinned" | "folders" | "views";
/** Folders shown before "Show N more" (6.2). */
export const FOLDER_CAP = 8;

type Saved = { v: 1; collapsed: Partial<Record<GroupId, boolean>>; foldersOpen: boolean; rail: boolean; foldedFolders: string[] };
const KEY = "shardx.sidebar";

function load(): Saved {
  try {
    const raw = JSON.parse(localStorage.getItem(spaceKey(KEY)) ?? "null") as Partial<Saved> | null;
    if (raw?.v === 1) return { v: 1, collapsed: raw.collapsed ?? {}, foldersOpen: raw.foldersOpen === true, rail: raw.rail === true,
      foldedFolders: Array.isArray(raw.foldedFolders) ? raw.foldedFolders.filter((f): f is string => typeof f === "string") : [] };
  } catch { /* broken value: defaults */ }
  return { v: 1, collapsed: {}, foldersOpen: false, rail: false, foldedFolders: [] };
}

/// How this viewer likes the sidebar: which groups are folded, whether every
/// Folder shows, and the icon rail (Task 8). A per-viewer convenience, so it
/// lives in this window's storage, one per space. `overPeek` is not saved: the
/// user opened the rail that Peek folded below 1280 px, and it holds until Peek closes.
export const useSidebarPrefs = create<Saved & {
  overPeek: boolean;
  toggleGroup: (id: GroupId) => void;
  setFoldersOpen: (open: boolean) => void;
  toggleFolder: (name: string) => void;
  setRail: (rail: boolean) => void;
  setOverPeek: (overPeek: boolean) => void;
}>((set, get) => {
  const save = () => {
    const { v, collapsed, foldersOpen, rail, foldedFolders } = get();
    try { localStorage.setItem(spaceKey(KEY), JSON.stringify({ v, collapsed, foldersOpen, rail, foldedFolders })); } catch { /* private mode */ }
  };
  return {
    ...load(),
    overPeek: false,
    toggleGroup: (id) => { set({ collapsed: { ...get().collapsed, [id]: !get().collapsed[id] } }); save(); },
    setFoldersOpen: (foldersOpen) => { set({ foldersOpen }); save(); },
    toggleFolder: (name) => { set({ foldedFolders: get().foldedFolders.includes(name) ? get().foldedFolders.filter((f) => f !== name) : [...get().foldedFolders, name] }); save(); },
    setRail: (rail) => { set({ rail }); save(); },
    setOverPeek: (overPeek) => set({ overPeek }),
  };
});

// Another space has its own sidebar.
onSpaceChange(() => useSidebarPrefs.setState(load()));

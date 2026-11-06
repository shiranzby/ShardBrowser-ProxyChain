import { useMemo, useRef, type HTMLAttributes } from "react";
import type { MenuItem } from "@proxyshard/shardx-ui-kit";
import { useT } from "../i18n";
import { listOrder, useListOrderStore } from "../model/listOrder";
import { reorderIds } from "../lib/listOrder";

const MIME = "application/x-shardx-list-order";
const CONTROLS = "button, a, input, textarea, select, label, [role='button'], [contenteditable='true']";
type Dragged = { scope: string; kind: string; ids: string[] };
let dragging: Dragged | null = null;

export type ListReorder = {
  bind: (id: string) => HTMLAttributes<HTMLDivElement>;
  menu: (id: string) => MenuItem[];
};

/** Native row/card drag plus equivalent menu commands; list navigation keeps its keys. */
export function useListReorder({ kind, allIds, visibleIds, selected, groupOf, onReorder, dragHandle, resetSavedOrder = false }: {
  kind: string;
  allIds: readonly string[];
  visibleIds: readonly string[];
  selected?: ReadonlySet<string>;
  groupOf?: (id: string) => string;
  onReorder?: () => void;
  /** A row that is itself a button may opt in as a drag handle; nested controls remain blocked. */
  dragHandle?: string;
  /** A column sort supplies its complete sorted list as allIds before becoming manual. */
  resetSavedOrder?: boolean;
}): ListReorder {
  const t = useT();
  const controlPress = useRef(false);
  const scope = useListOrderStore((s) => s.scope);
  return useMemo(() => {
    const blocked = (target: HTMLElement) => {
      const control = target.closest(CONTROLS);
      return !!control && (!dragHandle || control !== target.closest(dragHandle));
    };
    const siblings = (id: string) => groupOf ? visibleIds.filter((other) => groupOf(other) === groupOf(id)) : visibleIds;
    const moved = (id: string) => selected?.has(id) ? siblings(id).filter((other) => selected.has(other)) : [id];
    const canDrop = (target: string) => dragging?.scope === scope && dragging.kind === kind && !dragging.ids.includes(target) && dragging.ids.every((id) => siblings(target).includes(id));
    const move = (ids: string[], target: string, after: boolean) => {
      if (useListOrderStore.getState().scope !== scope) return;
      useListOrderStore.getState().save(kind, reorderIds(allIds, resetSavedOrder ? [] : listOrder(kind), siblings(target), ids, target, after));
      onReorder?.();
    };
    return {
      bind: (id) => ({
        draggable: true,
        onPointerDownCapture: (e) => { controlPress.current = blocked(e.target as HTMLElement); },
        onDragStart: (e) => {
          if (controlPress.current || (e.target !== e.currentTarget && blocked(e.target as HTMLElement))) { e.preventDefault(); return; }
          dragging = { scope, kind, ids: moved(id) };
          e.dataTransfer.effectAllowed = "move";
          e.dataTransfer.setData(MIME, JSON.stringify(dragging));
          // Keep the existing Profiles payload for dropping into a Folder.
          if (!e.dataTransfer.types.includes("text/plain")) e.dataTransfer.setData("text/plain", dragging.ids.join("\n"));
        },
        onDragOver: (e) => {
          if (!canDrop(id)) return;
          e.preventDefault();
          e.dataTransfer.dropEffect = "move";
          const r = e.currentTarget.getBoundingClientRect();
          e.currentTarget.dataset.reorderDrop = e.clientY >= r.top + r.height / 2 ? "after" : "before";
        },
        onDragLeave: (e) => { if (!(e.relatedTarget instanceof Node) || !e.currentTarget.contains(e.relatedTarget)) delete e.currentTarget.dataset.reorderDrop; },
        onDrop: (e) => {
          delete e.currentTarget.dataset.reorderDrop;
          if (!canDrop(id)) return;
          e.preventDefault(); e.stopPropagation();
          const r = e.currentTarget.getBoundingClientRect();
          move(dragging!.ids, id, e.clientY >= r.top + r.height / 2);
          dragging = null;
        },
        onDragEnd: () => {
          dragging = null;
          document.querySelectorAll<HTMLElement>("[data-reorder-drop]").forEach((el) => { delete el.dataset.reorderDrop; });
        },
      }),
      menu: (id) => {
        const ids = siblings(id);
        const at = ids.indexOf(id);
        return [
          { label: t("listOrder.moveUp"), disabled: at <= 0, onSelect: () => { if (at > 0) move([id], ids[at - 1], false); } },
          { label: t("listOrder.moveDown"), disabled: at < 0 || at >= ids.length - 1, onSelect: () => { if (at >= 0 && at < ids.length - 1) move([id], ids[at + 1], true); } },
          { type: "separator" },
        ];
      },
    };
  }, [kind, allIds, visibleIds, selected, groupOf, onReorder, dragHandle, resetSavedOrder, t, scope]);
}

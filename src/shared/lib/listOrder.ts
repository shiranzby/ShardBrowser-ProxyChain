/** Saved ids first, then newly arrived ids in their source order. */
export function orderedIds(ids: readonly string[], saved: readonly string[]): string[] {
  const present = new Set(ids);
  return [...new Set([...saved.filter((id) => present.has(id)), ...ids])];
}

export function orderItems<T>(items: readonly T[], saved: readonly string[], idOf: (item: T) => string): T[] {
  const byId = new Map(items.map((item) => [idOf(item), item]));
  return orderedIds([...byId.keys()], saved).map((id) => byId.get(id)!);
}

/** Change only visible slots: a filtered or paged drag never forgets hidden ids.
 * Seed those slots with the current display order before leaving a column sort. */
export function reorderIds(all: readonly string[], saved: readonly string[], visible: readonly string[], moving: readonly string[], target: string, after: boolean): string[] {
  const base = orderedIds(all, saved);
  const present = new Set(base);
  const shown = [...new Set(visible)].filter((id) => present.has(id));
  const moved = new Set(moving.filter((id) => shown.includes(id)));
  if (!shown.includes(target) || moved.size === 0 || moved.has(target)) return base;
  const remaining = shown.filter((id) => !moved.has(id));
  remaining.splice(remaining.indexOf(target) + (after ? 1 : 0), 0, ...shown.filter((id) => moved.has(id)));
  const visibleSet = new Set(shown);
  let next = 0;
  return base.map((id) => visibleSet.has(id) ? remaining[next++] : id);
}

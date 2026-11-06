/** Folder names remain stable keys. Hierarchy is explicit, never parsed from names. */
export type FolderParents = Readonly<Record<string, string>>;
export const NO_FOLDER_PARENTS: FolderParents = Object.freeze({});
const own = (parents: FolderParents, name: string): string => Object.prototype.hasOwnProperty.call(parents, name) ? parents[name] : "";

/** Includes the folder itself. Malformed imported loops always terminate. */
export function insideFolder(name: string, ancestor: string, parents: FolderParents = NO_FOLDER_PARENTS): boolean {
  if (name === ancestor) return true;
  const seen = new Set<string>();
  while (name && !seen.has(name)) {
    if (name === ancestor) return true;
    seen.add(name);
    name = own(parents, name);
  }
  return false;
}

/** Parent first, siblings in the caller's chosen order. Orphans stay visible. */
export function folderTree(names: readonly string[], parents: FolderParents): { name: string; depth: number; parent: string; children: boolean }[] {
  const known = new Set(names);
  const children = new Map<string, string[]>();
  for (const name of names) {
    const p = own(parents, name);
    const parent = p !== name && known.has(p) ? p : "";
    children.set(parent, [...(children.get(parent) ?? []), name]);
  }
  const seen = new Set<string>();
  const result: ReturnType<typeof folderTree> = [];
  const add = (name: string, depth: number, parent: string) => {
    if (seen.has(name)) return;
    seen.add(name);
    result.push({ name, depth, parent, children: (children.get(name)?.length ?? 0) > 0 });
    for (const child of children.get(name) ?? []) add(child, depth + 1, name);
  };
  for (const name of children.get("") ?? []) add(name, 0, "");
  // Corrupt cycles must not hide folders or the profiles they contain.
  for (const name of names) if (!seen.has(name)) add(name, 0, "");
  return result;
}

export function folderPath(name: string, parents: FolderParents): string {
  const path: string[] = [];
  const seen = new Set<string>();
  while (name && !seen.has(name)) { path.unshift(name); seen.add(name); name = own(parents, name); }
  return path.join(" / ");
}

export function renameFolderParents(parents: FolderParents, from: string, to: string): Record<string, string> {
  return Object.fromEntries(Object.entries(parents).map(([name, parent]) => [name === from ? to : name, parent === from ? to : parent]));
}

/** Removing only the container promotes its children, preserving their contents. */
export function forgetFolderParent(parents: FolderParents, name: string): Record<string, string> {
  const parent = own(parents, name);
  return Object.fromEntries(Object.entries(parents)
    .filter(([child, p]) => child !== name && (p !== name || parent))
    .map(([child, p]) => [child, p === name ? parent : p]));
}

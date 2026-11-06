export function canonical(value) {
  try { return new URL(value).href; } catch { return null; }
}

export function isBootstrapTab(tab, bootstrap) {
  return Boolean(tab && !tab.incognito && tab.url === bootstrap &&
    (!tab.pendingUrl || tab.pendingUrl === bootstrap));
}

export function existingTab(tabs, target, ownId) {
  const wanted = canonical(target);
  if (!wanted) return undefined;
  return tabs.filter(tab => tab.id !== ownId && !tab.incognito &&
    canonical(tab.pendingUrl || tab.url) === wanted)
    .sort((a, b) => Number(b.active) - Number(a.active) || a.id - b.id)[0];
}

export async function reuseOrOpen(api, ownId, target, bootstrap) {
  const own = await api.get(ownId);
  // A user may have navigated this temporary tab while restore was settling.
  if (!isBootstrapTab(own, bootstrap)) return "user_navigated";
  if (!target) {
    const beforeClear = await api.get(ownId);
    if (!isBootstrapTab(beforeClear, bootstrap)) return "user_navigated";
    await api.update(ownId, { url: "about:blank" });
    return "cleared";
  }
  const tabs = await api.query({});
  const match = existingTab(tabs, target, ownId);
  if (match) {
    // Re-query before mutation: a target closed meanwhile must not lose the entry tab.
    const current = await api.get(match.id);
    if (!existingTab([current], target, ownId)) return "retry";
    const beforeActivate = await api.get(ownId);
    if (!isBootstrapTab(beforeActivate, bootstrap)) return "user_navigated";
    if (beforeActivate.active) await api.update(match.id, { active: true });
    const beforeRemove = await api.get(ownId);
    if (!isBootstrapTab(beforeRemove, bootstrap)) return "user_navigated";
    await api.remove(ownId); // Only the bootstrap that runs this script is removed.
    return "reused";
  }
  const beforeOpen = await api.get(ownId);
  if (!isBootstrapTab(beforeOpen, bootstrap)) return "user_navigated";
  await api.update(ownId, { url: target });
  return "opened";
}

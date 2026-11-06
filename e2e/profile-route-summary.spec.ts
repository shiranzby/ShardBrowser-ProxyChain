import { expect, test } from "@playwright/test";
import { appModule, openApp } from "./support/app";

test("explicit system and first-hop-only routes are displayed and filtered as proxied", async ({ page }, info) => {
  await openApp(page, { theme: "light" });
  await appModule(page, "/src/entities/profile/lib/useProfile.tsx", "profiles");
  await appModule(page, "/src/entities/profile/lib/listing.ts", "listing");
  await appModule(page, "/src/entities/signals/signals.ts", "signals");
  const result = await page.evaluate(() => {
    const w = window as any;
    const store = w.profiles.useProfile;
    const base = store.getState().profiles[0];
    const proxies = store.getState().proxies;
    const chosen = proxies[0];
    const rows = [
      { ...base, id: "route-system", name: "System route", proxy_id: chosen.id, proxy_route: { mode: "system" } },
      { ...base, id: "route-first-only", name: "First hop only", proxy_id: "stale-retained-binding", proxy_route: { mode: "chain", first: { mode: "proxy", proxy_id: chosen.id }, second: { mode: "direct" } } },
      { ...base, id: "route-real-ip", name: "Explicit real IP", proxy_id: chosen.id, proxy_route: { mode: "direct" } },
    ];
    store.setState({ profiles: rows });
    const filter = (proxy: string) => rows.filter(w.listing.makeProfileFilter(proxies, "", "all", { ...w.listing.emptyFilters(), proxy }, {}, () => [], Date.now())).map(p => p.id);
    const signals = w.signals.computeSignals({ profiles: rows, scopeIds: new Set(rows.map(p => p.id)), proxies, running: {}, snapshots: {}, locales: {}, startFailed: {}, failReasons: {}, plans: [], now: Date.now() });
    return { proxied: filter("bound"), direct: filter("direct"), missing: rows.map(p => w.listing.proxyMissing(p, proxies)), leak: signals.find((s: any) => s.kind === "locationLeak")?.ids, firstName: chosen.name || `${chosen.host}:${chosen.port}` };
  });
  expect(result.proxied).toEqual(["route-system", "route-first-only"]);
  expect(result.direct).toEqual(["route-real-ip"]);
  expect(result.missing).toEqual([false, false, false]);
  expect(result.leak).toEqual(["route-real-ip"]);
  await expect(page.locator("[data-profile-id='route-system'] [data-cell='proxy']")).toContainText("System proxy");
  await expect(page.locator("[data-profile-id='route-first-only'] [data-cell='proxy']")).toContainText(result.firstName);
  await expect(page.locator("[data-profile-id='route-real-ip'] [data-cell='proxy']")).toContainText("Real IP, direct connection");
  await page.screenshot({ path: info.outputPath("route-list-light.png"), fullPage: true });
});

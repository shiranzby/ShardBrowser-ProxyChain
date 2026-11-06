import { expect, test } from "@playwright/test";
import { openApp, reloadApp } from "./support/app";

test("navigation collection is configurable and commits only on save", async ({ page }) => {
  await openApp(page, { section: "settings" });
  await page.getByRole("tab", { name: "Appearance", exact: true }).click();
  await page.getByRole("switch", { name: "Simple navigation", exact: true }).check();
  const pages = page.locator("[data-setting='simple-navigation'] fieldset");
  await pages.getByRole("checkbox", { name: "Proxies", exact: true }).check();
  await pages.getByRole("checkbox", { name: "Bookmarks", exact: true }).uncheck();
  await expect(page.locator("[data-nav='proxies']")).toBeVisible();
  await page.locator("[data-save-bar]").getByRole("button", { name: "Don’t save", exact: true }).click();
  expect(await page.evaluate(() => localStorage.getItem("shardx.simpleNavigation.items"))).toBeNull();
  await page.getByRole("switch", { name: "Simple navigation", exact: true }).check();
  await pages.getByRole("checkbox", { name: "Proxies", exact: true }).check();
  await pages.getByRole("checkbox", { name: "Bookmarks", exact: true }).uncheck();
  await page.locator("[data-save-bar]").getByRole("button", { name: "Save", exact: true }).click();
  await expect(page.locator("[data-save-bar]")).toHaveCount(0);
  await expect(page.locator("[data-nav='proxies']")).toHaveCount(0);
  await expect(page.locator("[data-nav='bookmarks']")).toBeVisible();
  await reloadApp(page);
  await expect(page.locator("[data-nav='proxies']")).toHaveCount(0);
  await expect(page.locator("[data-nav='bookmarks']")).toBeVisible();
  await page.locator("[data-nav='more']").click();
  await page.locator("[data-nav='proxies']").click();
  await expect(page.locator("main [data-page-title]")).toContainText("Proxies");
});

test("telemetry stays removed with a previous opt-in and about links to the fork", async ({ page }) => {
  await page.addInitScript(() => {
    localStorage.setItem("shardx.analytics", "on");
    localStorage.setItem("shardx-analytics-client-id", "old-id");
  });
  await openApp(page, { section: "settings" });
  await expect(page.getByRole("tab", { name: "Other", exact: true })).toHaveCount(0);
  await expect(page.getByRole("switch", { name: "Share usage statistics", exact: true })).toHaveCount(0);
  expect(await page.evaluate(() => (window as any).__shardxMock.analyticsHits)).toBe(0);
  expect(await page.evaluate(() => localStorage.getItem("shardx-analytics-client-id"))).toBeNull();
  await page.getByRole("tab", { name: "About", exact: true }).click();
  await expect(page.getByRole("link", { name: "shiranzby/ShardBrowser-ProxyChain", exact: true })).toHaveAttribute("href", "https://github.com/shiranzby/ShardBrowser-ProxyChain");
});

test("header switch preserves other settings, reports its route and survives reload", async ({ page }) => {
  await openApp(page, { theme: "light" });
  const before = await page.evaluate(() => {
    const s = structuredClone((window as any).__shardxMock.state.settings);
    delete s.proxy_chain; return s;
  });
  const root = page.locator("[data-proxy-quick]");
  await root.getByRole("button", { name: "Proxy chain off", exact: true }).hover();
  const pop = root.getByRole("dialog");
  await expect(pop.getByText("Direct network exit", { exact: true })).toBeVisible();
  await expect(pop.locator("[data-quick-ip]")).toHaveText("192.0.2.18");
  const choice = pop.getByRole("switch");
  await choice.click();
  await expect(choice).toBeChecked();
  await expect(root.getByRole("button", { name: "Proxy chain on", exact: true })).toBeVisible();
  await expect(pop.locator("[data-quick-ip]")).toHaveText("198.51.100.24");
  await expect(pop.getByText("First-hop exit", { exact: true })).toBeVisible();
  expect(await page.evaluate(() => { const s = structuredClone((window as any).__shardxMock.state.settings); delete s.proxy_chain; return s; })).toEqual(before);
  await page.screenshot({ path: "../.audit/pc7-quick-control-light.png" });
  await page.keyboard.press("Escape");
  await expect(pop).toHaveCount(0);
  await reloadApp(page);
  await expect(root.getByRole("button", { name: "Proxy chain on", exact: true })).toBeVisible();
});

test("header failed save keeps the saved switch and permits retry", async ({ page }) => {
  await page.addInitScript(() => Object.assign(window, { __shardxMockFail: { proxy_chain_set_enabled: 1 } }));
  await openApp(page);
  const root = page.locator("[data-proxy-quick]");
  await root.getByRole("button").hover();
  const pop = root.getByRole("dialog");
  await expect(pop.locator("[data-quick-ip]")).toBeVisible();
  await pop.getByRole("switch").click();
  await expect(pop.getByRole("alert")).toBeVisible();
  await expect(pop.getByRole("switch")).not.toBeChecked();
  expect(await page.evaluate(() => (window as any).__shardxMock.state.settings.proxy_chain?.enabled ?? false)).toBe(false);
  await pop.getByRole("switch").click();
  await expect(pop.getByRole("switch")).toBeChecked();
});

test("header popover remains in the minimum window", async ({ page }) => {
  await page.setViewportSize({ width: 1024, height: 680 });
  await openApp(page);
  const root = page.locator("[data-proxy-quick]");
  await root.getByRole("button").hover();
  const pop = root.getByRole("dialog");
  await expect(pop.locator("[data-quick-ip]")).toBeVisible();
  const box = await pop.boundingBox();
  expect(box).not.toBeNull();
  expect(box!.x).toBeGreaterThanOrEqual(0);
  expect(box!.x + box!.width).toBeLessThanOrEqual(1024);
  expect(box!.y + box!.height).toBeLessThanOrEqual(680);
  expect(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth)).toBe(false);
  await page.screenshot({ path: "../.audit/pc7-quick-control-1024.png" });
});

test("a late direct probe cannot replace the newly enabled first-hop result", async ({ page }) => {
  await page.addInitScript(() => Object.assign(window, { __shardxMockSlow: { proxy_chain_test: 1200 } }));
  await openApp(page);
  const root = page.locator("[data-proxy-quick]");
  await root.getByRole("button").hover();
  const pop = root.getByRole("dialog");
  await expect(pop.getByText("Checking the current route…", { exact: true })).toBeVisible();
  await pop.getByRole("switch").click();
  await expect(pop.getByRole("switch")).toBeChecked();
  await expect(pop.getByRole("switch")).toBeEnabled();
  await expect(pop.locator("[data-quick-ip]")).toHaveText("198.51.100.24");
  await expect(pop.getByText("First-hop exit", { exact: true })).toBeVisible();
});

test("leaving during a saved toggle does not start a probe from the unmounted toolbar", async ({ page }) => {
  await page.addInitScript(() => Object.assign(window, { __shardxMockSlow: { proxy_chain_set_enabled: 1200 } }));
  await openApp(page);
  const root = page.locator("[data-proxy-quick]");
  await root.getByRole("button").hover();
  const pop = root.getByRole("dialog");
  await expect(pop.locator("[data-quick-ip]")).toBeVisible();
  const probes = await page.evaluate(() => (window as any).__shardxMock.calls.proxy_chain_test);
  await pop.getByRole("switch").click();
  await expect(pop.getByRole("switch")).toBeDisabled();
  await pop.getByRole("button", { name: "Proxy settings", exact: true }).click();
  await expect(page.locator("main")).toHaveAttribute("data-section", "settings");
  await expect.poll(() => page.evaluate(() => (window as any).__shardxMock.pending)).toBe(0);
  expect(await page.evaluate(() => (window as any).__shardxMock.calls.proxy_chain_test)).toBe(probes);
  expect(await page.evaluate(() => (window as any).__shardxMock.state.settings.proxy_chain.enabled)).toBe(true);
  await expect(page.locator("[data-setting='proxy-chain-enabled']").getByRole("switch")).toBeChecked();
});

test("quick route failures use the Chinese layer error", async ({ page }) => {
  await page.addInitScript(() => {
    localStorage.setItem("shardx.lang", "zh");
    Object.assign(window, { __shardxMockChainResult: {
      exit_ip: null, exit_geo: null, exit_error_code: "proxy_chain_local_geo_failed", exit_error: "Raw local lookup failed",
      local_network: { state: "failed", error_code: "proxy_chain_local_geo_failed", error_message: "Raw local lookup failed" },
    } });
  });
  await openApp(page);
  const root = page.locator("[data-proxy-quick]");
  await root.getByRole("button").hover();
  await expect(root.getByRole("alert")).toBeVisible();
  await expect(root.getByRole("alert")).not.toContainText("Raw local");
  await expect(root.getByRole("alert")).toContainText("出口");
});

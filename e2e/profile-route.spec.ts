import { expect, test, type Page } from "@playwright/test";
import { appModule, openApp, openEditForm, reloadApp } from "./support/app";

const profileId = "c31d92f1";
const saved = (page: Page) => page.evaluate((id) => structuredClone((window as any).__shardxMock.state.profiles[id]), profileId);
const calls = (page: Page, cmd: string) => page.evaluate((c) => (window as any).__shardxMock.calls[c] ?? 0, cmd);
async function custom(page: Page) {
  const form = await openEditForm(page, "FB Ads acc-02", "Proxy");
  await form.getByRole("button", { name: "Customize connection", exact: true }).click();
  return form;
}
async function pick(page: Page, label: string, option: string) {
  await page.getByRole("dialog").getByRole("combobox", { name: label, exact: true }).click();
  await page.getByRole("option", { name: option, exact: true }).click();
}

test("single connection choices retain the assigned proxy and test a draft without saving", async ({ page }) => {
  await openApp(page, { theme: "light" });
  const before = await saved(page);
  const form = await custom(page);
  expect(before._meta.proxy_route).toBeUndefined();
  await expect(form.getByRole("combobox", { name: "Connection", exact: true })).toContainText("Choose proxy");
  await form.getByRole("button", { name: "Test connection", exact: true }).click();
  await expect(form.locator("[data-profile-route-exit]")).toContainText("203.0.113.42");
  await expect(form.locator("[data-profile-route-location]")).toContainText("Singapore");
  expect(await calls(page, "profile_save")).toBe(0);
  expect(await calls(page, "proxy_save")).toBe(0);
  expect(await saved(page)).toEqual(before);
  for (const option of ["Use system proxy", "Real IP, direct connection"]) {
    await pick(page, "Connection", option);
    await expect(form.locator("[data-profile-route-result]")).toHaveCount(0);
    await form.getByRole("button", { name: "Test connection", exact: true }).click();
    await expect(form.locator("[data-profile-route-exit]")).toContainText(option === "Use system proxy" ? "198.51.100.24" : "192.0.2.18");
  }
  await form.getByRole("button", { name: "Save", exact: true }).click();
  await expect(form).toBeHidden();
  expect((await saved(page))._meta).toMatchObject({ proxy_id: before._meta.proxy_id, proxy_route: { mode: "direct" } });
  await reloadApp(page);
  const reopened = await openEditForm(page, "FB Ads acc-02", "Proxy");
  await expect(reopened.getByRole("combobox", { name: "Connection", exact: true })).toContainText("Real IP, direct connection");
});

for (const mode of ["proxy", "system", "direct"] as const) test(`single ${mode} failures describe only the selected connection`, async ({ page }) => {
  await openApp(page, { theme: "light" });
  const form = await custom(page);
  if (mode !== "proxy") await pick(page, "Connection", mode === "system" ? "Use system proxy" : "Real IP, direct connection");
  await appModule(page, "/src/shared/i18n/index.ts", "__singleRouteI18n");
  await page.evaluate((mode) => {
    (window as any).__singleRouteI18n.useLang.getState().setLang("zh");
    (window as any).__shardxMockRouteResult = {
      local_network: mode === "direct" ? { state: "failed", error_code: "proxy_chain_local_network_failed" } : null,
      system_proxy: mode === "system" ? { state: "failed", error_code: "system_proxy_connect_refused", error_details: { protocol: "http", phase: "connect", response_code: 403, target: "example.com:443" } } : { state: "not_tested" },
      profile_proxy: mode === "proxy" ? { state: "failed", error_code: "proxy_chain_profile_connect_failed", error_details: { protocol: "socks5", phase: "connect", response_code: 4, target: "example.com:443" } } : { state: "not_tested" },
      proxy_address: mode === "system" ? "127.0.0.1:7890" : null,
      // An inactive assignment must not appear as a second hop in system/direct mode.
      profile_proxy_address: "203.0.113.7:1080", exit_ip: null, exit_geo: null,
    };
  }, mode);
  const localizedForm = page.getByRole("dialog");
  await localizedForm.getByRole("button", { name: "测试连接", exact: true }).click();
  const result = localizedForm.locator("[data-profile-route-result]");
  await expect(result).toContainText(mode === "proxy" ? "所选代理无法连接检测目标" : mode === "system" ? "系统代理无法连接检测目标" : "本机网络的出口 IP 和地区查询失败");
  await expect(result).not.toContainText(/第一层|第二层/);
  if (mode !== "direct") {
    await expect(result.locator("[data-profile-route-protocol]")).toContainText(mode === "proxy" ? "SOCKS5 CONNECT 返回：4" : "HTTP CONNECT 返回：403");
    await expect(result.locator("[data-profile-route-protocol]")).toContainText("检测目标：example.com:443");
  }
  if (mode === "proxy") await expect(result).toContainText("代理地址");
  else await expect(result).not.toContainText("203.0.113.7:1080");
  if (mode === "proxy") {
    await page.setViewportSize({ width: 1024, height: 680 });
    expect(await localizedForm.evaluate(el => el.scrollWidth - el.clientWidth)).toBeLessThanOrEqual(1);
    await page.screenshot({ path: test.info().outputPath("single-proxy-failed-zh.png"), fullPage: true });
  }
  expect(await calls(page, "profile_save")).toBe(0);
  expect(await calls(page, "proxy_save")).toBe(0);
});

test("single proxy authentication reply does not claim a target connection", async ({ page }) => {
  await openApp(page);
  const form = await custom(page);
  await page.evaluate(() => { (window as any).__shardxMockRouteResult = {
    profile_proxy: { state: "failed", error_code: "proxy_chain_profile_auth_failed", error_details: { protocol: "socks5", phase: "authentication", response_code: 1 } },
    exit_ip: null, exit_geo: null,
  }; });
  await form.getByRole("button", { name: "Test connection", exact: true }).click();
  const result = form.locator("[data-profile-route-result]");
  await expect(result.locator("[data-profile-route-protocol]")).toContainText("SOCKS5 authentication response: 1");
  await expect(result).not.toContainText("Test target:");
  await expect(result).not.toContainText(/First hop|Second hop/);
});

test("connected proxy with failed exit lookup shows safe provider failures without a stale IP", async ({ page }) => {
  await openApp(page, { theme: "light" });
  const form = await custom(page);
  await appModule(page, "/src/shared/i18n/index.ts", "__geoRouteI18n");
  await page.evaluate(() => {
    (window as any).__geoRouteI18n.useLang.getState().setLang("zh");
    (window as any).__shardxMockRouteResult = {
      profile_proxy: { state: "ok", latency_ms: 42 },
      exit_ip: "203.0.113.201", exit_geo: { ip: "203.0.113.201", country: "Stale country", country_code: "US", city: "Stale city" },
      exit_error: "Safe lookup failure", exit_error_code: "proxy_chain_profile_geo_failed",
      exit_error_details: [
        { provider: "ip-api.com", phase: "http_status", http_status: 429 },
        { provider: "ipapi.co", phase: "timeout" },
        { provider: "ipwho.is", phase: "invalid_ip" },
      ],
    };
  });
  const localizedForm = page.getByRole("dialog");
  await localizedForm.getByRole("button", { name: "测试连接", exact: true }).click();
  const result = localizedForm.locator("[data-profile-route-result]");
  await expect(result).toContainText("已连接，42 毫秒");
  await expect(result).toContainText("所选代理已连接，但出口 IP 和地区查询失败");
  await expect(result.locator("[data-profile-route-geo-details]")).toContainText("ip-api.com：HTTP 返回 429");
  await expect(result.locator("[data-profile-route-geo-details]")).toContainText("ipapi.co：请求超时");
  await expect(result.locator("[data-profile-route-geo-details]")).toContainText("ipwho.is：未返回有效出口 IP");
  await expect(result.locator("[data-profile-route-exit]")).toContainText("查询失败");
  await expect(result).not.toContainText(/203\.0\.113\.201|Stale country|Stale city|第二层|第一层/);
  await page.setViewportSize({ width: 1024, height: 680 });
  expect(await localizedForm.evaluate(el => el.scrollWidth - el.clientWidth)).toBeLessThanOrEqual(1);
  await page.screenshot({ path: test.info().outputPath("single-proxy-geo-failed-zh.png"), fullPage: true });
  const lastFailure = result.locator("[data-profile-route-geo-details] li").last();
  await lastFailure.scrollIntoViewIfNeeded();
  await expect(lastFailure).toBeVisible();
  const lastBox = await lastFailure.boundingBox();
  const saveBox = await localizedForm.getByRole("button", { name: "保存", exact: true }).boundingBox();
  expect(lastBox!.y + lastBox!.height).toBeLessThanOrEqual(saveBox!.y);
  await page.screenshot({ path: test.info().outputPath("single-proxy-geo-failed-zh-bottom.png"), fullPage: true });
  expect(await calls(page, "profile_save")).toBe(0);
});

test("exit lookup diagnostics never render raw provider errors or unknown phase values", async ({ page }) => {
  await openApp(page);
  const form = await custom(page);
  await page.evaluate(() => { (window as any).__shardxMockRouteResult = {
    profile_proxy: { state: "ok", latency_ms: 42 }, exit_ip: null, exit_geo: null,
    exit_error: "Safe lookup failure", exit_error_code: "proxy_chain_profile_geo_failed",
    exit_error_details: [
      { provider: "ip-api.com", phase: "tls", message: "raw-secret-A" },
      { provider: "ipapi.co", phase: "json", message: "raw-secret-B" },
      { provider: "ipwho.is", phase: "provider_refused", message: "raw-secret-C" },
      { provider: "raw-secret-provider", phase: "raw-secret-phase", http_status: "raw-secret-status" },
      { provider: "ip-api.com", phase: "connect" },
      { provider: "ip-api.com", phase: "response_body" },
      { provider: "ipapi.co", phase: "redirect" },
      { provider: "ipwho.is", phase: "configuration" },
      { provider: "ipwho.is", phase: "request" },
    ],
  }; });
  await form.getByRole("button", { name: "Test connection", exact: true }).click();
  const details = form.locator("[data-profile-route-geo-details]");
  await expect(details).toContainText("TLS or certificate verification failed");
  await expect(details).toContainText("Response could not be parsed");
  await expect(details).toContainText("Provider refused the lookup");
  await expect(details).toContainText("Unknown source: Lookup failed");
  await expect(details).toContainText("Connection failed");
  await expect(details).toContainText("Response could not be read");
  await expect(details).toContainText("Redirect failed");
  await expect(details).toContainText("Lookup configuration failed");
  await expect(details).toContainText("Request failed");
  await expect(details).not.toContainText("raw-secret");
});

test("single system proxy exit lookup failure never describes a full chain", async ({ page }) => {
  await openApp(page);
  await custom(page);
  await pick(page, "Connection", "Use system proxy");
  await appModule(page, "/src/shared/i18n/index.ts", "__systemGeoI18n");
  await page.evaluate(() => {
    (window as any).__systemGeoI18n.useLang.getState().setLang("zh");
    (window as any).__shardxMockRouteResult = {
      system_proxy: { state: "ok", latency_ms: 24 }, profile_proxy: { state: "not_tested" },
      exit_ip: null, exit_geo: null, exit_error: "Safe lookup failure", exit_error_code: "proxy_chain_profile_geo_failed",
      exit_error_details: [{ provider: "ipapi.co", phase: "http_status", http_status: 429 }],
    };
  });
  const form = page.getByRole("dialog");
  await form.getByRole("button", { name: "测试连接", exact: true }).click();
  const result = form.locator("[data-profile-route-result]");
  await expect(result).toContainText("系统代理已连接，但出口 IP 和地区查询失败");
  await expect(result).toContainText("已连接，24 毫秒");
  await expect(result).not.toContainText(/完整代理链|第一层|第二层/);
  await expect(result.locator("[data-profile-route-geo-details]")).toContainText("HTTP 返回 429");
});

test("chain first-hop choices and direct second hop describe and save the exact route", async ({ page }) => {
  await openApp(page, { theme: "light" });
  const before = await saved(page);
  const form = await custom(page);
  await form.getByRole("switch", { name: "Enable proxy chain", exact: true }).click();
  await expect(form.getByRole("combobox", { name: "First-hop proxy", exact: true })).toContainText("Use system proxy");
  await pick(page, "First-hop proxy", "Choose proxy");
  await expect(form.getByRole("button", { name: "Save", exact: true })).toBeDisabled();
  await form.getByRole("combobox", { name: "First-hop proxy choice", exact: true }).click();
  await page.getByRole("option", { name: /^DC Tokyo\b/ }).click();
  await expect(form.locator("[data-profile-route-same]")).toBeVisible();
  await form.getByRole("switch", { name: "Use the assigned proxy if the first hop fails", exact: true }).click();
  await form.getByRole("button", { name: "Test connection", exact: true }).click();
  await expect(form.locator("[data-profile-route-result] [data-chain-route='same_proxy_endpoint']")).toBeVisible();
  await pick(page, "Second-hop proxy", "Direct connection");
  await expect(form.getByText("Adds no second proxy. Websites use the first hop; only a first hop using real IP also makes this a direct connection.")).toBeVisible();
  await expect(form.locator("[data-property='proxy']")).toHaveCount(0);
  await form.getByRole("button", { name: "Save", exact: true }).click();
  await expect(form).toBeHidden();
  expect((await saved(page))._meta).toMatchObject({ proxy_id: before._meta.proxy_id,
    proxy_route: { mode: "chain", first: { mode: "proxy", proxy_id: "px-jp-1" }, second: { mode: "direct" } } });
});

test("failed save keeps the route draft, retry saves it, and cancellation keeps the stored route", async ({ page }) => {
  await openApp(page);
  const before = await saved(page);
  let form = await custom(page);
  await pick(page, "Connection", "Use system proxy");
  await page.evaluate(() => { (window as any).__shardxMockFail = { profile_save: 1 }; });
  await form.getByRole("button", { name: "Save", exact: true }).click();
  await expect(form.getByRole("alert")).toBeVisible();
  await expect(form.getByRole("combobox", { name: "Connection", exact: true })).toContainText("Use system proxy");
  expect(await saved(page)).toEqual(before);
  await form.getByRole("button", { name: "Save", exact: true }).click();
  await expect(form).toBeHidden();
  form = await openEditForm(page, "FB Ads acc-02", "Proxy");
  await pick(page, "Connection", "Real IP, direct connection");
  await form.getByRole("button", { name: "Cancel", exact: true }).click();
  await page.getByRole("alertdialog", { name: "Discard your changes?" }).getByRole("button", { name: "Discard", exact: true }).click();
  await expect(form).toBeHidden();
  expect((await saved(page))._meta.proxy_route).toEqual({ mode: "system" });
});

test("a late connection result cannot replace a changed draft and test failure can be retried", async ({ page }) => {
  await openApp(page);
  const form = await custom(page);
  await page.evaluate(() => { (window as any).__shardxMockSlow = { proxy_route_test: 1500 }; });
  await form.getByRole("button", { name: "Test connection", exact: true }).click();
  await expect.poll(() => calls(page, "proxy_route_test")).toBe(1);
  await pick(page, "Connection", "Real IP, direct connection");
  await page.waitForTimeout(1700);
  await expect(form.locator("[data-profile-route-result]")).toHaveCount(0);
  await page.evaluate(() => { (window as any).__shardxMockSlow = {}; (window as any).__shardxMockFail = { proxy_route_test: 1 }; });
  await form.getByRole("button", { name: "Test connection", exact: true }).click();
  await expect(form.getByRole("alert")).toBeVisible();
  await form.getByRole("button", { name: "Test connection", exact: true }).click();
  await expect(form.locator("[data-profile-route-exit]")).toContainText("192.0.2.18");
  expect(await calls(page, "profile_save")).toBe(0);
});

test("new profiles can test their direct draft before any profile exists", async ({ page }) => {
  await openApp(page);
  await page.getByRole("button", { name: "New profile", exact: true }).first().click();
  const form = page.getByRole("dialog", { name: "New profile", exact: true });
  await form.getByRole("tab", { name: "Proxy", exact: true }).click();
  await expect(form.getByRole("combobox", { name: "Connection", exact: true })).toContainText("Real IP, direct connection");
  await form.getByRole("button", { name: "Test connection", exact: true }).click();
  await expect(form.locator("[data-profile-route-exit]")).toContainText("192.0.2.18");
  expect(await calls(page, "profile_save")).toBe(0);
  await appModule(page, "/src/features/profile-form/model/useProfileForm.ts", "__routeForm");
  expect(await page.evaluate(() => (window as any).__routeForm.useProfileForm.getState().draft.form.id)).toBe("");
});

for (const lang of ["zh", "ru"]) test(`explicit route choices fit the minimum window in ${lang}`, async ({ page }) => {
  await openApp(page, { theme: "light" });
  await appModule(page, "/src/shared/i18n/index.ts", "__routeI18n");
  await appModule(page, "/src/features/profile-form/model/useProfileForm.ts", "__routeForm");
  await page.evaluate((lang) => (window as any).__routeI18n.useLang.getState().setLang(lang), lang);
  await expect(page.locator("html")).toHaveAttribute("lang", lang === "zh" ? "zh-CN" : "ru");
  await page.evaluate(() => (window as any).__routeForm.openProfileForm({ mode: "edit", id: "c31d92f1", tab: "proxy" }));
  const form = page.getByRole("dialog");
  const customize = lang === "zh" ? "自定义连接方式" : "Настроить соединение";
  await form.getByRole("button", { name: customize, exact: true }).click();
  await page.setViewportSize({ width: 1024, height: 680 });
  await form.getByRole("switch").first().click();
  await expect(form.locator("[data-profile-first-hop]")).toBeVisible();
  const overflow = await form.evaluate((el) => ({ horizontal: el.scrollWidth - el.clientWidth, right: el.getBoundingClientRect().right, width: innerWidth }));
  expect(overflow.horizontal).toBeLessThanOrEqual(1);
  expect(overflow.right).toBeLessThanOrEqual(overflow.width);
  await page.screenshot({ path: test.info().outputPath(`profile-route-${lang}.png`), fullPage: true });
});

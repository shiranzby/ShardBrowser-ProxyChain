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

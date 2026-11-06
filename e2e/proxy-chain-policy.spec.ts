import { expect, test, type Page } from "@playwright/test";
import { openApp, openEditForm, reloadApp } from "./support/app";

const preferredLabel = "Prefer first hop, otherwise assigned proxy only";
const choice = (page: Page) => page.getByRole("dialog").getByRole("combobox", { name: "Proxy chain choice" });
const savedProxy = (page: Page) => page.evaluate(() => (window as any).__shardxMock.state.proxies.find((p: any) => p.id === "px-jp-1"));
const saveCalls = (page: Page, command: string) => page.evaluate((cmd) => (window as any).__shardxMock.calls[cmd] ?? 0, command);
async function editTokyo(page: Page) {
  await page.locator("[role='row'][data-proxy-id='px-jp-1'] [data-cell='name']").click();
  await page.locator("aside[data-peek='proxy'] [data-peek-header]").getByRole("button", { name: "Edit", exact: true }).click();
  return page.getByRole("dialog", { name: "Edit DC Tokyo", exact: true });
}

test("profile chain choice is saved, reopened and does not unbind its proxy", async ({ page }) => {
  await openApp(page, { theme: "light" });
  const before = await page.evaluate(() => (window as any).__shardxMock.state.profiles["c31d92f1"]._meta.proxy_id);
  expect(before).toBe("px-jp-1");
  let form = await openEditForm(page, "FB Ads acc-02", "Proxy");
  await form.getByRole("combobox", { name: "Proxy chain choice" }).click();
  await page.getByRole("option", { name: "Use assigned proxy only (skip first hop)", exact: true }).click();
  await form.getByRole("button", { name: "Save", exact: true }).click();
  await expect(form).toBeHidden();
  expect(await page.evaluate(() => (window as any).__shardxMock.state.profiles["c31d92f1"]._meta.proxy_chain_policy)).toBe("disabled");
  expect(await page.evaluate(() => (window as any).__shardxMock.state.profiles["c31d92f1"]._meta.proxy_id)).toBe(before);
  await reloadApp(page);
  form = await openEditForm(page, "FB Ads acc-02", "Proxy");
  await expect(form.getByRole("combobox", { name: "Proxy chain choice" })).toContainText("Use assigned proxy only (skip first hop)");
});

test("a profile can prefer the first hop, retry a failed save, and reload without changing its assigned proxy", async ({ page }) => {
  await openApp(page, { theme: "light" });
  const proxyBefore = await savedProxy(page);
  const saved = () => page.evaluate(() => (window as any).__shardxMock.state.profiles["c31d92f1"]);
  const before = await saved();
  expect(before._meta.proxy_id).toBe("px-jp-1");
  let form = await openEditForm(page, "FB Ads acc-02", "Proxy");
  await choice(page).click();
  await page.getByRole("option", { name: preferredLabel, exact: true }).click();
  await expect(form.locator("[data-chain-prefer-help]")).toContainText("a shared exit IP is not proof");
  await page.evaluate(() => { (window as any).__shardxMockFail = { profile_save: 1 }; });
  const calls = await saveCalls(page, "profile_save");
  await form.getByRole("button", { name: "Save", exact: true }).click();
  await expect.poll(() => saveCalls(page, "profile_save")).toBe(calls + 1);
  await expect(form).toBeVisible();
  await expect(form.locator("[data-form-error]").or(form.getByRole("alert")).first()).toBeVisible();
  await expect(choice(page)).toContainText(preferredLabel);
  expect(await saved()).toEqual(before);
  expect(await savedProxy(page)).toEqual(proxyBefore);
  await form.getByRole("button", { name: "Save", exact: true }).click();
  await expect(form).toBeHidden();
  expect((await saved())._meta).toMatchObject({ proxy_id: "px-jp-1", proxy_chain_policy: "prefer_first_hop" });
  expect(await savedProxy(page)).toEqual(proxyBefore);
  await reloadApp(page);
  form = await openEditForm(page, "FB Ads acc-02", "Proxy");
  await expect(choice(page)).toContainText(preferredLabel);
  await choice(page).click();
  await page.getByRole("option", { name: "Use assigned proxy only (skip first hop)", exact: true }).click();
  const beforeCancel = await saveCalls(page, "profile_save");
  await form.getByRole("button", { name: "Cancel", exact: true }).click();
  await page.getByRole("alertdialog", { name: "Discard your changes?" }).getByRole("button", { name: "Discard", exact: true }).click();
  await expect(form).toBeHidden();
  expect(await saveCalls(page, "profile_save")).toBe(beforeCancel);
  expect((await saved())._meta).toMatchObject({ proxy_id: "px-jp-1", proxy_chain_policy: "prefer_first_hop" });
});

test("a proxy preference survives save failure and reload while cancellation preserves credentials and bindings", async ({ page }) => {
  await openApp(page, { section: "proxies", theme: "light" });
  const before = await savedProxy(page);
  const bindings = () => page.evaluate(() => Object.entries((window as any).__shardxMock.state.profiles).map(([id, p]: [string, any]) => [id, p._meta.proxy_id]));
  const boundBefore = await bindings();
  let form = await editTokyo(page);
  await choice(page).click();
  await page.getByRole("option", { name: preferredLabel, exact: true }).click();
  await page.evaluate(() => { (window as any).__shardxMockFail = { proxy_save: 1 }; });
  const calls = await saveCalls(page, "proxy_save");
  await form.getByRole("button", { name: "Save", exact: true }).click();
  await expect.poll(() => saveCalls(page, "proxy_save")).toBe(calls + 1);
  await expect(form).toBeVisible();
  await expect(choice(page)).toContainText(preferredLabel);
  expect(await savedProxy(page)).toEqual(before);
  expect(await bindings()).toEqual(boundBefore);
  await form.getByRole("button", { name: "Save", exact: true }).click();
  await expect(form).toBeHidden();
  await expect(page.locator("aside[data-peek='proxy'] [data-property='chain-policy']")).toContainText(preferredLabel);
  expect(await savedProxy(page)).toEqual({ ...before, proxy_chain_policy: "prefer_first_hop" });
  await reloadApp(page);
  form = await editTokyo(page);
  await expect(choice(page)).toContainText(preferredLabel);
  await choice(page).click();
  await page.getByRole("option", { name: "Use first hop for this proxy", exact: true }).click();
  const beforeCancel = await saveCalls(page, "proxy_save");
  await form.getByRole("button", { name: "Cancel", exact: true }).click();
  const discard = page.getByRole("button", { name: /Discard|Don’t save|Don't save/i });
  if (await discard.count()) await discard.last().click();
  await expect(form).toBeHidden();
  expect(await saveCalls(page, "proxy_save")).toBe(beforeCancel);
  expect(await savedProxy(page)).toEqual({ ...before, proxy_chain_policy: "prefer_first_hop" });
  expect(await bindings()).toEqual(boundBefore);
});

test("proxy choice is editable without touching credentials and cancel writes nothing", async ({ page }, info) => {
  await openApp(page, { section: "proxies", theme: "light" });
  const before = await page.evaluate(() => (window as any).__shardxMock.state.proxies.find((p: any) => p.id === "px-jp-1"));
  const edit = async () => {
    await page.locator("[role='row'][data-proxy-id='px-jp-1'] [data-cell='name']").click();
    await page.locator("aside[data-peek='proxy'] [data-peek-header]").getByRole("button", { name: "Edit", exact: true }).click();
    return page.getByRole("dialog", { name: "Edit DC Tokyo", exact: true });
  };
  let d = await edit();
  await d.getByRole("combobox", { name: "Proxy chain choice" }).click();
  await page.getByRole("option", { name: "Use first hop for this proxy", exact: true }).click();
  await d.getByRole("button", { name: "Save", exact: true }).click();
  await expect(d).toBeHidden();
  await expect(page.locator("aside[data-peek='proxy'] [data-property='chain-policy']")).toContainText("Use first hop for this proxy");
  const after = await page.evaluate(() => (window as any).__shardxMock.state.proxies.find((p: any) => p.id === "px-jp-1"));
  expect(after).toEqual({ ...before, proxy_chain_policy: "enabled" });
  d = await edit();
  await expect(d.getByRole("combobox", { name: "Proxy chain choice" })).toContainText("Use first hop");
  await page.screenshot({ path: info.outputPath("proxy-policy-light.png"), animations: "disabled" });
  await d.getByRole("combobox", { name: "Proxy chain choice" }).click();
  await page.getByRole("option", { name: "Use assigned proxy only (skip first hop)", exact: true }).click();
  await d.getByRole("button", { name: "Cancel", exact: true }).click();
  const discard = page.getByRole("button", { name: /Discard|Don’t save|Don't save/i });
  if (await discard.count()) await discard.last().click();
  expect(await page.evaluate(() => (window as any).__shardxMock.state.proxies.find((p: any) => p.id === "px-jp-1").proxy_chain_policy)).toBe("enabled");
  await expect(page.locator("[data-chain-choice-note]")).toContainText("Each profile can override");
  await page.locator("aside[data-peek='proxy']").screenshot({ path: info.outputPath("proxy-choice-summary.png"), animations: "disabled" });
});

for (const language of ["zh", "ru"]) {
  test(`route choice stays readable in ${language} at the minimum window`, async ({ page }, info) => {
    await page.setViewportSize({ width: 1024, height: 680 });
    await page.addInitScript((lang) => localStorage.setItem("shardx.lang", lang), language);
    await openApp(page, { section: "proxies", theme: "light" });
    await page.locator("[role='row'][data-proxy-id='px-jp-1'] [data-cell='name']").click();
    const label = language === "zh" ? "编辑" : "Изменить";
    await page.locator("aside[data-peek='proxy'] [data-peek-header]").getByRole("button", { name: label, exact: true }).click();
    const d = page.getByRole("dialog");
    const policy = d.locator("[data-chain-policy]");
    await policy.scrollIntoViewIfNeeded();
    await expect(policy).toBeVisible();
    const preferred = language === "zh" ? "优先第一层，失败时仅用配置代理" : "Сначала первый узел, при отказе только назначенный прокси";
    await policy.getByRole("combobox").click();
    const option = page.getByRole("option", { name: preferred, exact: true });
    await expect(option).toBeVisible();
    const menu = page.getByRole("listbox");
    const bounds = (await menu.boundingBox())!;
    expect(bounds.x).toBeGreaterThanOrEqual(0);
    expect(bounds.y).toBeGreaterThanOrEqual(0);
    expect(bounds.x + bounds.width).toBeLessThanOrEqual(1024);
    expect(bounds.y + bounds.height).toBeLessThanOrEqual(680);
    expect(await option.evaluate(el => el.scrollWidth - el.clientWidth)).toBeLessThanOrEqual(1);
    await page.screenshot({ path: info.outputPath(`proxy-policy-${language}-options-1024.png`), animations: "disabled" });
    await option.click();
    await expect(d.locator("[data-chain-prefer-help]")).toBeVisible();
    expect(await policy.evaluate(el => el.scrollWidth - el.clientWidth)).toBeLessThanOrEqual(1);
    expect(await page.evaluate(() => document.documentElement.scrollWidth - innerWidth)).toBeLessThanOrEqual(1);
    await page.screenshot({ path: info.outputPath(`proxy-policy-${language}-1024.png`), animations: "disabled" });
  });
}

import { expect, test } from "@playwright/test";
import { openApp, openEditForm, reloadApp } from "./support/app";

test("profile chain choice is saved, reopened and does not unbind its proxy", async ({ page }) => {
  await openApp(page, { theme: "light" });
  const before = await page.evaluate(() => (window as any).__shardxMock.state.profiles["f62a9d43"]._meta.proxy_id);
  let form = await openEditForm(page, "Sandbox", "Proxy");
  await form.getByRole("combobox", { name: "Proxy chain choice" }).click();
  await page.getByRole("option", { name: "Do not use first hop", exact: true }).click();
  await form.getByRole("button", { name: "Save", exact: true }).click();
  await expect(form).toBeHidden();
  expect(await page.evaluate(() => (window as any).__shardxMock.state.profiles["f62a9d43"]._meta.proxy_chain_policy)).toBe("disabled");
  expect(await page.evaluate(() => (window as any).__shardxMock.state.profiles["f62a9d43"]._meta.proxy_id)).toBe(before);
  await reloadApp(page);
  form = await openEditForm(page, "Sandbox", "Proxy");
  await expect(form.getByRole("combobox", { name: "Proxy chain choice" })).toContainText("Do not use first hop");
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
  await page.getByRole("option", { name: "Do not use first hop", exact: true }).click();
  await d.getByRole("button", { name: "Cancel", exact: true }).click();
  const discard = page.getByRole("button", { name: /Discard|Don’t save|Don't save/i });
  if (await discard.count()) await discard.last().click();
  expect(await page.evaluate(() => (window as any).__shardxMock.state.proxies.find((p: any) => p.id === "px-jp-1").proxy_chain_policy)).toBe("enabled");
  await expect(page.locator("[data-chain-choice-note]")).toContainText("profile choice overrides");
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
    expect(await policy.evaluate(el => el.scrollWidth - el.clientWidth)).toBeLessThanOrEqual(1);
    expect(await page.evaluate(() => document.documentElement.scrollWidth - innerWidth)).toBeLessThanOrEqual(1);
    await page.screenshot({ path: info.outputPath(`proxy-policy-${language}-1024.png`), animations: "disabled" });
  });
}

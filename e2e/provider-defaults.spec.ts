import { expect, test, type Page } from "@playwright/test";
import { openApp, reloadApp } from "./support/app";

async function openSource(page: Page, source: string) {
  await page.locator("main header").getByRole("button", { name: "Add proxies", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "Add proxies", exact: true });
  await dialog.getByRole("combobox", { name: "Proxy source", exact: true }).click();
  await page.getByRole("option", { name: source, exact: true }).click();
  return dialog;
}

test("provider credentials persist separately and generate a SOCKS5 preview", async ({ page }, info) => {
  await openApp(page, { section: "proxies", theme: "light" });
  let dialog = await openSource(page, "1024Proxy");
  await dialog.getByText("Provider defaults", { exact: true }).click();
  const panel = dialog.locator("[data-provider-defaults]");
  await panel.getByRole("textbox", { name: "Account / base username", exact: true }).fill("synthetic-base");
  await panel.getByLabel("Password", { exact: true }).fill("synthetic-provider-password");
  await panel.getByRole("button", { name: "Save provider defaults", exact: true }).click();
  await expect(panel.getByRole("status")).toHaveText("Saved");
  await expect(panel.getByLabel("Password", { exact: true })).toHaveValue("");
  await reloadApp(page);
  dialog = await openSource(page, "1024Proxy");
  await dialog.getByText("Provider defaults", { exact: true }).click();
  await expect(dialog.getByRole("textbox", { name: "Account / base username", exact: true })).toHaveValue("synthetic-base");
  await dialog.getByText("Provider defaults", { exact: true }).click();
  await dialog.getByRole("combobox", { name: "Country", exact: true }).click();
  const countryPicker = page.getByRole("combobox", { name: "Search country or code", exact: true });
  await countryPicker.fill("US");
  await page.getByRole("option", { name: /^US,/ }).click();
  await dialog.getByRole("textbox", { name: "State / province (provider name, optional)", exact: true }).fill("Louisiana");
  await dialog.getByRole("button", { name: "Generate and preview", exact: true }).click();
  await expect(dialog.getByRole("button", { name: "Add 1 proxy", exact: true })).toBeVisible();
  await dialog.screenshot({ path: info.outputPath("1024proxy-preview.png"), animations: "disabled" });
  await dialog.getByRole("button", { name: "Add 1 proxy", exact: true }).click();
  await expect(dialog).toHaveCount(0);
  const entry = await page.evaluate(() => (window as any).__shardxMock.state.proxies.find((p: any) => p.provider_id === "1024proxy"));
  expect(entry).toMatchObject({ kind: "socks5", host: "us.1024proxy.io", port: 3000, username: "synthetic-base-region-US-st-Louisiana", password: "synthetic-provider-password" });
  const peek = page.locator("aside[data-peek='proxy']");
  await peek.locator("[data-peek-header]").getByRole("button", { name: "Edit", exact: true }).click();
  const edit = page.getByRole("dialog", { name: "Edit 1024Proxy US", exact: true });
  await edit.getByRole("combobox", { name: "Proxy source", exact: true }).click();
  await page.getByRole("option", { name: "TNBProxy", exact: true }).click();
  await edit.getByRole("button", { name: "Save", exact: true }).click();
  await expect(edit).toHaveCount(0);
  await expect(peek.locator("[data-property='provider']")).toContainText("tnbproxy");
});

test("TNB defaults restore after reload and authentication switches remain independent", async ({ page }, info) => {
  await openApp(page, { section: "proxies", theme: "dark" });
  let dialog = await openSource(page, "TNBProxy");
  await expect(dialog.getByRole("textbox", { name: "Extraction server", exact: true })).toHaveValue("http://hkdata.adv.tnbproxy.com:9090");
  await dialog.getByText("Provider defaults", { exact: true }).click();
  const panel = dialog.locator("[data-provider-defaults]");
  await panel.getByRole("textbox", { name: "Extraction server", exact: true }).fill("https://custom.tnbproxy.com:9443");
  await panel.getByRole("textbox", { name: "Extraction API username (optional)", exact: true }).fill("api-account");
  await panel.getByLabel("Extraction API password", { exact: true }).fill("api-password");
  await panel.getByRole("textbox", { name: "Generated proxy username (optional)", exact: true }).fill("proxy-account");
  await panel.getByLabel("Generated proxy password", { exact: true }).fill("proxy-password");
  await panel.getByRole("button", { name: "Save provider defaults", exact: true }).click();
  await expect(panel.getByRole("status")).toHaveText("Saved");
  await reloadApp(page);
  dialog = await openSource(page, "TNBProxy");
  await expect(dialog.getByRole("textbox", { name: "Extraction server", exact: true })).toHaveValue("https://custom.tnbproxy.com:9443");
  await expect(dialog.getByRole("checkbox", { name: "Use saved API authentication", exact: true })).not.toBeChecked();
  await expect(dialog.getByRole("checkbox", { name: "Fill missing proxy authentication", exact: true })).not.toBeChecked();
  await dialog.getByRole("checkbox", { name: "Fill missing proxy authentication", exact: true }).check();
  await dialog.screenshot({ path: info.outputPath("tnb-authentication.png"), animations: "disabled" });
  await dialog.getByRole("button", { name: "Extract and preview", exact: true }).click();
  const request = await page.evaluate(() => (window as any).__shardxMock.log.filter((v: any) => v.cmd === "tnbproxy_extract").at(-1).args.request);
  expect(request).toMatchObject({ use_api_credentials: false, use_proxy_credentials: true });
  expect(request).not.toHaveProperty("password");
});


test("settings provider defaults are shared with the add dialog after reload", async ({ page }) => {
  await openApp(page, { section: "settings" });
  await page.getByRole("tab", { name: "Proxy", exact: true }).click();
  const panel = page.locator("[data-provider-defaults]");
  await panel.getByRole("textbox", { name: "Extraction server", exact: true }).fill("https://settings.tnbproxy.com:9443");
  await panel.getByRole("button", { name: "Save provider defaults", exact: true }).click();
  await expect(panel.getByRole("status")).toHaveText("Saved");
  await reloadApp(page);
  await expect(panel.getByRole("textbox", { name: "Extraction server", exact: true })).toHaveValue("https://settings.tnbproxy.com:9443");
  await page.locator("[data-nav='proxies']").click();
  const dialog = await openSource(page, "TNBProxy");
  await expect(dialog.getByRole("textbox", { name: "Extraction server", exact: true })).toHaveValue("https://settings.tnbproxy.com:9443");
});

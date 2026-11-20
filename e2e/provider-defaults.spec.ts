import { expect, test, type Page } from "@playwright/test";
import { openApp, reloadApp } from "./support/app";

test("providers stay in navigation while legacy account tools are hidden", async ({ page }) => {
  await openApp(page, { section: "proxyshard" });
  await expect(page.locator("[data-nav='proxyshard']")).toBeVisible();
  await expect(page.locator("[data-provider-defaults]")).toBeVisible();
  await expect(page.getByRole("button", { name: "ProxyShard account tools", exact: true })).toHaveCount(0);
});

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
  await panel.getByRole("combobox", { name: "Session type", exact: true }).click();
  await page.getByRole("option", { name: "Per request", exact: true }).click();
  await panel.getByLabel("Password", { exact: true }).fill("synthetic-provider-password");
  await panel.getByRole("button", { name: "Save provider defaults", exact: true }).click();
  await expect(panel.getByRole("status")).toHaveText("Saved");
  await expect(panel.getByLabel("Password", { exact: true })).toHaveValue("");
  await reloadApp(page);
  dialog = await openSource(page, "1024Proxy");
  await dialog.getByText("Provider defaults", { exact: true }).click();
  await expect(dialog.getByRole("textbox", { name: "Account / base username", exact: true })).toHaveValue("synthetic-base");
  await expect(dialog.locator("[data-provider-defaults]").getByRole("combobox", { name: "Session type", exact: true })).toContainText("Per request");
  await dialog.getByText("Provider defaults", { exact: true }).click();
  await dialog.getByRole("combobox", { name: "Session type", exact: true }).click();
  await page.getByRole("option", { name: "Sticky session", exact: true }).click();
  await dialog.getByRole("combobox", { name: "Country", exact: true }).click();
  const countryPicker = page.getByRole("combobox", { name: "Search country or code", exact: true });
  await countryPicker.fill("US");
  await page.getByRole("option", { name: /^US,/ }).click();
  await dialog.getByRole("textbox", { name: "State / province (provider name, optional)", exact: true }).fill("Louisiana");
  await dialog.getByRole("textbox", { name: "City", exact: true }).fill("New Orleans");
  await dialog.getByRole("button", { name: "Generate and preview", exact: true }).click();
  await expect(dialog.getByRole("button", { name: "Add 1 proxy", exact: true })).toBeVisible();
  await dialog.screenshot({ path: info.outputPath("1024proxy-preview.png"), animations: "disabled" });
  await dialog.getByRole("button", { name: "Add 1 proxy", exact: true }).click();
  await expect(dialog).toHaveCount(0);
  const entry = await page.evaluate(() => (window as any).__shardxMock.state.proxies.find((p: any) => p.provider_id === "1024proxy"));
  expect(entry).toMatchObject({ kind: "socks5", host: "us.1024proxy.io", port: 3000, username: expect.stringMatching(/^synthetic-base-region-US-st-Louisiana-city-New Orleans-sid-[a-f0-9]{8}-t-120$/), password: "synthetic-provider-password" });
  const peek = page.locator("aside[data-peek='proxy']");
  await peek.locator("[data-peek-header]").getByRole("button", { name: "Edit", exact: true }).click();
  const edit = page.getByRole("dialog", { name: "Edit 1024Proxy US", exact: true });
  await edit.getByRole("combobox", { name: "Proxy source", exact: true }).click();
  await page.getByRole("option", { name: "TNBProxy", exact: true }).click();
  await edit.getByRole("button", { name: "Save", exact: true }).click();
  await expect(edit).toHaveCount(0);
  await expect(peek.locator("[data-property='provider']")).toContainText("tnbproxy");

  dialog = await openSource(page, "1024Proxy");
  await dialog.getByRole("combobox", { name: "Country", exact: true }).click();
  await countryPicker.fill("EG");
  await page.getByRole("option", { name: "EG, Egypt", exact: true }).click();
  await dialog.getByRole("combobox", { name: "Session type", exact: true }).click();
  await page.getByRole("option", { name: "Sticky session", exact: true }).click();
  await dialog.getByRole("spinbutton", { name: "Session length (minutes)", exact: true }).fill("90");
  await dialog.getByRole("spinbutton", { name: "Generation count", exact: true }).fill("3");
  await dialog.getByRole("button", { name: "Generate and preview", exact: true }).click();
  await dialog.getByRole("button", { name: "Add 3 proxies", exact: true }).click();
  await expect(dialog).toHaveCount(0);
  const usernames = await page.evaluate(() => (window as any).__shardxMock.state.proxies
    .filter((p: any) => p.provider_id === "1024proxy" && p.country === "EG").map((p: any) => p.username));
  expect(usernames).toHaveLength(3);
  expect(new Set(usernames).size).toBe(3);
  expect(usernames.every((username: string) => username.endsWith("-t-90"))).toBe(true);
});

test("TNB defaults restore after reload and authentication switches remain independent", async ({ page }, info) => {
  await openApp(page, { section: "proxies", theme: "dark" });
  let dialog = await openSource(page, "TNBProxy");
  await expect(dialog.getByRole("textbox", { name: "Extraction server", exact: true })).toHaveValue("http://usdata.adv.tnbproxy.com:9090");
  await dialog.getByText("Provider defaults", { exact: true }).click();
  const panel = dialog.locator("[data-provider-defaults]");
  const batch = dialog.locator("[data-tnb-form]");
  await batch.getByRole("spinbutton", { name: "Count (1–1000)", exact: true }).fill("3");
  await panel.getByRole("textbox", { name: "Extraction server", exact: true }).fill("https://custom.tnbproxy.com:9443");
  await panel.getByRole("spinbutton", { name: "Session length (minutes)", exact: true }).fill("240");
  await panel.getByRole("spinbutton", { name: "Generation count", exact: true }).fill("2");
  await panel.getByText("Provider options", { exact: true }).click();
  await panel.getByRole("textbox", { name: "Extraction API username (optional)", exact: true }).fill("api-account");
  await panel.getByLabel("Extraction API password", { exact: true }).fill("api-password");
  await panel.getByRole("textbox", { name: "Generated proxy username (optional)", exact: true }).fill("proxy-account");
  await panel.getByLabel("Generated proxy password", { exact: true }).fill("proxy-password");
  await panel.getByRole("button", { name: "Save provider defaults", exact: true }).click();
  await expect(panel.getByRole("status")).toHaveText("Saved");
  await expect(batch.getByRole("spinbutton", { name: "Session duration (minutes)", exact: true })).toHaveValue("180");
  await expect(batch.getByRole("spinbutton", { name: "Count (1–1000)", exact: true })).toHaveValue("3");
  await reloadApp(page);
  dialog = await openSource(page, "TNBProxy");
  await expect(dialog.getByRole("textbox", { name: "Extraction server", exact: true })).toHaveValue("https://custom.tnbproxy.com:9443");
  await expect(dialog.getByRole("spinbutton", { name: "Session duration (minutes)", exact: true })).toHaveValue("180");
  await expect(dialog.getByRole("spinbutton", { name: "Count (1–1000)", exact: true })).toHaveValue("2");
  await expect(dialog.getByRole("checkbox", { name: "Use saved API authentication", exact: true })).toBeChecked();
  await expect(dialog.getByRole("checkbox", { name: "Fill missing proxy authentication", exact: true })).toBeChecked();
  await dialog.getByRole("checkbox", { name: "Use saved API authentication", exact: true }).uncheck();
  await dialog.screenshot({ path: info.outputPath("tnb-authentication.png"), animations: "disabled" });
  await dialog.getByRole("button", { name: "Extract and preview", exact: true }).click();
  const request = await page.evaluate(() => (window as any).__shardxMock.log.filter((v: any) => v.cmd === "tnbproxy_extract").at(-1).args.request);
  expect(request).toMatchObject({ ttl: 180, use_api_credentials: false, use_proxy_credentials: true });
  expect(request).not.toHaveProperty("password");
});


test("settings provider defaults are shared with the add dialog after reload", async ({ page }) => {
  await openApp(page, { section: "settings" });
  await page.getByRole("tab", { name: "Proxy", exact: true }).click();
  await page.locator("[data-provider-defaults-disclosure] > summary").click();
  const panel = page.locator("[data-provider-defaults]");
  await panel.getByRole("textbox", { name: "Extraction server", exact: true }).fill("https://settings.tnbproxy.com:9443");
  await panel.getByRole("button", { name: "Save provider defaults", exact: true }).click();
  await expect(panel.getByRole("status")).toHaveText("Saved");
  await reloadApp(page);
  await page.getByRole("tab", { name: "Proxy", exact: true }).click();
  await page.locator("[data-provider-defaults-disclosure] > summary").click();
  await expect(panel.getByRole("textbox", { name: "Extraction server", exact: true })).toHaveValue("https://settings.tnbproxy.com:9443");
  await page.locator("[data-nav='proxies']").click();
  const dialog = await openSource(page, "TNBProxy");
  await expect(dialog.getByRole("textbox", { name: "Extraction server", exact: true })).toHaveValue("https://settings.tnbproxy.com:9443");
});


test("custom provider defaults persist with protocol and separate credentials", async ({ page }) => {
  await openApp(page, { section: "settings" });
  await page.getByRole("tab", { name: "Proxy", exact: true }).click();
  await page.locator("[data-provider-defaults-disclosure] > summary").click();
  const panel = page.locator("[data-provider-defaults]");
  await panel.getByRole("button", { name: "Add provider", exact: true }).click();
  await panel.getByRole("textbox", { name: "Gateway URL", exact: true }).fill("https://proxy.example.test:8443");
  await panel.getByText("Provider options", { exact: true }).click();
  await panel.getByRole("combobox", { name: "Proxy protocol", exact: true }).click();
  await page.getByRole("option", { name: "HTTPS", exact: true }).click();
  await panel.getByRole("textbox", { name: "Extraction API username (optional)", exact: true }).fill("custom-api");
  await panel.getByLabel("Extraction API password", { exact: true }).fill("custom-api-password");
  await panel.getByRole("textbox", { name: "Generated proxy username (optional)", exact: true }).fill("custom-proxy");
  await panel.getByLabel("Generated proxy password", { exact: true }).fill("custom-proxy-password");
  await panel.getByRole("button", { name: "Save provider defaults", exact: true }).click();
  await expect(panel.getByRole("status")).toHaveText("Saved");
  await panel.getByRole("button", { name: "Add provider", exact: true }).click();
  await panel.getByRole("textbox", { name: "Custom provider name" }).fill("Second provider");
  await panel.getByRole("textbox", { name: "Gateway URL", exact: true }).fill("http://second.example.test:8081");
  await panel.getByRole("button", { name: "Save provider defaults", exact: true }).click();
  await expect(panel.getByRole("status")).toHaveText("Saved");
  await reloadApp(page);
  await page.getByRole("tab", { name: "Proxy", exact: true }).click();
  await page.locator("[data-provider-defaults-disclosure] > summary").click();
  await panel.getByRole("tab", { name: "Custom provider", exact: true }).click();
  await expect(panel.getByRole("textbox", { name: "Gateway URL", exact: true })).toHaveValue("https://proxy.example.test:8443");
  await panel.getByText("Provider options", { exact: true }).click();
  await expect(panel.getByRole("combobox", { name: "Proxy protocol", exact: true })).toContainText("HTTPS");
  await expect(panel.getByText("Saved", { exact: true })).toHaveCount(0);
  await panel.getByRole("tab", { name: "Second provider", exact: true }).click();
  await expect(panel.getByRole("textbox", { name: "Gateway URL", exact: true })).toHaveValue("http://second.example.test:8081");
  page.once("dialog", (confirmation) => confirmation.accept());
  await panel.getByRole("button", { name: "Delete provider", exact: true }).click();
  await expect(panel.getByRole("tab", { name: "Second provider", exact: true })).toHaveCount(0);
  await expect(panel.getByRole("tab", { name: "Custom provider", exact: true })).toBeVisible();
  await page.locator("[data-nav='proxies']").click();
  const dialog = await openSource(page, "Custom provider");
  await expect(dialog.getByRole("textbox", { name: "Gateway", exact: true })).toHaveValue("https://proxy.example.test:8443");
  await dialog.getByRole("button", { name: "Generate and preview", exact: true }).click();
  const selected = dialog.getByRole("checkbox", { name: "Select proxy.example.test:8443", exact: true });
  const previewRow = dialog.locator(".import-cols").filter({
    has: page.getByRole("checkbox", { name: "Select proxy.example.test:8443", exact: true }),
  });
  await expect(selected).toBeChecked();
  await expect(previewRow).toContainText("HTTPS");
  await expect(previewRow).toContainText("proxy.example.test:8443, custom-proxy");
  const request = await page.evaluate(() => (window as any).__shardxMock.log.filter((v: any) => v.cmd === "proxy_provider_generate").at(-1).args.request);
  expect(request).toMatchObject({ custom: true, country: "", state: "", city: "", custom_provider_id: expect.any(String) });
});

test("an API-only custom provider previews proxies before saving them", async ({ page }) => {
  await openApp(page, { section: "settings" });
  await page.getByRole("tab", { name: "Proxy", exact: true }).click();
  await page.locator("[data-provider-defaults-disclosure] > summary").click();
  const panel = page.locator("[data-provider-defaults]");
  await panel.getByRole("button", { name: "Add provider", exact: true }).click();
  await panel.getByRole("textbox", { name: "Custom provider name" }).fill("API provider");
  await panel.getByText("Provider options", { exact: true }).click();
  const url = "https://example.test/list?format=txt";
  await panel.getByRole("textbox", { name: "Extraction URL (HTTPS)", exact: true }).fill(url);
  await panel.getByRole("spinbutton", { name: "Generation count", exact: true }).fill("3");
  await panel.getByRole("textbox", { name: "Generated proxy username (optional)", exact: true }).fill("api-provider-exit");
  await panel.getByLabel("Generated proxy password", { exact: true }).fill("fixture-password");
  await panel.getByRole("button", { name: "Save provider defaults", exact: true }).click();
  await expect(panel.getByRole("status")).toHaveText("Saved");

  await page.locator("[data-nav='proxies']").click();
  const dialog = await openSource(page, "API provider");
  await expect(dialog.getByRole("textbox", { name: "Extraction URL (HTTPS)", exact: true })).toHaveValue(url);
  await expect(dialog.getByLabel("Maximum proxies to preview")).toHaveValue("3");
  await dialog.getByLabel("Maximum proxies to preview").fill("2");
  await dialog.getByRole("button", { name: "Extract and preview", exact: true }).click();
  await expect(dialog.getByRole("checkbox", { name: "Select proxy1.example.test:8080" })).toBeChecked();
  await expect(dialog.getByRole("checkbox", { name: "Select proxy2.example.test:8080" })).toBeChecked();
  await expect(dialog.getByText(/api-provider-exit/)).toHaveCount(2);
  const saves = () => page.evaluate(() => (window as any).__shardxMock.log.filter((entry: any) => entry.cmd === "proxy_bulk_save").length);
  expect(await saves()).toBe(0);
  await dialog.getByRole("button", { name: "Add 2 proxies", exact: true }).click();
  await expect(dialog).toBeHidden();
  expect(await saves()).toBe(1);
  await expect(page.getByText("proxy1.example.test:8080", { exact: true }).first()).toBeVisible();
});

test("provider save reports a specific invalid-defaults error", async ({ page }) => {
  await openApp(page, { section: "settings" });
  await page.getByRole("tab", { name: "Proxy", exact: true }).click();
  await page.locator("[data-provider-defaults-disclosure] > summary").click();
  const panel = page.locator("[data-provider-defaults]");
  await panel.getByRole("button", { name: "Add provider", exact: true }).click();
  await panel.getByRole("textbox", { name: "Gateway URL", exact: true }).fill("not-a-gateway");
  await panel.getByRole("button", { name: "Save provider defaults", exact: true }).click();
  await expect(panel.getByRole("alert")).toHaveText("Provider defaults are invalid. Check the gateway, protocol, region, and session settings.");
});

test("provider generation reports a specific invalid-gateway error", async ({ page }) => {
  await openApp(page, { section: "settings" });
  await page.getByRole("tab", { name: "Proxy", exact: true }).click();
  await page.locator("[data-provider-defaults-disclosure] > summary").click();
  const panel = page.locator("[data-provider-defaults]");
  await panel.getByRole("button", { name: "Add provider", exact: true }).click();
  await panel.getByRole("textbox", { name: "Gateway URL", exact: true }).fill("https://proxy.example.test:8443");
  await panel.getByText("Provider options", { exact: true }).click();
  await panel.getByRole("combobox", { name: "Proxy protocol", exact: true }).click();
  await page.getByRole("option", { name: "SOCKS5", exact: true }).click();
  await panel.getByRole("button", { name: "Save provider defaults", exact: true }).click();
  await expect(panel.getByRole("status")).toHaveText("Saved");
  await page.locator("[data-nav='proxies']").click();
  const dialog = await openSource(page, "Custom provider");
  await dialog.getByRole("button", { name: "Generate and preview", exact: true }).click();
  await expect(dialog.getByRole("alert")).toHaveText("Enter a gateway URL that matches the selected proxy protocol, with no API path.");
});

test("custom proxy entry uses the nested proxy folder selector", async ({ page }) => {
  await openApp(page, { section: "proxies" });
  const dialog = await openSource(page, "Custom (paste proxies)");
  await dialog.getByRole("button", { name: "Enter manually", exact: true }).click();
  await expect(dialog.getByRole("combobox", { name: "Folder", exact: true })).toBeVisible();
  await dialog.getByRole("button", { name: "Add folder", exact: true }).click();
  await dialog.getByRole("textbox", { name: "New folder", exact: true }).fill("Residential");
  await dialog.getByRole("button", { name: "Create", exact: true }).click();
  await dialog.getByRole("combobox", { name: "Folder", exact: true }).click();
  await expect(page.getByRole("option", { name: "Residential", exact: true })).toBeVisible();
  await page.getByRole("option", { name: "Residential", exact: true }).click();
  await expect(dialog.getByRole("combobox", { name: "Folder", exact: true })).toContainText("Residential");
});

test("TNB batch saves to the selected folder without changing the saved default", async ({ page }) => {
  await openApp(page, { section: "proxies" });
  let dialog = await openSource(page, "Custom (paste proxies)");
  await dialog.getByRole("button", { name: "Enter manually", exact: true }).click();
  await dialog.getByRole("button", { name: "Add folder", exact: true }).click();
  await dialog.getByRole("textbox", { name: "New folder", exact: true }).fill("Residential");
  await dialog.getByRole("button", { name: "Create", exact: true }).click();
  await expect(dialog.getByRole("combobox", { name: "Folder", exact: true })).toContainText("Residential");
  await dialog.getByRole("button", { name: "Cancel", exact: true }).click();

  dialog = await openSource(page, "TNBProxy");
  await dialog.getByText("Provider defaults", { exact: true }).click();
  const panel = dialog.locator("[data-provider-defaults]");
  await panel.getByRole("combobox", { name: "Folder", exact: true }).click();
  await page.getByRole("option", { name: "Residential", exact: true }).click();
  await panel.getByRole("button", { name: "Save provider defaults", exact: true }).click();
  await expect(panel.getByRole("status")).toHaveText("Saved");
  await dialog.getByText("Provider defaults", { exact: true }).click();
  const folder = dialog.getByRole("combobox", { name: "Folder", exact: true });
  await expect(folder).toContainText("Residential");
  await folder.click();
  await page.getByRole("option", { name: "No folder", exact: true }).click();
  await dialog.getByRole("spinbutton", { name: "Count (1–1000)", exact: true }).fill("2");
  await dialog.getByRole("button", { name: "Extract and preview", exact: true }).click();
  await dialog.getByRole("button", { name: "Add 2 proxies", exact: true }).click();
  await expect(dialog).toHaveCount(0);
  const savedFolders = await page.evaluate(() => (window as any).__shardxMock.state.proxies
    .filter((entry: any) => /^tnb-\d+\.proxy\.example$/.test(entry.host))
    .map((entry: any) => entry.folder ?? ""));
  expect(savedFolders).toEqual(["", ""]);

  await reloadApp(page);
  dialog = await openSource(page, "TNBProxy");
  await expect(dialog.getByRole("combobox", { name: "Folder", exact: true })).toContainText("Residential");
  await dialog.getByRole("button", { name: "Extract and preview", exact: true }).click();
  await expect(dialog.getByRole("combobox", { name: "Folder", exact: true })).toContainText("Residential");
});

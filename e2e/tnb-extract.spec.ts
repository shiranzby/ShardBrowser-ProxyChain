import { expect, test, type Page } from "@playwright/test";
import { openApp, reloadApp } from "./support/app";

type Mock = {
  __shardxMock: { calls: Record<string, number>; log: { cmd: string; args: Record<string, unknown> }[]; state: { proxies: { id: string; host: string; kind: string }[] } };
  __shardxMockSlow?: Record<string, number>;
  __shardxMockFailWith?: Record<string, string>;
};
const dialog = (page: Page) => page.getByRole("dialog", { name: "Add proxies", exact: true });
const count = (page: Page, cmd: string) => page.evaluate((key) => (window as unknown as Mock).__shardxMock.calls[key] ?? 0, cmd);
const proxies = (page: Page) => page.evaluate(() => (window as unknown as Mock).__shardxMock.state.proxies);
const request = (page: Page) => page.evaluate(() => (window as unknown as Mock).__shardxMock.log.filter((x) => x.cmd === "tnbproxy_extract").at(-1)?.args.request);
async function openTnb(page: Page) {
  await openApp(page, { section: "proxies", theme: "light" });
  await page.locator("main header").getByRole("button", { name: "Add proxies", exact: true }).click();
  await dialog(page).getByRole("combobox", { name: "Proxy source" }).click();
  await page.getByRole("option", { name: "TNBProxy", exact: true }).click();
}

test("TNB extracts a preview and saves only selected entries through the existing bulk path", async ({ page }, info) => {
  await openTnb(page);
  const d = dialog(page);
  await expect(d.getByRole("spinbutton", { name: "Count (1–1000)" })).toHaveValue("1");
  await d.getByRole("spinbutton", { name: "Count (1–1000)" }).fill("2");
  await d.locator("[data-tnb-location='country'] button").click();
  await page.getByRole("combobox", { name: "Search country or code" }).fill("EG");
  await page.getByRole("option", { name: /Egypt/ }).click();
  await d.locator("[data-tnb-location='region'] button").click();
  await page.getByRole("option", { name: "Cairo", exact: true }).click();
  await d.locator("[data-tnb-location='city'] button").click();
  await page.getByRole("option", { name: "Cairo", exact: true }).click();
  await d.getByRole("spinbutton", { name: "Session duration (minutes)" }).fill("20");
  await d.getByRole("button", { name: "Extract and preview" }).click();
  await expect(d.locator("[data-tnb-preview]")).toContainText("Received 2 proxies via Direct");
  expect(await count(page, "proxy_bulk_save")).toBe(0);
  expect(await proxies(page)).toHaveLength(7);
  expect(await request(page)).toMatchObject({ server: "http://usdata.adv.tnbproxy.com:9090", plan_name: "advanced_traffic", count: 2, kind: "http", geo: "EG_Cairo_Cairo", ttl: 20, pre: false, route: "direct" });
  await d.getByRole("checkbox", { name: /tnb-2\.proxy\.example/ }).uncheck();
  await d.screenshot({ path: info.outputPath("tnb-preview.png"), animations: "disabled" });
  await d.getByRole("button", { name: "Add 1 proxy", exact: true }).click();
  await expect(d).toBeHidden();
  expect(await count(page, "proxy_bulk_save")).toBe(1);
  expect((await proxies(page)).filter((p) => p.host.startsWith("tnb-"))).toEqual([expect.objectContaining({ host: "tnb-1.proxy.example", kind: "http" })]);
  await reloadApp(page);
  await expect(page.locator("main [data-proxy-id][role='row']")).toHaveCount(8);
});

test("explicit first-hop extraction keeps fields on failure, has no direct retry, and prevents duplicate requests", async ({ page }) => {
  await openTnb(page);
  const d = dialog(page);
  await d.getByRole("combobox", { name: "Extraction route" }).click();
  await page.getByRole("option", { name: "Saved first hop", exact: true }).click();
  await d.getByRole("combobox", { name: "Proxy protocol" }).click();
  await page.getByRole("option", { name: "SOCKS5", exact: true }).click();
  await d.getByRole("textbox", { name: "Plan name" }).fill("my-plan");
  await page.evaluate(() => { (window as unknown as Mock).__shardxMockFailWith = { tnbproxy_extract: "tnb_route_failed" }; });
  await d.getByRole("button", { name: "Extract and preview" }).click();
  await expect(d.getByRole("alert")).toContainText("no direct retry was made");
  await expect(d.getByRole("textbox", { name: "Plan name" })).toHaveValue("my-plan");
  expect(await count(page, "tnbproxy_extract")).toBe(1);
  expect(await count(page, "proxy_bulk_save")).toBe(0);
  await page.evaluate(() => { (window as unknown as Mock).__shardxMockSlow = { tnbproxy_extract: 1800 }; });
  await d.getByRole("button", { name: "Extract and preview" }).click();
  await expect(d.getByRole("button", { name: "Extract and preview" })).toBeDisabled();
  await expect(d.getByRole("button", { name: "Cancel", exact: true })).toBeDisabled();
  await expect(d.getByRole("textbox", { name: "Plan name" })).toBeDisabled();
  await expect(d.locator("[data-tnb-preview]")).toContainText("Saved first hop");
  expect(await count(page, "tnbproxy_extract")).toBe(2);
  expect(await request(page)).toMatchObject({ route: "first_hop", kind: "socks5", plan_name: "my-plan" });
  expect(await proxies(page)).toHaveLength(7);
  await d.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(d).toBeHidden();
  expect(await count(page, "proxy_bulk_save")).toBe(0);
});

test("invalid counts, sessions and URLs never send a request; advanced options reach the request", async ({ page }) => {
  await openTnb(page);
  const d = dialog(page);
  const extract = d.getByRole("button", { name: "Extract and preview" });
  await d.getByRole("spinbutton", { name: "Count (1–1000)" }).fill("1001");
  await extract.click();
  await expect(d.getByRole("alert")).toBeVisible();
  expect(await count(page, "tnbproxy_extract")).toBe(0);
  await d.getByRole("spinbutton", { name: "Count (1–1000)" }).fill("1");
  await d.getByRole("spinbutton", { name: "Session duration (minutes)" }).fill("181");
  await extract.click();
  expect(await count(page, "tnbproxy_extract")).toBe(0);
  await d.getByRole("spinbutton", { name: "Session duration (minutes)" }).fill("0");
  await d.getByRole("textbox", { name: "Extraction server" }).fill("http://user:pass@proxy.example:9090/path");
  await extract.click();
  expect(await count(page, "tnbproxy_extract")).toBe(0);
  await d.getByRole("textbox", { name: "Extraction server" }).fill("https://synthetic.tnb.example:9090");
  await d.locator("summary").click();
  await d.getByRole("textbox", { name: "Pool", exact: true }).fill("pool-a");
  await d.getByRole("textbox", { name: "ISP", exact: true }).fill("carrier");
  await d.getByRole("textbox", { name: "ASN", exact: true }).fill("1234");
  await d.getByRole("textbox", { name: "Preferred IP", exact: true }).fill("192.0.2.9");
  await d.getByRole("textbox", { name: "Session ID", exact: true }).fill("session-a");
  await d.getByRole("checkbox", { name: "Enable TNB pre option" }).check();
  await extract.click();
  await expect(d.locator("[data-tnb-preview]")).toBeVisible();
  expect(await request(page)).toMatchObject({ pool: "pool-a", isp: "carrier", asn: "1234", ip: "192.0.2.9", sid: "session-a", pre: true });
});

test("a failed save retains the preview; going back retains the extraction inputs", async ({ page }) => {
  await openTnb(page);
  const d = dialog(page);
  await d.getByRole("spinbutton", { name: "Count (1–1000)" }).fill("2");
  await d.getByRole("button", { name: "Extract and preview" }).click();
  await expect(d.locator("[data-tnb-preview]")).toBeVisible();
  await page.evaluate(() => { (window as unknown as Mock).__shardxMockFailWith = { proxy_bulk_save: "Synthetic save failure" }; });
  await d.getByRole("button", { name: "Add 2 proxies", exact: true }).click();
  await expect(d.getByRole("button", { name: "Add 2 proxies", exact: true })).toBeEnabled();
  await expect(d.locator("[data-tnb-preview]")).toBeVisible();
  expect(await proxies(page)).toHaveLength(7);
  await d.getByRole("button", { name: "Back", exact: true }).click();
  await expect(d.getByRole("spinbutton", { name: "Count (1–1000)" })).toHaveValue("2");
  await expect(d.getByRole("textbox", { name: "Extraction server" })).toHaveValue("http://usdata.adv.tnbproxy.com:9090");
});

test("the custom source still previews pasted proxies and never calls TNB", async ({ page }) => {
  await openApp(page, { section: "proxies" });
  await page.locator("[data-tour='add-proxy']").click();
  const d = dialog(page);
  await expect(d.getByRole("combobox", { name: "Proxy source" })).toContainText("Custom (paste proxies)");
  await d.locator("textarea").fill("192.0.2.10:1080:synthetic:password");
  await d.getByRole("button", { name: "Preview", exact: true }).click();
  await expect(d.getByRole("button", { name: "Add 1 proxy", exact: true })).toBeVisible();
  expect(await count(page, "tnbproxy_extract")).toBe(0);
  expect(await count(page, "proxy_bulk_parse")).toBe(1);
  await d.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(d).toBeHidden();
  expect(await proxies(page)).toHaveLength(7);
});

for (const lang of ["en", "zh", "ru"]) {
  test(`TNB form fits the minimum window in ${lang}`, async ({ page }, info) => {
    await page.setViewportSize({ width: 1024, height: 680 });
    await page.addInitScript((language) => localStorage.setItem("shardx.lang", language), lang);
    await openApp(page, { section: "proxies", theme: "light" });
    await page.locator("[data-tour='add-proxy']").click();
    const d = page.getByRole("dialog");
    await d.getByRole("combobox").first().click();
    await page.getByRole("option", { name: "TNBProxy", exact: true }).click();
    const form = d.locator("[data-tnb-form]");
    await expect(form).toBeVisible();
    expect(await form.evaluate((el) => el.scrollWidth - el.clientWidth)).toBeLessThanOrEqual(1);
    expect(await page.evaluate(() => document.documentElement.scrollWidth - innerWidth)).toBeLessThanOrEqual(1);
    await d.screenshot({ path: info.outputPath(`tnb-${lang}-compact.png`), animations: "disabled" });
  });
}

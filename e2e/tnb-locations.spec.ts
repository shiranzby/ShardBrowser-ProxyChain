import { expect, test, type Page } from "@playwright/test";
import { openApp, waitForIdle } from "./support/app";

type Mock = {
  __shardxMock: { calls: Record<string, number>; log: { cmd: string; args: { request?: Record<string, unknown> } }[] };
  __shardxMockSlow?: Record<string, number>;
  __shardxMockFailWith?: Record<string, string>;
};
const form = (page: Page) => page.getByRole("dialog", { name: "Add proxies", exact: true });
const field = (page: Page, level: string) => form(page).locator(`[data-tnb-location='${level}'] button`);
const requests = (page: Page, command = "tnbproxy_locations") => page.evaluate((cmd) => (window as unknown as Mock).__shardxMock.log.filter((entry) => entry.cmd === cmd).map((entry) => entry.args.request), command);
async function openTnb(page: Page) {
  await openApp(page, { section: "proxies", theme: "light" });
  await page.locator("[data-tour='add-proxy']").click();
  await form(page).getByRole("combobox", { name: "Proxy source" }).click();
  await page.getByRole("option", { name: /TNB/i }).click();
}
async function egypt(page: Page) {
  await field(page, "country").click();
  await page.getByRole("combobox", { name: "Search country or code" }).fill("EG");
  await page.getByRole("option", { name: /Egypt/ }).click();
}
async function cairo(page: Page, level: "region" | "city") {
  await field(page, level).click();
  await page.getByRole("option", { name: "Cairo", exact: true }).click();
}

test("country search accepts EG, English and Chinese names; three selected codes reach extraction unchanged", async ({ page }, info) => {
  await openTnb(page);
  await expect(field(page, "region")).toBeDisabled();
  await expect(field(page, "city")).toBeDisabled();
  await field(page, "country").click();
  const search = page.getByRole("combobox", { name: "Search country or code" });
  for (const query of ["EG", "Egypt", "埃及"]) {
    await search.fill(query);
    await expect(page.getByRole("option", { name: /Egypt/ })).toHaveCount(1);
  }
  await page.getByRole("option", { name: /Egypt/ }).click();
  await cairo(page, "region");
  await cairo(page, "city");
  expect(await requests(page)).toEqual([
    expect.objectContaining({ level: "country", route: "direct" }),
    expect.objectContaining({ level: "state", country: "EG", route: "direct" }),
    expect.objectContaining({ level: "city", country: "EG", state: "Cairo", route: "direct" }),
  ]);
  await form(page).screenshot({ path: info.outputPath("tnb-cairo-selected.png"), animations: "disabled" });
  await form(page).getByRole("button", { name: "Extract and preview" }).click();
  await expect(form(page).locator("[data-tnb-preview]")).toBeVisible();
  expect((await requests(page, "tnbproxy_extract"))[0]).toMatchObject({ geo: "EG_Cairo_Cairo" });
});

test("changing a parent clears descendants and any location omits the unselected levels", async ({ page }) => {
  await openTnb(page);
  await egypt(page);
  await cairo(page, "region");
  await cairo(page, "city");
  await field(page, "region").click();
  await page.getByRole("option", { name: "Any state or region", exact: true }).click();
  await expect(field(page, "city")).toBeDisabled();
  await expect(field(page, "city")).toHaveText("Any city");
  await form(page).getByRole("button", { name: "Extract and preview" }).click();
  await expect(form(page).locator("[data-tnb-preview]")).toBeVisible();
  expect((await requests(page, "tnbproxy_extract"))[0]).toMatchObject({ geo: "EG" });
  await form(page).getByRole("button", { name: "Back", exact: true }).click();
  await field(page, "country").click();
  await page.getByRole("option", { name: /United States/ }).click();
  await expect(field(page, "region")).toHaveText("Any state or region");
  await expect(field(page, "city")).toBeDisabled();
  await field(page, "country").click();
  await page.getByRole("option", { name: "Any country", exact: true }).click();
  await expect(field(page, "region")).toBeDisabled();
  await form(page).getByRole("button", { name: "Extract and preview" }).click();
  await expect(form(page).locator("[data-tnb-preview]")).toBeVisible();
  expect((await requests(page, "tnbproxy_extract"))[1]).toMatchObject({ geo: "" });
});

test("a late state response cannot replace the current country’s empty result", async ({ page }) => {
  await openTnb(page);
  await egypt(page);
  await page.evaluate(() => { (window as unknown as Mock).__shardxMockSlow = { tnbproxy_locations: 1500 }; });
  await field(page, "region").click();
  await expect(page.getByText("Loading available locations…", { exact: true })).toBeVisible();
  await expect.poll(() => requests(page)).toHaveLength(2);
  await page.keyboard.press("Escape");
  await page.evaluate(() => { (window as unknown as Mock).__shardxMockSlow = {}; });
  await field(page, "country").click();
  await page.getByRole("option", { name: /United States/ }).click();
  await field(page, "region").click();
  await expect(page.getByText("No specific locations are available for this selection.", { exact: true })).toBeVisible();
  await waitForIdle(page);
  await expect(page.getByRole("option", { name: "Cairo", exact: true })).toHaveCount(0);
  await expect(page.getByText("No specific locations are available for this selection.", { exact: true })).toBeVisible();
});

test("a location failure keeps the chosen route and retries explicitly without a direct fallback", async ({ page }) => {
  await openTnb(page);
  await form(page).getByRole("combobox", { name: "Extraction route" }).click();
  await page.getByRole("option", { name: "Saved first hop", exact: true }).click();
  await page.evaluate(() => { (window as unknown as Mock).__shardxMockFailWith = { tnbproxy_locations: "tnb_locations_route_failed" }; });
  await field(page, "country").click();
  await expect(page.getByText("The first hop could not load TNB locations. No direct retry was made.", { exact: true })).toBeVisible();
  expect(await requests(page)).toHaveLength(1);
  await page.getByRole("button", { name: "Retry", exact: true }).click();
  await expect(page.getByRole("option", { name: /Egypt/ })).toBeVisible();
  expect((await requests(page)).map((r) => r?.route)).toEqual(["first_hop", "first_hop"]);
});

test("unmatched text cannot become a location and a custom server offers only any location", async ({ page }) => {
  await openTnb(page);
  await field(page, "country").click();
  await page.getByRole("combobox", { name: "Search country or code" }).fill("not-a-real-country");
  await expect(page.getByText("No matching location", { exact: true })).toBeVisible();
  await page.keyboard.press("Enter");
  await page.keyboard.press("Escape");
  await page.keyboard.press("Escape");
  await expect(field(page, "country")).toHaveText("Any country");
  await egypt(page);
  await cairo(page, "region");
  await form(page).getByRole("textbox", { name: "Extraction server" }).fill("http://synthetic.example:9090");
  await expect(field(page, "country")).toHaveText("Any country");
  await expect(field(page, "region")).toBeDisabled();
  await field(page, "country").click();
  await expect(page.getByText("This custom server has no supported location catalog. Only any location is available.", { exact: true })).toBeVisible();
  await expect(page.getByRole("option")).toHaveCount(1);
  await page.getByRole("option", { name: "Any country", exact: true }).click();
  await form(page).getByRole("button", { name: "Extract and preview" }).click();
  await expect(form(page).locator("[data-tnb-preview]")).toBeVisible();
  expect((await requests(page, "tnbproxy_extract"))[0]).toMatchObject({ server: "http://synthetic.example:9090", geo: "" });
});

test("Chinese location menus fit the minimum window and support keyboard selection from EG to Cairo", async ({ page }, info) => {
  await page.setViewportSize({ width: 1024, height: 680 });
  await page.addInitScript(() => localStorage.setItem("shardx.lang", "zh"));
  await openApp(page, { section: "proxies", theme: "light" });
  await page.locator("[data-tour='add-proxy']").click();
  const d = page.getByRole("dialog", { name: "添加代理", exact: true });
  await d.getByRole("combobox", { name: "代理来源" }).click();
  await page.getByRole("option", { name: "TNBProxy", exact: true }).click();
  const control = (level: string) => d.locator(`[data-tnb-location='${level}'] button`);
  const captureMenu = async (name: string, filename: string) => {
    const popup = page.getByRole("dialog", { name, exact: true });
    await expect(popup).toBeVisible();
    const box = (await popup.boundingBox())!;
    expect(box.x).toBeGreaterThanOrEqual(0);
    expect(box.y).toBeGreaterThanOrEqual(0);
    expect(box.x + box.width).toBeLessThanOrEqual(1024);
    expect(box.y + box.height).toBeLessThanOrEqual(680);
    expect(await popup.evaluate((el) => el.scrollWidth - el.clientWidth)).toBeLessThanOrEqual(1);
    const optionsFit = await popup.getByRole("option").evaluateAll((els) => els.every((el) => {
      const item = el as HTMLElement;
      const popup = el.closest("[role='dialog']")!.getBoundingClientRect();
      const r = el.getBoundingClientRect();
      return item.scrollWidth <= item.clientWidth + 1 && r.left >= popup.left && r.right <= popup.right;
    }));
    expect(optionsFit).toBe(true);
    expect(await page.evaluate(() => document.documentElement.scrollWidth - innerWidth)).toBeLessThanOrEqual(1);
    await page.screenshot({ path: info.outputPath(filename), animations: "disabled" });
  };

  await control("country").click();
  const countrySearch = page.getByRole("combobox", { name: "搜索国家/地区名称或代码" });
  await countrySearch.fill("EG");
  await expect(page.getByRole("option", { name: /埃及/ })).toHaveCount(1);
  await captureMenu("国家/地区", "tnb-zh-01-eg-search.png");
  await countrySearch.press("Enter");
  await expect(control("country")).toHaveText("埃及");

  await control("region").click();
  await expect(page.getByRole("option", { name: "Cairo", exact: true })).toBeVisible();
  await expect(page.getByRole("option", { name: "AlQahirah", exact: true })).toBeVisible();
  await captureMenu("州/省", "tnb-zh-02-eg-states.png");
  const regionSearch = page.getByRole("combobox", { name: "搜索州/省" });
  await regionSearch.fill("Cairo");
  await regionSearch.press("Enter");
  await expect(control("region")).toHaveText("Cairo");

  await control("city").click();
  await expect(page.getByRole("option", { name: "Cairo", exact: true })).toBeVisible();
  await expect(page.getByRole("option", { name: "NewCairo", exact: true })).toHaveCount(1);
  await captureMenu("城市", "tnb-zh-03-cairo-cities.png");
  const citySearch = page.getByRole("combobox", { name: "搜索城市" });
  await citySearch.fill("Cairo");
  await citySearch.press("ArrowDown");
  await citySearch.press("ArrowUp");
  await citySearch.press("Enter");
  await expect(control("city")).toHaveText("Cairo");
  expect(await requests(page)).toEqual([
    expect.objectContaining({ level: "country" }),
    expect.objectContaining({ level: "state", country: "EG" }),
    expect.objectContaining({ level: "city", country: "EG", state: "Cairo" }),
  ]);
  expect(await requests(page, "tnbproxy_extract")).toHaveLength(0);
});

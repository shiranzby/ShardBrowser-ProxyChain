import { expect, test, type Locator, type Page } from "@playwright/test";
import { appModule, openApp, reloadApp, seed } from "./support/app";
import { openSearch } from "./support/search";

const saved = (page: Page, kind: string, scope = "personal") => page.evaluate(([kind, scope]) => {
  const orders = JSON.parse(localStorage.getItem("shardx.list-order.v1") ?? "{}");
  return orders[JSON.stringify([scope, kind])] as string[] | undefined;
}, [kind, scope]);
const ids = (rows: Locator, attr: string) => rows.evaluateAll((els, key) => els.map((el) => el.getAttribute(key)), attr);
async function dragBefore(source: Locator, target: Locator) {
  await source.dragTo(target, { sourcePosition: { x: 100, y: 20 }, targetPosition: { x: 100, y: 4 } });
}

test("profiles drag within pin groups, restore on reload, and retain folder dragging", async ({ page }) => {
  await openApp(page);
  const rows = page.locator("main [data-profile-id][role='row']");
  await expect(rows).toHaveCount(8);
  const before = await ids(rows, "data-profile-id");
  const source = rows.filter({ hasText: "Etsy craft shop" });
  const target = rows.filter({ hasText: "eBay DE buyer" });
  await dragBefore(source, target);
  await expect.poll(() => ids(rows, "data-profile-id")).toEqual(before.map((id) => id === "aed41005" ? "9828bf28" : id === "9828bf28" ? "aed41005" : id));
  expect(await page.evaluate(() => localStorage.getItem("shardx.profiles.sort"))).toBe("manual");
  const ordered = await ids(rows, "data-profile-id");
  await reloadApp(page);
  await expect.poll(() => ids(rows, "data-profile-id")).toEqual(ordered);
  // Native folder moves still use the existing Profiles payload.
  await source.dragTo(page.locator("aside [data-folder='Ads']"), { sourcePosition: { x: 100, y: 20 } });
  await expect(source.locator("[data-cell='folder']")).toContainText("Ads");
});

test("proxy filtering keeps hidden positions, drag clears a column sort, and keys follow the saved order", async ({ page }) => {
  await seed(page, "shardx.proxies.sort", "name");
  await openApp(page, { section: "proxies" });
  const rows = page.locator("main [data-proxy-id][role='row']");
  await expect(rows).toHaveCount(7);
  const before = await ids(rows, "data-proxy-id");
  const search = await openSearch(page);
  await search.fill("DC");
  await expect(rows).toHaveCount(3);
  await dragBefore(rows.filter({ hasText: "DC Tokyo" }), rows.filter({ hasText: "DC New York" }));
  const order = await saved(page, "proxies");
  expect(order).toHaveLength(before.length);
  for (const id of before.filter((id) => !["px-us-2", "px-jp-1", "px-new"].includes(id!))) expect(order![before.indexOf(id)]).toBe(id);
  await search.fill("");
  await expect(rows).toHaveCount(before.length);
  const displayed = await ids(rows, "data-proxy-id");
  expect(displayed.indexOf("px-jp-1")).toBeLessThan(displayed.indexOf("px-us-2"));
  expect(await page.evaluate(() => localStorage.getItem("shardx.proxies.sort"))).toBe("manual");
  await rows.filter({ hasText: "DC Tokyo" }).focus();
  await page.keyboard.press("ArrowDown");
  const next = displayed[displayed.indexOf("px-jp-1") + 1];
  await expect(page.locator(`[data-proxy-id='${next}'][role='row']`)).toBeFocused();
  await reloadApp(page);
  await expect.poll(() => ids(rows, "data-proxy-id")).toEqual(displayed);
});

for (const entry of [
  { section: "fingerprints", kind: "fingerprints", first: "mac-m3-pro", second: "mac-m1" },
  { section: "extensions", kind: "extensions", first: "ublock", second: "cookie-editor" },
  { section: "bookmarks", kind: "bookmarks", first: "bm-1", second: "bm-2" },
  { section: "automation", kind: "projects", first: "proj-pricing", second: "proj-warmup" },
]) {
  test(`${entry.kind}: native drag and keyboard menu persist without opening or editing the item`, async ({ page }) => {
    if (entry.kind === "bookmarks") await seed(page, "shardx.bookmarks.layout", { sort: { id: "name", dir: "descending" } });
    await openApp(page, { section: entry.section });
    const rows = page.locator("main [data-row-id]");
    const a = page.locator(`main [data-row-id='${entry.first}']`);
    const b = page.locator(`main [data-row-id='${entry.second}']`);
    await expect(a).toBeVisible();
    await expect(b).toBeVisible();
    const before = await ids(rows, "data-row-id");
    const firstAt = before.indexOf(entry.first), secondAt = before.indexOf(entry.second);
    const later = firstAt > secondAt ? a : b;
    const earlier = firstAt > secondAt ? b : a;
    const laterId = firstAt > secondAt ? entry.first : entry.second;
    const earlierId = firstAt > secondAt ? entry.second : entry.first;
    await dragBefore(later, earlier);
    await expect.poll(async () => (await saved(page, entry.kind))?.indexOf(laterId)).toBeLessThan(Math.max(firstAt, secondAt));
    const after = await ids(rows, "data-row-id");
    expect(after.indexOf(laterId)).toBeLessThan(after.indexOf(earlierId));
    await expect(page.getByRole("dialog")).toHaveCount(0);
    if (entry.kind === "bookmarks") expect(await page.evaluate(() => JSON.parse(localStorage.getItem("shardx.bookmarks.layout") ?? "{}").sort)).toBeNull();
    await reloadApp(page);
    await expect.poll(() => ids(rows, "data-row-id")).toEqual(after);
    await later.focus();
    await page.keyboard.press("Shift+F10");
    await page.getByRole("menuitem", { name: "Move down", exact: true }).click();
    const restored = await ids(rows, "data-row-id");
    expect(restored.indexOf(laterId)).toBeGreaterThan(restored.indexOf(earlierId));
  });
}

test("proxy multi-selection moves as one block and an action-button drag does not reorder", async ({ page }) => {
  await openApp(page, { section: "proxies" });
  const row = (id: string) => page.locator(`[data-proxy-id='${id}'][role='row']`);
  await row("px-us-2").hover();
  await row("px-us-2").getByRole("checkbox").check();
  await row("px-jp-1").getByRole("checkbox").check();
  await dragBefore(row("px-jp-1"), row("px-us-1"));
  expect((await saved(page, "proxies"))?.slice(0, 2)).toEqual(["px-us-2", "px-jp-1"]);
  await expect(row("px-us-2").getByRole("checkbox")).toBeChecked();
  await expect(row("px-jp-1").getByRole("checkbox")).toBeChecked();
  const before = await saved(page, "proxies");
  await row("px-us-2").getByRole("button", { name: "More actions", exact: true }).dragTo(row("px-de-1"));
  expect(await saved(page, "proxies")).toEqual(before);
});

test("manual preferences remain isolated between personal and team spaces", async ({ page }) => {
  await openApp(page, { section: "proxies" });
  await appModule(page, "/src/shared/model/listOrder.ts", "orderModel");
  await page.evaluate(() => {
    const model = (window as unknown as { orderModel: { useListOrderStore: { getState: () => { setScope: (s: string) => void; save: (kind: string, ids: string[]) => void } } } }).orderModel;
    const store = model.useListOrderStore.getState();
    store.save("proxies", ["px-jp-1", "px-us-1"]);
    store.setScope("team:synthetic");
    store.save("proxies", ["px-us-1", "px-jp-1"]);
    store.setScope("personal");
  });
  expect(await saved(page, "proxies")).toEqual(["px-jp-1", "px-us-1"]);
  expect(await saved(page, "proxies", "team:synthetic")).toEqual(["px-us-1", "px-jp-1"]);
});

test("fingerprint platform groups and pinned profile groups reject cross-group drops", async ({ page }) => {
  await openApp(page, { section: "fingerprints" });
  const mac = page.locator("main [data-row-id='mac-m3-pro']");
  const win = page.locator("main [data-row-id='win11-rtx4070']");
  await expect(mac).toBeVisible();
  await expect(win).toBeVisible();
  await dragBefore(win, mac);
  expect(await saved(page, "fingerprints")).toBeUndefined();
  await page.locator("aside nav").getByRole("button", { name: /^All profiles/ }).click();
  const rows = page.locator("main [data-profile-id][role='row']");
  await expect(rows).toHaveCount(8);
  await dragBefore(rows.filter({ hasText: "Etsy craft shop" }), rows.filter({ hasText: "Amazon US seller" }));
  expect(await saved(page, "profiles")).toBeUndefined();
});

test("Display can return to a column order and back to the saved manual order", async ({ page }) => {
  await openApp(page, { section: "proxies" });
  const rows = page.locator("main [data-proxy-id][role='row']");
  await dragBefore(rows.filter({ hasText: "DC Tokyo" }), rows.filter({ hasText: "DC New York" }));
  const manual = await ids(rows, "data-proxy-id");
  await page.getByRole("button", { name: "Display", exact: true }).click();
  await page.getByRole("dialog", { name: "Display" }).getByRole("combobox", { name: "Order" }).click();
  await page.getByRole("option", { name: "Name", exact: true }).click();
  expect(await ids(rows, "data-proxy-id")).not.toEqual(manual);
  await page.getByRole("dialog", { name: "Display" }).getByRole("combobox", { name: "Order" }).click();
  await page.getByRole("option", { name: "Manual (drag to reorder)", exact: true }).click();
  expect(await ids(rows, "data-proxy-id")).toEqual(manual);
});

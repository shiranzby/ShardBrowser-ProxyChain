import { expect, test, type Page } from "@playwright/test";
import { openApp } from "./support/app";

// EMIL-19 and the 6.2 default: ask after the 10th successful Start on 2 or more days.
const row = (page: Page, name: string) => page.locator("[role='row'][data-profile-id]", { hasText: name });
const seed = (page: Page, saved: object | null) => page.addInitScript((s) => {
  // openApp sets the old key to "done" only when it is missing; any other
  // value lets the new counting run.
  localStorage.setItem("shardx-star-prompt", "counting");
  if (s) localStorage.setItem("shardx.starAsk", JSON.stringify(s));
}, saved);
const start = async (page: Page, name: string) => {
  await row(page, name).hover();
  await row(page, name).getByRole("button", { name: "Start" }).click();
  await expect(row(page, name).getByRole("button", { name: "Stop" })).toBeVisible();
  await page.waitForTimeout(1200); // the mock's launch takes 900 ms; a start counts once confirmed
};
const modal = (page: Page) => page.getByRole("dialog", { name: /Enjoying ShyShard/ });

test("a fresh install never sees the ask (?mock=empty)", async ({ page }) => {
  await seed(page, null);
  await page.addInitScript(() => Object.assign(window, { __shardxMockEmpty: true }));
  await openApp(page);
  await page.waitForTimeout(1500);
  await expect(modal(page)).toHaveCount(0);
});

test("the 10th confirmed start on a second day asks", async ({ page }) => {
  await seed(page, { v: 1, starts: 9, days: ["2020-01-01"], state: "counting" });
  await openApp(page);
  await expect(modal(page)).toHaveCount(0);
  await start(page, "Etsy craft shop");
  await expect(modal(page)).toBeVisible();
});

test("ten starts on one day do not ask", async ({ page }) => {
  await openApp(page);
  const today = await page.evaluate(() => { const d = new Date(); return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`; });
  await seed(page, { v: 1, starts: 11, days: [today], state: "counting" });
  await openApp(page);
  await start(page, "Etsy craft shop");
  await expect(modal(page)).toHaveCount(0);
});

test("a failed start does not count", async ({ page }) => {
  await seed(page, { v: 1, starts: 9, days: ["2020-01-01"], state: "counting" });
  await page.addInitScript(() => Object.assign(window, { __shardxMockLaunchFails: ["aed41005"] }));
  await openApp(page);
  await row(page, "Etsy craft shop").hover();
  await row(page, "Etsy craft shop").getByRole("button", { name: "Start" }).click();
  await expect(page.locator("[data-toast]").filter({ hasText: /could not start/i })).toBeVisible();
  await expect(modal(page)).toHaveCount(0);
});

test("Maybe later means later, not never", async ({ page }) => {
  await seed(page, { v: 1, starts: 9, days: ["2020-01-01"], state: "counting" });
  await openApp(page);
  await start(page, "Etsy craft shop");
  await modal(page).getByRole("button", { name: "Maybe later" }).click();
  // The page's clock is fixed (openApp): "later" is measured on it.
  const { saved, now } = await page.evaluate(() => ({ saved: JSON.parse(localStorage.getItem("shardx.starAsk")!), now: Date.now() }));
  expect(saved.state).toBe("counting");
  expect(saved.laterUntil - now).toBeGreaterThan(29 * 86_400_000);
});

// MP4 review, M13: a due ask never opens over another dialog; it waits.
test("the ask waits until no other dialog is open", async ({ page }) => {
  await seed(page, { v: 1, starts: 9, days: ["2020-01-01"], state: "counting" });
  await openApp(page);
  await row(page, "Etsy craft shop").hover();
  await row(page, "Etsy craft shop").getByRole("button", { name: "Start" }).click();
  await page.locator("aside").getByRole("button", { name: "New folder" }).click();
  const other = page.getByRole("dialog", { name: /New folder/ });
  await expect(other).toBeVisible();
  await page.waitForTimeout(1500); // the start is confirmed meanwhile
  await expect(modal(page)).toHaveCount(0);
  await page.keyboard.press("Escape");
  await expect(other).toHaveCount(0);
  await expect(modal(page)).toBeVisible();
});

import { expect, test } from "@playwright/test";
import { openApp, reloadApp } from "./support/app";

const KEY = "shardx.simpleNavigation";
const advanced = ["extensions", "bookmarks", "automation", "proxyshard"];

test("simple navigation waits out an in-flight save before accepting the next choice", async ({ page }) => {
  await page.addInitScript(() => Object.assign(window, { __shardxMockSlow: { settings_save: 2500 } }));
  await openApp(page, { section: "settings" });
  await page.getByRole("switch", { name: /tray/i }).click();
  await page.getByRole("tab", { name: "Appearance", exact: true }).click();
  const choice = page.getByRole("switch", { name: "Simple navigation", exact: true });
  await choice.check();
  const bar = page.locator("[data-save-bar]");
  await bar.getByRole("button", { name: "Save", exact: true }).click();
  await expect(choice).toBeDisabled();
  await page.locator("[data-setting='simple-navigation'] [data-setting-help]").click();
  await expect(choice).toBeChecked();
  await expect(bar).toHaveCount(0);
  await expect(choice).toBeEnabled();
  await choice.uncheck();
  await expect(bar).toBeVisible();
  await bar.getByRole("button", { name: "Save", exact: true }).click();
  await expect(bar).toHaveCount(0);
  expect(await page.evaluate(key => localStorage.getItem(key), KEY)).toBe("false");
  await expect(page.locator("[data-nav='more']")).toHaveCount(0);
});

test("a combined failed save keeps simple navigation unchanged until retry succeeds", async ({ page }) => {
  await page.addInitScript(() => Object.assign(window, { __shardxMockFail: { settings_save: 1 } }));
  await openApp(page, { section: "settings" });
  const tray = page.getByRole("switch", { name: /tray/i });
  await tray.click();
  await page.getByRole("tab", { name: "Appearance", exact: true }).click();
  await page.getByRole("switch", { name: "Simple navigation", exact: true }).check();
  const bar = page.locator("[data-save-bar]");
  await bar.getByRole("button", { name: "Save", exact: true }).click();
  await expect(bar.locator("[data-save-bar-note]")).toBeVisible();
  await expect(page.locator("[data-nav='more']")).toHaveCount(0);
  expect(await page.evaluate(key => localStorage.getItem(key), KEY)).toBeNull();
  await expect(page.getByRole("switch", { name: "Simple navigation", exact: true })).toBeChecked();
  await bar.getByRole("button", { name: "Save", exact: true }).click();
  await expect(bar).toHaveCount(0);
  await expect(page.locator("[data-nav='more']")).toBeVisible();
  expect(await page.evaluate(key => localStorage.getItem(key), KEY)).toBe("true");
});

test("simple navigation waits for Save, cancels without writes and survives reload", async ({ page }) => {
  await openApp(page, { section: "settings", theme: "light" });
  const data = () => page.evaluate(() => {
    const s = (window as any).__shardxMock.state;
    return JSON.stringify({ profiles: s.profiles, proxies: s.proxies, settings: s.settings });
  });
  const before = await data();
  await page.getByRole("tab", { name: "Appearance", exact: true }).click();
  const choice = page.getByRole("switch", { name: "Simple navigation", exact: true });
  await choice.check();
  await expect(page.locator("[data-nav='more']")).toHaveCount(0);
  await page.locator("[data-save-bar]").getByRole("button", { name: "Don’t save", exact: true }).click();
  await expect(choice).not.toBeChecked();
  expect(await page.evaluate(key => localStorage.getItem(key), KEY)).toBeNull();
  expect(await data()).toBe(before);
  await choice.check();
  await page.locator("[data-save-bar]").getByRole("button", { name: "Save", exact: true }).click();
  await expect(page.locator("[data-save-bar]")).toHaveCount(0);
  await expect(page.locator("[data-nav='more']")).toHaveAttribute("aria-expanded", "false");
  for (const id of advanced) await expect(page.locator(`[data-nav='${id}']`)).toHaveCount(0);
  expect(await data()).toBe(before);
  await reloadApp(page);
  await expect(page.locator("[data-nav='more']")).toBeVisible();
  await page.getByRole("tab", { name: "Appearance", exact: true }).click();
  await expect(choice).toBeChecked();
  await choice.uncheck();
  await page.locator("[data-save-bar]").getByRole("button", { name: "Save", exact: true }).click();
  await expect(page.locator("[data-nav='more']")).toHaveCount(0);
  for (const id of advanced) await expect(page.locator(`[data-nav='${id}']`)).toBeVisible();
});

test("More works by keyboard, retains the selected page and preserves extension update notices", async ({ page }) => {
  await page.addInitScript(key => localStorage.setItem(key, "true"), KEY);
  await page.addInitScript(() => Object.assign(window, { __shardxMockExtUpdates: {
    "cookie-editor": { state: "available", latest: "1.14.0", size: 700000, first_seen: 1 },
    ublock: { state: "current" },
  } }));
  await openApp(page, { theme: "light" });
  const more = page.locator("[data-nav='more']");
  await expect(more.locator("[data-update-count]")).toHaveText("1");
  await expect(more).toHaveAccessibleName("More 1 update available");
  await page.keyboard.press("ControlOrMeta+\\");
  await expect(page.locator("[data-region='sidebar']")).toHaveAttribute("data-rail", "true");
  await expect(more.locator("[data-update-count]")).toHaveText("1");
  await expect(more).toHaveAccessibleName("More: 1 update available");
  await page.keyboard.press("ControlOrMeta+\\");
  await more.focus();
  await page.keyboard.press("Enter");
  await expect(more).toHaveAttribute("aria-expanded", "true");
  for (const id of advanced) await expect(page.locator(`[data-nav='${id}']`)).toBeVisible();
  await page.locator("[data-nav='bookmarks']").click();
  await expect(page.locator("main")).toHaveAttribute("data-section", "bookmarks");
  await more.focus();
  await page.keyboard.press("Space");
  await expect(more).toHaveAttribute("aria-expanded", "false");
  await expect(more).toBeFocused();
  await expect(page.locator("[data-nav='bookmarks']")).toHaveAttribute("aria-current", "page");
  await expect(page.locator("[data-nav='extensions']")).toHaveCount(0);
  // The existing update count moves to More while its destination is folded.
  await more.click();
  await expect(page.locator("[data-nav='extensions'] [data-update-count]")).toHaveText("1");
  await more.click();
  await expect(more.locator("[data-update-count]")).toHaveText("1");
  await page.locator("[data-nav='proxies']").click();
  await expect(page.locator("[data-nav='bookmarks']")).toHaveCount(0);
  // Command navigation is still offered even though the destination is folded.
  await page.keyboard.press("ControlOrMeta+k");
  await page.getByRole("combobox", { name: "Search commands" }).fill("bookmarks");
  await page.getByRole("option", { name: /^Bookmarks/ }).click();
  await expect(page.locator("main")).toHaveAttribute("data-section", "bookmarks");
  await expect(page.locator("[data-nav='bookmarks']")).toHaveAttribute("aria-current", "page");
});

for (const section of advanced) {
  test(`simple navigation keeps the ${section} deep link visible`, async ({ page }) => {
    await page.addInitScript(key => localStorage.setItem(key, "true"), KEY);
    await openApp(page, { section });
    await expect(page.locator("main")).toHaveAttribute("data-section", section);
    await expect(page.locator(`[data-nav='${section}']`)).toHaveAttribute("aria-current", "page");
    await expect(page.locator("[data-nav='more']")).toHaveAttribute("aria-expanded", "false");
  });
}

for (const language of ["zh", "ru"]) {
  test(`simple navigation and rail remain readable in ${language} at minimum window`, async ({ page }, info) => {
    await page.setViewportSize({ width: 1024, height: 680 });
    await page.addInitScript(({ key, lang }) => {
      localStorage.setItem(key, "true"); localStorage.setItem("shardx.lang", lang);
    }, { key: KEY, lang: language });
    await openApp(page, { section: "proxies", theme: "light" });
    await expect(page.locator("[data-nav='more']")).toBeVisible();
    await page.screenshot({ path: info.outputPath(`simple-${language}.png`), animations: "disabled" });
    await page.locator("[data-nav='more']").click();
    for (const id of advanced) await expect(page.locator(`[data-nav='${id}']`)).toBeInViewport();
    expect(await page.evaluate(() => document.documentElement.scrollWidth - innerWidth)).toBeLessThanOrEqual(1);
    await page.keyboard.press("ControlOrMeta+\\");
    await expect(page.locator("[data-region='sidebar']")).toHaveAttribute("data-rail", "true");
    await expect(page.locator("[data-nav='more']")).toBeVisible();
    await page.locator("[data-nav='more']").click();
    for (const id of advanced) await expect(page.locator(`[data-nav='${id}']`)).toHaveCount(0);
    await page.screenshot({ path: info.outputPath(`simple-rail-${language}.png`), animations: "disabled" });
  });
}

test("unknown stored simple navigation value keeps the complete navigation", async ({ page }) => {
  await page.addInitScript(key => localStorage.setItem(key, "future"), KEY);
  await openApp(page);
  await expect(page.locator("[data-nav='more']")).toHaveCount(0);
  for (const id of advanced) await expect(page.locator(`[data-nav='${id}']`)).toBeVisible();
});

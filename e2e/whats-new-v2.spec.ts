import { expect, test, type Page } from "@playwright/test";
import { openApp } from "./support/app";

// CRAFT-28: titles first; a body opens on demand.
const open = async (page: Page, lang: "en" | "zh" = "en") => {
  await page.addInitScript((l) => { localStorage.setItem("shardx.lang", l); localStorage.setItem("shardx-patchlog-seen-version", "0.0.1"); }, lang);
  await openApp(page);
  const dialog = page.getByRole("dialog").first();
  await expect(dialog).toBeVisible();
  return dialog;
};

test("every entry opens collapsed, as a row with its lead", async ({ page }) => {
  const dialog = await open(page);
  const entries = dialog.locator("[data-entry]");
  expect(await entries.count()).toBeGreaterThan(2);
  await expect(dialog.locator("[data-entry-body]")).toHaveCount(0);
  for (const b of await dialog.locator("[data-entry] button[aria-expanded]").all()) await expect(b).toHaveAttribute("aria-expanded", "false");
});

test("an entry opens and closes from the keyboard", async ({ page }) => {
  const dialog = await open(page);
  const first = dialog.locator("[data-entry] button[aria-expanded]").first();
  await first.focus();
  await page.keyboard.press("Enter");
  await expect(first).toHaveAttribute("aria-expanded", "true");
  await expect(dialog.locator("[data-entry-body]")).toHaveCount(1);
  await page.keyboard.press("Space");
  await expect(dialog.locator("[data-entry-body]")).toHaveCount(0);
});

test("first focus is the panel, not the release picker (plan ruling 23)", async ({ page }) => {
  const dialog = await open(page);
  await expect(dialog.locator("[data-whats-new-panel]")).toBeFocused();
});

test("entries are grouped by what changed, with counts", async ({ page }) => {
  const dialog = await open(page);
  await expect(dialog.locator("[data-scope-group='launcher'] h3")).toHaveText(/^Launcher\s*\d+$/);
  await expect(dialog.locator("[data-scope-group='browser'] h3")).toHaveCount(0);
});

test("no synthesized italics; badges are neutral (TYPE-11, COLOR-10)", async ({ page }) => {
  const dialog = await open(page);
  for (const b of await dialog.locator("[data-entry] button[aria-expanded]").all()) await b.click();
  await expect(dialog.locator("em, i")).toHaveCount(0);
  const colors = await dialog.locator("[data-entry] [data-badge]").evaluateAll((els) => [...new Set(els.map((e) => e.getAttribute("data-color")))]);
  expect(colors.every((c) => c === "gray" || c === "warning")).toBe(true);
});

test("Chinese notes can switch to English and back (WRITE-27)", async ({ page }) => {
  const dialog = await open(page, "zh");
  const entry = dialog.locator('[data-entry="pc17-proxy-chain"]');
  await entry.getByRole("button", { name: /代理链控制/ }).click();
  await expect(entry.getByText(/启动器把系统代理/)).toBeVisible();
  await expect(dialog.getByRole("button", { name: "English", exact: true })).toBeVisible();
  await dialog.getByRole("button", { name: "English", exact: true }).click();
  await expect(dialog.getByText("Proxy chain controls", { exact: true })).toBeVisible();
  await expect(entry.getByText(/The launcher keeps system/)).toBeVisible();
  await expect(dialog.getByRole("button", { name: "中文", exact: true })).toBeVisible();
  await dialog.getByRole("button", { name: "中文", exact: true }).click();
  await expect(dialog.getByText("代理链控制", { exact: true })).toBeVisible();
  await dialog.getByRole("combobox").click();
  await page.getByRole("option", { name: /^v2\.1\.3 / }).click();
  const passwords = dialog.locator('[data-entry="profile-password"]');
  await passwords.getByRole("button", { name: /配置密码/ }).click();
  await expect(passwords.getByText(/每次启动时都要输入密码/)).toBeVisible();
  await dialog.getByRole("button", { name: "English", exact: true }).click();
  await expect(passwords.getByText(/It is asked at every start/)).toBeVisible();
});

test("historical release details render in the selected language", async ({ page }) => {
  const dialog = await open(page, "zh");
  await dialog.getByRole("combobox").click();
  await page.getByRole("option", { name: /^v2\.0\.1 / }).click();
  const fixes = dialog.locator('[data-entry="fixes"]');
  await fixes.getByRole("button", { name: /修复/ }).click();
  await expect(fixes.getByText(/代理页新增的代理立即出现在配置编辑器中/)).toBeVisible();

  await dialog.getByRole("combobox").click();
  await page.getByRole("option", { name: /^v2\.0\.2 / }).click();
  const requests = dialog.locator('[data-entry="automation-requests-and-drag"]');
  await requests.getByRole("button", { name: /自动化请求与拖动/ }).click();
  await expect(requests.getByText(/等待元素消失/)).toBeVisible();
  await dialog.getByRole("button", { name: "English", exact: true }).click();
  await expect(requests.getByText(/until an element is gone/)).toBeVisible();
  await expect(dialog.getByRole("combobox")).toHaveText(/v2\.0\.2/);

  await dialog.getByRole("button", { name: "中文", exact: true }).click();
  await dialog.getByRole("combobox").click();
  await page.getByRole("option", { name: /^v2\.0\.3 / }).click();
  const settings = dialog.locator('[data-entry="settings-file-kept"]');
  await settings.getByRole("button", { name: /无法读取时保留设置文件/ }).click();
  await expect(settings.getByText(/保留原文件和 API 密钥/)).toBeVisible();

  await dialog.getByRole("combobox").click();
  await page.getByRole("option", { name: /^v2\.1\.0 / }).click();
  const proxies = dialog.locator('[data-entry="proxy-password-sessions"]');
  await proxies.getByRole("button", { name: /仅密码不同的代理/ }).click();
  await expect(proxies.getByText(/只跳过完全相同的代理/)).toBeVisible();
});

// Live layer 6.3: a body's height moves from what shows now, so a second
// press mid-way turns it around; under Reduce motion it opens at once.
// The suite runs with Reduce motion on (playwright.config.ts); this one moves.
test.describe("with motion", () => {
  test.use({ reducedMotion: "no-preference" });
  test("a body opens from its current height and turns around mid-way", async ({ page }) => {
    const dialog = await open(page);
    // Freeze the animation timeline and seek to an intermediate frame.
    const cdp = await page.context().newCDPSession(page);
    await cdp.send("Animation.enable");
    await cdp.send("Animation.setPlaybackRate", { playbackRate: 0 });
    const first = dialog.locator("[data-entry] button[aria-expanded]").first();
    await first.click();
    const body = dialog.locator("[data-entry-body]");
    const mid = await body.evaluate((el) => {
      const animation = el.getAnimations()[0];
      animation.currentTime = Number(animation.effect!.getTiming().duration) / 4;
      return { h: el.offsetHeight, full: el.scrollHeight };
    });
    expect(mid.h).toBeGreaterThan(0);
    expect(mid.h).toBeLessThan(mid.full);
    await first.click();
    // It closes from where it was, not from its full height.
    expect(await body.evaluate((el) => el.offsetHeight)).toBeLessThan(mid.full);
    await cdp.send("Animation.setPlaybackRate", { playbackRate: 1 });
    await expect(body).toHaveCount(0);
  });
});

test("under Reduce motion a body opens and closes at once", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  const dialog = await open(page);
  const first = dialog.locator("[data-entry] button[aria-expanded]").first();
  await first.click();
  const body = dialog.locator("[data-entry-body]");
  const shown = await body.evaluate((el) => ({ moving: el.getAnimations().length, h: el.offsetHeight, full: el.scrollHeight }));
  expect(shown.moving).toBe(0);
  expect(shown.h).toBe(shown.full);
  await first.click();
  await expect(body).toHaveCount(0);
});

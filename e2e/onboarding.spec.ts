import { expect, test, type Page } from "@playwright/test";
import { openApp, pressCreate, waitForIdle } from "./support/app";

const empty = (page: Page) => page.addInitScript(() => { (window as unknown as { __shardxMockEmpty: boolean }).__shardxMockEmpty = true; });
const checklist = (page: Page) => page.getByRole("region", { name: "Get started" });
const step = (page: Page, name: string) => checklist(page).locator("[data-step]", { hasText: name });
const heading = (page: Page) => page.getByRole("heading", { level: 1 });
const rows = (page: Page) => page.locator("[role='row'][data-profile-id]");

test("a fresh install shows the checklist instead of an empty list", async ({ page }) => {
  await empty(page);
  await openApp(page);
  await expect(checklist(page)).toBeVisible();
  await expect(checklist(page).locator("[data-step]")).toHaveCount(3);
  await expect(checklist(page).locator("[data-step][data-done]")).toHaveCount(0);
  await expect(page.getByText("No profiles yet")).toHaveCount(0);
});

test("the checklist sits in the middle of the page's content area, up and down as well as across (owner, 2026-10-02)", async ({ page }) => {
  await empty(page);
  await openApp(page);
  await expect(checklist(page)).toBeVisible();
  const { card, area } = await checklist(page).evaluate((el) => {
    const r = (e: Element) => e.getBoundingClientRect();
    return { card: r(el), area: r(el.parentElement!) };
  });
  expect(Math.abs(card.y + card.height / 2 - (area.y + area.height / 2))).toBeLessThanOrEqual(2);
  expect(Math.abs(card.x + card.width / 2 - (area.x + area.width / 2))).toBeLessThanOrEqual(2);
  // The area is the page body under the header, not a box the card's own size makes.
  expect(area.height).toBeGreaterThan(card.height + 200);
});

test("the steps: add a proxy, create a profile that uses it, start it; then it hides for good", async ({ page }) => {
  await empty(page);
  await openApp(page);
  await step(page, "Connect through a proxy").getByRole("button", { name: "Add a proxy" }).click();
  await expect(heading(page)).toHaveText("Proxies");
  await page.getByRole("textbox", { name: "Paste one proxy per line" }).fill("socks5://1.2.3.4:1080");
  // One line goes through the preview too, where it can be tested first.
  await page.getByRole("button", { name: "Preview", exact: true }).click();
  await page.getByRole("button", { name: "Add 1 proxy", exact: true }).click();
  // Back on the checklist by itself, with the keyboard on the next step (IFACE-02).
  await expect(heading(page)).toHaveText("All profiles");
  await expect(step(page, "Connect through a proxy")).toHaveAttribute("data-done", "");
  const create = step(page, "Create a profile");
  await expect(create).toContainText("Uses the proxy you added: 1.2.3.4:1080.");
  await expect(create.getByRole("button", { name: "New profile" })).toBeFocused();
  await create.getByRole("button", { name: "New profile" }).click();
  // The form starts on the proxy the first step added; Create profile makes it.
  await pressCreate(page);
  await expect(page.locator("aside[data-peek]")).toBeVisible();
  await expect(rows(page)).toHaveCount(1);
  type S = { order: string[]; profiles: Record<string, { _meta: { proxy_id: string | null } }> };
  await waitForIdle(page);
  const bound = await page.evaluate(() => {
    const s = (window as unknown as { __shardxMock: { state: S } }).__shardxMock.state;
    return s.profiles[s.order[0]]._meta.proxy_id;
  });
  expect(bound).not.toBeNull();
  await step(page, "Start the profile").getByRole("button", { name: /^Start/ }).click();
  await expect(checklist(page)).toHaveCount(0);
  const launches = await page.evaluate(() => (window as unknown as { __shardxMockLaunches: { proxy_id: string | null }[] }).__shardxMockLaunches);
  expect(launches[0].proxy_id).toBe(bound);
  await page.reload();
  await expect(rows(page)).toHaveCount(1);
  await expect(checklist(page)).toHaveCount(0);
});

// Pair 2 review, Minor 1: the proxy added from the checklist was opened in
// Peek on the way back to Profiles, so Proxies opened with it later.
test("the proxy added from the checklist is not left open in Peek", async ({ page }) => {
  await empty(page);
  await openApp(page);
  await step(page, "Connect through a proxy").getByRole("button", { name: "Add a proxy" }).click();
  await page.getByRole("textbox", { name: "Paste one proxy per line" }).fill("socks5://1.2.3.4:1080");
  await page.getByRole("button", { name: "Preview", exact: true }).click();
  await page.getByRole("button", { name: "Add 1 proxy", exact: true }).click();
  await expect(heading(page)).toHaveText("All profiles");
  await page.locator("aside [data-nav='proxies']").click();
  await expect(heading(page)).toHaveText("Proxies");
  await expect(page.locator("[role='row'][data-proxy-id]")).toHaveCount(1);
  await expect(page.locator("aside[data-peek='proxy']")).toHaveCount(0);
});

// Final review I1: on a first install the checklist's proxy round trip met the
// Proxies page tip. It started as the importer closed (the Profiles reload was
// still on its way) and rode the jump back, a card over Profiles whose Next
// drove the person to Proxies again. `openApp`'s default has the tips off, so
// only a fresh install shows it.
test("a fresh install: the checklist's proxy round trip leaves no tip over Profiles, and Proxies still plays its tip later", async ({ page }) => {
  await empty(page);
  await openApp(page, { onboarding: "fresh" });
  const welcome = page.getByRole("dialog", { name: "Welcome to ShyShard" });
  await expect(welcome).toBeVisible();
  await welcome.getByRole("button", { name: "Skip", exact: true }).click();
  await expect(welcome).toHaveCount(0);
  await step(page, "Connect through a proxy").getByRole("button", { name: "Add a proxy" }).click();
  await expect(heading(page)).toHaveText("Proxies");
  await page.getByRole("textbox", { name: "Paste one proxy per line" }).fill("socks5://1.2.3.4:1080");
  await page.getByRole("button", { name: "Preview", exact: true }).click();
  await page.getByRole("button", { name: "Add 1 proxy", exact: true }).click();
  await expect(heading(page)).toHaveText("All profiles");
  await expect(step(page, "Connect through a proxy")).toHaveAttribute("data-done", "");
  // No tour card, over Profiles or anywhere, once the tip's window has passed.
  await page.waitForTimeout(800);
  await expect(page.locator("[data-tour-layer]")).toHaveCount(0);
  await expect(page.locator("[data-tour-card]")).toHaveCount(0);
  await expect(heading(page)).toHaveText("All profiles");
  // The tip that was cut short was not read: the page is still unseen, and its next visit plays it.
  expect((await page.evaluate(() => JSON.parse(localStorage.getItem("shardx.onboarding")!))).pagesSeen).toEqual([]);
  await page.locator("aside [data-nav='proxies']").click();
  const card = page.locator("[data-tour-card]");
  await expect(card).toHaveAttribute("data-step", "proxiesAdd");
  await expect(card).toContainText("1 of 2");
});

test("Skip marks the proxy step done; Hide puts the card away for good", async ({ page }) => {
  await empty(page);
  await openApp(page);
  await step(page, "Connect through a proxy").getByRole("button", { name: "Skip" }).click();
  await expect(step(page, "Connect through a proxy")).toHaveAttribute("data-done", "");
  await step(page, "Create a profile").getByRole("button", { name: "New profile" }).click();
  await pressCreate(page);
  await expect(rows(page)).toHaveCount(1);
  await checklist(page).getByRole("button", { name: "Hide" }).click();
  await expect(checklist(page)).toHaveCount(0);
  await page.reload();
  await expect(checklist(page)).toHaveCount(0);
});

test("an existing install never sees the checklist", async ({ page }) => {
  await openApp(page);
  await expect(rows(page)).toHaveCount(8);
  await expect(checklist(page)).toHaveCount(0);
  expect(await page.evaluate(() => JSON.parse(localStorage.getItem("shardx.onboarding")!).state)).toBe("done");
});

test("emptying the list later shows the plain empty state, not the checklist", async ({ page }) => {
  await empty(page);
  await openApp(page);
  await step(page, "Connect through a proxy").getByRole("button", { name: "Skip" }).click();
  await step(page, "Create a profile").getByRole("button", { name: "New profile" }).click();
  await pressCreate(page);
  await step(page, "Start the profile").getByRole("button", { name: /^Start/ }).click();
  await expect(checklist(page)).toHaveCount(0);
  await rows(page).first().click({ button: "right" });
  await page.getByRole("menuitem", { name: "Move to Trash" }).click();
  await expect(rows(page)).toHaveCount(0);
  await expect(page.getByText("No profiles yet")).toBeVisible();
  await expect(checklist(page)).toHaveCount(0);
});

test("steps tick from any path: an imported profile, a start from the palette", async ({ page }) => {
  await empty(page);
  await openApp(page);
  await page.evaluate(() => { (window as unknown as { __shardxMockClipboard: string }).__shardxMockClipboard = JSON.stringify([{ name: "Imported one" }]); });
  await page.keyboard.press("ControlOrMeta+k");
  await page.getByRole("combobox", { name: "Search commands" }).fill("import");
  await page.getByRole("option", { name: /^Import profiles/ }).click();
  await expect(step(page, "Create a profile")).toHaveAttribute("data-done", "");
  await page.keyboard.press("ControlOrMeta+k");
  await page.getByRole("combobox", { name: "Search commands" }).fill("imported");
  await page.keyboard.press("Enter");
  await expect(step(page, "Start the profile")).toHaveAttribute("data-done", "");
});

test("a broken stored value decides again from the list", async ({ page }) => {
  await empty(page);
  await page.addInitScript(() => localStorage.setItem("shardx.onboarding", "{nope"));
  await openApp(page);
  await expect(checklist(page)).toBeVisible();
});

test("in Chinese", async ({ page }) => {
  await empty(page);
  await page.addInitScript(() => localStorage.setItem("shardx.lang", "zh"));
  await openApp(page);
  await expect(page.getByRole("region", { name: "开始使用" })).toContainText("添加代理");
});

test("a start that fails doesn't tick the start step; the next one that works does", async ({ page }) => {
  await empty(page);
  await openApp(page);
  await step(page, "Connect through a proxy").getByRole("button", { name: "Skip" }).click();
  await step(page, "Create a profile").getByRole("button", { name: "New profile" }).click();
  await pressCreate(page);
  await expect(rows(page)).toHaveCount(1);
  await page.evaluate(() => { (window as unknown as { __shardxMockFail: Record<string, number> }).__shardxMockFail = { launch: 1 }; });
  await step(page, "Start the profile").getByRole("button", { name: /^Start/ }).click();
  await expect(page.locator("[data-toast]").filter({ hasText: "Mock failure: launch" })).toBeVisible();
  await expect(checklist(page)).toBeVisible();
  await expect(step(page, "Start the profile")).not.toHaveAttribute("data-done", "");
  await step(page, "Start the profile").getByRole("button", { name: /^Start/ }).click();
  await expect(checklist(page)).toHaveCount(0);
});

test("a skipped proxy step stays skipped after a reload", async ({ page }) => {
  await empty(page);
  await openApp(page);
  await step(page, "Connect through a proxy").getByRole("button", { name: "Skip" }).click();
  await expect(step(page, "Connect through a proxy")).toHaveAttribute("data-done", "");
  await page.reload();
  await expect(step(page, "Connect through a proxy")).toHaveAttribute("data-done", "");
});

test("dev mock: ?mock=empty opens as a fresh install", async ({ page }) => {
  await page.addInitScript(() => { if (localStorage.getItem("shardx.lang") === null) localStorage.setItem("shardx.lang", "en"); });
  await page.goto("/?mock=empty");
  await expect(checklist(page)).toBeVisible();
  await expect(page).not.toHaveURL(/mock=/);
});

test("the current step's button is the filled one, all one size (CRAFT-20)", async ({ page }) => {
  await empty(page);
  await openApp(page);
  const proxyAction = step(page, "Connect through a proxy").getByRole("button", { name: "Add a proxy" });
  const profileAction = step(page, "Create a profile").getByRole("button", { name: "New profile" });
  await expect(proxyAction).toHaveAttribute("data-mode", "filled");
  await expect(profileAction).toHaveAttribute("data-mode", "stroke");
  expect((await proxyAction.boundingBox())!.height).toBe((await profileAction.boundingBox())!.height);
});

test("Start names the profile it starts (PC-116)", async ({ page }) => {
  await empty(page);
  await openApp(page);
  await step(page, "Connect through a proxy").getByRole("button", { name: "Skip" }).click();
  await step(page, "Create a profile").getByRole("button", { name: "New profile" }).click();
  await pressCreate(page);
  await expect(step(page, "Start the profile").getByRole("button", { name: /^Start “.+”$/ })).toBeVisible();
});

test("an install whose profiles are all in the Trash is not fresh (PC-115)", async ({ page }) => {
  await openApp(page);
  await page.evaluate(async () => {
    const w = window as unknown as {
      __shardxMock: { state: { profiles: Record<string, unknown> } };
      __TAURI_INTERNALS__: { invoke: (cmd: string, args: unknown) => Promise<unknown> };
    };
    for (const id of Object.keys(w.__shardxMock.state.profiles)) await w.__TAURI_INTERNALS__.invoke("profile_delete", { id });
  });
  await page.evaluate(() => localStorage.removeItem("shardx.onboarding"));
  await page.reload();
  // Decided once the list is in: the plain empty state, not the checklist.
  await expect(page.getByText("No profiles yet")).toBeVisible();
  await expect(checklist(page)).toHaveCount(0);
});

test("a fresh install never shows a frame of the empty list first (PC-118)", async ({ page }) => {
  await empty(page);
  await page.addInitScript(() => {
    (window as unknown as { __sawEmpty: boolean }).__sawEmpty = false;
    new MutationObserver(() => { if (document.body?.textContent?.includes("No profiles yet")) (window as unknown as { __sawEmpty: boolean }).__sawEmpty = true; })
      .observe(document, { subtree: true, childList: true, characterData: true });
  });
  await openApp(page);
  await expect(checklist(page)).toBeVisible();
  expect(await page.evaluate(() => (window as unknown as { __sawEmpty: boolean }).__sawEmpty)).toBe(false);
});

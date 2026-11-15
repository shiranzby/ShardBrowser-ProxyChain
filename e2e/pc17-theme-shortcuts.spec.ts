import { expect, test, type Page } from "@playwright/test";
import { openApp, reloadApp } from "./support/app";

type MockWindow = Window & {
  __shardxMockShortcutPending?: string[];
  __shardxMock: {
    calls: Record<string, number>;
    log: { cmd: string; args: Record<string, unknown> }[];
    state: { profiles: Record<string, { name: string; _meta: { proxy_id: string | null } }>; running: Record<string, unknown> };
    emit: (event: string) => void;
  };
};
const calls = (page: Page, cmd: string) => page.evaluate((cmd) => (window as MockWindow).__shardxMock.calls[cmd] ?? 0, cmd);
const request = (page: Page, id: string) => page.evaluate((id) => {
  const w = window as MockWindow;
  w.__shardxMockShortcutPending = [id];
  w.__shardxMock.emit("profile-shortcut-requested");
}, id);
const sandboxId = (page: Page) => page.locator("[role='row'][data-profile-id]", { hasText: "Sandbox" }).getAttribute("data-profile-id");

test("homepage theme choice applies, persists and supports Escape", async ({ page }) => {
  await openApp(page, { theme: "light" });
  const trigger = page.locator("[data-theme-quick]");
  await expect(trigger).toHaveAccessibleName("Theme: Light");
  await trigger.click();
  await page.getByRole("menuitemradio", { name: "Dark", exact: true }).click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  await expect(trigger).toHaveAccessibleName("Theme: Dark");
  await reloadApp(page);
  await expect(trigger).toHaveAccessibleName("Theme: Dark");
  await trigger.click();
  await page.keyboard.press("Escape");
  await expect(trigger).toBeFocused();
  await expect(page.getByRole("menu")).toHaveCount(0);
  await trigger.click();
  await page.getByRole("menuitemradio", { name: "System", exact: true }).click();
  await expect(trigger).toHaveAccessibleName("Theme: System");
  expect(await page.evaluate(() => localStorage.getItem("shardx-ui-kit-theme"))).toBe("system");
});

test("profile menu creates a link using only the stable profile ID", async ({ page }) => {
  await openApp(page);
  const row = page.locator("[role='row'][data-profile-id]", { hasText: "Sandbox" });
  const id = await row.getAttribute("data-profile-id");
  await row.click({ button: "right" });
  await page.getByRole("menuitem", { name: "Create desktop shortcut", exact: true }).click();
  await expect.poll(() => calls(page, "profile_create_desktop_shortcut")).toBe(1);
  const args = await page.evaluate(() => (window as MockWindow).__shardxMock.log.find((e) => e.cmd === "profile_create_desktop_shortcut")?.args);
  expect(args).toEqual({ profileId: id });
  await expect(page.getByRole("region", { name: "Notifications" }).getByText("Desktop shortcut created", { exact: true })).toBeVisible();
});

test("shortcut starts through normal launch, then focuses without stopping or restarting", async ({ page }) => {
  await openApp(page);
  const id = (await sandboxId(page))!;
  await request(page, id);
  await expect.poll(() => calls(page, "launch")).toBe(1);
  await expect.poll(() => page.evaluate((id) => !!(window as MockWindow).__shardxMock.state.running[id], id)).toBe(true);
  await request(page, id);
  await expect.poll(() => calls(page, "profile_focus_window")).toBe(1);
  expect(await calls(page, "launch")).toBe(1);
  expect(await calls(page, "process_kill")).toBe(0);
  const args = await page.evaluate(() => (window as MockWindow).__shardxMock.log.find((e) => e.cmd === "launch")?.args);
  expect(args?.profileId).toBe(id);
  expect(Object.keys(args ?? {}).filter((k) => /proxy|cdp|debug/i.test(k))).toEqual([]);
});

test("cold startup drains a queued shortcut after profiles load", async ({ page }) => {
  // The fixture ID stands in for the ID already queued by native setup.
  await page.addInitScript(() => { (window as MockWindow).__shardxMockShortcutPending = ["f62a9d43"]; });
  await openApp(page, { idle: false });
  await expect.poll(() => calls(page, "profile_shortcut_take_pending")).toBeGreaterThan(0);
  // Native data contains UUIDs; the webview accepts opaque fixture IDs too.
  await expect.poll(() => page.evaluate(() => (window as MockWindow).__shardxMockShortcutPending?.length)).toBe(0);
  await expect.poll(() => calls(page, "launch")).toBe(1);
});

test("missing or trashed shortcut gives an explicit error without creating or launching", async ({ page }) => {
  await openApp(page);
  await request(page, "8ff57c18-945b-4114-9237-b72b4334a014");
  await expect(page.getByRole("region", { name: "Notifications" }).getByText("This profile no longer exists or is in the Trash. Restore it before using the shortcut.", { exact: true })).toBeVisible();
  expect(await calls(page, "launch")).toBe(0);
  expect(await calls(page, "profile_create")).toBe(0);
});


test("renamed profile and changed proxy are reloaded before shortcut launch", async ({ page }) => {
  await openApp(page);
  const id = (await sandboxId(page))!;
  await page.evaluate((id) => {
    const profile = (window as MockWindow).__shardxMock.state.profiles[id];
    profile.name = "Renamed shortcut target";
    profile._meta.proxy_id = "px-jp-1";
  }, id);
  await request(page, id);
  await expect.poll(() => calls(page, "launch")).toBe(1);
  await expect(page.locator("[role='row'][data-profile-id]", { hasText: "Renamed shortcut target" })).toBeVisible();
  const args = await page.evaluate(() => (window as MockWindow).__shardxMock.log.find((e) => e.cmd === "launch")?.args);
  expect(args).toEqual({ profileId: id, password: undefined });
  const listedBeforeStart = await page.evaluate(() => {
    const log = (window as MockWindow).__shardxMock.log;
    const launched = log.findIndex((entry) => entry.cmd === "launch");
    return log.slice(0, launched).some((entry) => entry.cmd === "profile_list");
  });
  expect(listedBeforeStart).toBe(true);
});

import { expect, test, type Page } from "@playwright/test";
import { appModule, openApp, row } from "./support/app";
import { pickOption } from "./support/select";

type Stored = { name: string; startup_url?: string; ui_language?: string; timezone?: string; navigator: { language?: string }; noise?: { fonts?: { enabled: boolean } } };
type Mock = { __shardxMock: { state: { profiles: Record<string, Stored>; settings: { profile_defaults?: Record<string, unknown> } } } };
const profiles = (page: Page) => page.evaluate(() => (window as unknown as Mock).__shardxMock.state.profiles);
const defaults = (page: Page) => page.evaluate(() => (window as unknown as Mock).__shardxMock.state.settings.profile_defaults);
const defaultsTab = (page: Page) => page.getByRole("tablist", { name: "Settings sections" }).getByRole("tab", { name: "Profile defaults", exact: true });

test("fresh creation stores BrowserScan and Chinese without changing existing profiles", async ({ page }) => {
  await openApp(page);
  const before = await profiles(page);
  await page.getByRole("button", { name: "New profile", exact: true }).first().click();
  const dialog = page.getByRole("dialog", { name: "New profile", exact: true });
  await expect(dialog.getByRole("textbox", { name: "Startup URL", exact: true })).toHaveValue("https://www.browserscan.net/zh/");
  await dialog.getByRole("textbox", { name: "Name", exact: true }).fill("Chinese defaults");
  await dialog.getByRole("tab", { name: "Fingerprint", exact: true }).click();
  await expect(dialog.getByRole("combobox", { name: "Language", exact: true })).toContainText("中文 (简体)");
  await expect(dialog.getByRole("combobox", { name: "Browser interface language", exact: true })).toContainText("Same as the profile language");
  await dialog.getByRole("button", { name: "Create profile", exact: true }).click();
  await expect(dialog).toHaveCount(0);
  const after = await profiles(page);
  for (const [id, value] of Object.entries(before)) expect(after[id]).toEqual(value);
  const made = Object.values(after).find((p) => p.name === "Chinese defaults")!;
  expect(made.startup_url).toBe("https://www.browserscan.net/zh/");
  expect(made.navigator.language).toBe("zh-CN");
  expect(made.ui_language).toBe("profile");
});

test("saved creation settings reach the next form and survive a reload", async ({ page }) => {
  await openApp(page, { section: "settings" });
  const before = await profiles(page);
  await defaultsTab(page).click();
  await page.locator("[data-setting='default-startup-url'] input").fill("https://example.com/start");
  await pickOption(page.locator("[data-setting='default-locale']"), "Language", "English (US)");
  await pickOption(page.locator("[data-setting='default-locale']"), "Timezone", "UTC");
  await pickOption(page.locator("[data-setting='default-locale']"), "Browser interface language", "System language");
  await pickOption(page.locator("[data-setting='default-noise']"), "Fonts", "Auto noise");
  expect(await defaults(page)).toBeUndefined();
  await page.locator("[data-save-bar]").getByRole("button", { name: "Save", exact: true }).click();
  await expect(page.locator("[data-save-bar]")).toHaveCount(0);
  expect(await defaults(page)).toMatchObject({ startup_url: "https://example.com/start", language: "en-US", timezone: "UTC", ui_language: "system", noise_fonts: "auto" });
  expect(await profiles(page)).toEqual(before);
  await page.goto("/?section=profiles");
  await page.getByRole("button", { name: "New profile", exact: true }).first().click();
  const dialog = page.getByRole("dialog", { name: "New profile", exact: true });
  await expect(dialog.getByRole("textbox", { name: "Startup URL", exact: true })).toHaveValue("https://example.com/start");
  await dialog.getByRole("textbox", { name: "Name", exact: true }).fill("Custom defaults");
  await dialog.getByRole("button", { name: "Create profile", exact: true }).click();
  await expect(dialog).toHaveCount(0);
  const made = Object.values(await profiles(page)).find((p) => p.name === "Custom defaults")!;
  expect(made).toMatchObject({ startup_url: "https://example.com/start", timezone: "UTC", navigator: { language: "en-US" }, noise: { fonts: { enabled: true } } });
  expect(made.ui_language).not.toBe("profile");
});

test("editing a legacy profile leaves its missing startup URL and language unchanged", async ({ page }) => {
  await openApp(page);
  const before = await profiles(page);
  const sandbox = Object.entries(before).find(([, p]) => p.name === "Sandbox")!;
  await row(page, "Sandbox").click({ button: "right" });
  await page.getByRole("menuitem", { name: "Edit", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "Edit Sandbox", exact: true });
  await expect(dialog.getByRole("textbox", { name: "Startup URL", exact: true })).toHaveValue("");
  await dialog.getByRole("textbox", { name: "Name", exact: true }).fill("Legacy renamed");
  await dialog.getByRole("button", { name: "Save", exact: true }).click();
  await expect(dialog).toHaveCount(0);
  const saved = (await profiles(page))[sandbox[0]];
  expect(saved.startup_url).toBeUndefined();
  expect(saved.navigator.language).toBe(sandbox[1].navigator.language);
  expect(saved.ui_language).toBe(sandbox[1].ui_language);
});

test("an invalid startup URL blocks a pasted proxy before any save and can be corrected", async ({ page }) => {
  await openApp(page);
  const writes = () => page.evaluate(() => {
    const mock = (window as unknown as { __shardxMock: { calls: Record<string, number>; state: { proxies: unknown[] } } }).__shardxMock;
    return { proxy: mock.calls.proxy_save ?? 0, profile: mock.calls.profile_save ?? 0, count: mock.state.proxies.length };
  });
  const before = await writes();
  await page.getByRole("button", { name: "New profile", exact: true }).first().click();
  const dialog = page.getByRole("dialog", { name: "New profile", exact: true });
  await dialog.getByRole("textbox", { name: "Name", exact: true }).fill("Corrected startup");
  await dialog.getByRole("textbox", { name: "Startup URL", exact: true }).fill("javascript:alert(1)");
  await dialog.getByRole("tab", { name: "Proxy", exact: true }).click();
  await dialog.locator("[data-property='proxy'] button").click();
  await page.getByRole("button", { name: "Add a new proxy", exact: true }).click();
  await page.getByRole("textbox", { name: "Proxy line", exact: true }).fill("203.0.113.111:1081:user:pass");
  await page.getByRole("button", { name: "Add", exact: true }).click();
  await expect(dialog.getByRole("button", { name: "Create profile", exact: true })).toBeDisabled();
  expect(await writes()).toEqual(before);
  // The write entry point repeats the validation even if a caller bypasses
  // the disabled button, so a staged proxy cannot become an orphan.
  await appModule(page, "/src/features/profile-form/model/useProfileForm.ts", "__defaultsForm");
  expect(await page.evaluate(() => (window as unknown as { __defaultsForm: { saveProfileForm: () => Promise<boolean> } }).__defaultsForm.saveProfileForm())).toBe(false);
  expect(await writes()).toEqual(before);
  const url = dialog.getByRole("textbox", { name: "Startup URL", exact: true });
  await expect(url).toHaveAttribute("aria-invalid", "true");
  await expect(dialog.getByText("Enter an HTTP or HTTPS URL, about:blank, or leave it empty.").first()).toBeVisible();
  await url.fill("https://example.com/start");
  await expect(dialog.getByRole("button", { name: "Create profile", exact: true })).toBeEnabled();
  await dialog.getByRole("button", { name: "Create profile", exact: true }).click();
  await expect(dialog).toHaveCount(0);
  expect(await writes()).toMatchObject({ proxy: before.proxy + 1, count: before.count + 1 });
  const made = Object.values(await profiles(page)).find((p) => p.name === "Corrected startup")!;
  expect(made.startup_url).toBe("https://example.com/start");
});

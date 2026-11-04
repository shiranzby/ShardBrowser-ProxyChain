import { expect, test, type Page } from "@playwright/test";
import { appModule, openApp, reloadApp, waitForIdle } from "./support/app";

const tab = (page: Page, name: string) => page.getByRole("tablist", { name: "Settings sections" }).getByRole("tab", { name, exact: true });
const bar = (page: Page) => page.locator("[data-save-bar]");
type Mock = { __shardxMock: { state: { settings: Record<string, any> }; calls: Record<string, number> }; __shardxMockSettingsSaves?: Record<string, any>[] };
const mock = (page: Page) => page.evaluate(() => (window as unknown as Mock).__shardxMock);
const password = (page: Page) => page.getByRole("textbox", { name: "Password (optional)" });
const save = (page: Page) => bar(page).getByRole("button", { name: "Save", exact: true }).click();
async function manual(page: Page, mode = "Manual HTTP") {
  await page.getByRole("combobox", { name: "Configuration method" }).click();
  await page.getByRole("option", { name: mode, exact: true }).click();
}
async function openProxy(page: Page) {
  await openApp(page, { section: "settings" });
  await tab(page, "Proxy").click();
}

test("Proxy is an independent settings section with a TCP-only boundary", async ({ page }) => {
  await openApp(page, { section: "settings" });
  await expect(tab(page, "Proxy")).toBeVisible();
  await tab(page, "Proxy").click();
  await expect(page.getByText("Proxy chain v1 supports TCP only.")).toBeVisible();
  await expect(page.getByRole("switch", { name: "Use system proxy to connect to proxies" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Read Windows settings" })).toBeVisible();
});

test("Proxy chain draft writes a secret once and never returns it", async ({ page }) => {
  await openApp(page, { section: "settings" });
  await tab(page, "Proxy").click();
  await page.getByRole("switch", { name: "Use system proxy to connect to proxies" }).check();
  await page.getByRole("combobox", { name: "Configuration method" }).click();
  await page.getByRole("option", { name: "Manual HTTP" }).click();
  await page.getByRole("textbox", { name: "Host or IP" }).fill("127.0.0.1");
  await page.getByRole("textbox", { name: "Port" }).fill("7890");
  await page.getByRole("textbox", { name: "Password (optional)" }).fill("mock-only-password");
  await expect(bar(page)).toBeVisible();
  await bar(page).getByRole("button", { name: "Save", exact: true }).click();
  await expect.poll(async () => (await mock(page)).calls.settings_save).toBe(1);
  await expect(bar(page)).toHaveCount(0);
  await expect(page.getByText("A password is saved securely.")).toBeVisible();
  const saveArgs = (await mock(page)).log.filter((x: any) => x.cmd === "settings_save").at(-1)?.args as any;
  // A newly entered password is a write-only intent. The mock records the
  // request so this test can prove the backend consumed it, but its persisted
  // settings state never keeps the plaintext value.
  expect(saveArgs?.value?.proxy_chain?.password).toBe("mock-only-password");
  expect((await mock(page)).state.settings.proxy_chain.password).toBeUndefined();
  await reloadApp(page);
  await tab(page, "Proxy").click();
  await expect(page.getByRole("textbox", { name: "Password (optional)" })).toHaveValue("");
  await expect(page.getByText("A password is saved securely.")).toBeVisible();
});

test("System and profile test results keep their stages separate", async ({ page }) => {
  await openApp(page, { section: "settings" });
  await tab(page, "Proxy").click();
  await page.getByRole("button", { name: "Test system proxy" }).click();
  await expect(page.locator("[data-chain-result]")).toBeVisible();
  await expect(page.locator("[data-chain-stage='system']")).toContainText("Connected");
  await expect(page.locator("[data-chain-stage='profile']")).toContainText("Not tested");
  await expect(page.locator("[data-chain-exit]")).toContainText("System proxy exit IP (first hop)");
  await expect(page.locator("[data-chain-exit]")).toContainText("198.51.100.24");
  await expect(page.locator("[data-chain-location]")).toContainText("Singapore");
  await expect(page.locator("[data-chain-exit-scope]")).toContainText("not the profile’s final exit");
  await page.getByRole("combobox", { name: "Profile" }).click();
  await page.getByRole("option").first().click();
  await page.getByRole("button", { name: "Test profile proxy chain" }).click();
  await expect(page.locator("[data-chain-stage='profile']")).toContainText("Connected");
  await expect(page.locator("[data-chain-exit]")).toContainText("203.0.113.42");
  await expect(page.locator("[data-chain-exit]")).toContainText("Final exit IP");
  await expect(page.locator("[data-chain-exit]")).not.toContainText("198.51.100.24");
  await expect(page.locator("[data-chain-location]")).toContainText("Singapore");
  await expect(page.locator("[data-chain-geo-source]")).toContainText("mock-only.example");
  await expect(page.locator("[data-chain-exit-scope]")).toHaveCount(0);
});

test("local network lookup identifies its own exit without claiming a proxy connection", async ({ page }) => {
  await openProxy(page);
  await page.getByRole("button", { name: "Test local network", exact: true }).click();
  await expect(page.locator("[data-chain-exit]")).toContainText("Local network exit IP");
  await expect(page.locator("[data-chain-exit]")).toContainText("192.0.2.18");
  await expect(page.locator("[data-chain-location]")).toContainText("Singapore");
  await expect(page.locator("[data-chain-stage='system']")).toContainText("Not tested");
  await expect(page.locator("[data-chain-stage='profile']")).toContainText("Not tested");
  expect((await mock(page)).calls.settings_save ?? 0).toBe(0);
});

test("a connected system proxy with failed Geo stays a partial result without invented IP or region", async ({ page }) => {
  await openProxy(page);
  await page.evaluate(() => {
    (window as any).__shardxMockChainResult = {
      exit_ip: null, exit_geo: null, exit_error_code: "proxy_chain_system_geo_failed",
      exit_error: "The system proxy connected, but no live exit IP was returned.",
    };
  });
  await page.getByRole("button", { name: "Test system proxy", exact: true }).click();
  await expect(page.locator("[data-chain-stage='system']")).toContainText("Connected");
  await expect(page.locator("[data-chain-exit]")).toContainText("Lookup failed");
  await expect(page.locator("[data-chain-location]")).toContainText("Lookup failed");
  await expect(page.locator("[data-chain-exit-error]")).toContainText("did not fall back to a direct lookup");
  await expect(page.locator("[data-chain-geo-source]")).toHaveCount(0);
});

test("Geo never borrows a location from a different exit and absent fields stay unknown", async ({ page }) => {
  await openProxy(page);
  await page.evaluate(() => {
    (window as any).__shardxMockChainResult = {
      exit_geo: { ip: "203.0.113.99", country: "Borrowed country", country_code: "", region: "Borrowed region", city: "Borrowed city", provider: "wrong-route.example" },
    };
  });
  await page.getByRole("button", { name: "Test system proxy", exact: true }).click();
  await expect(page.locator("[data-chain-location]")).toContainText("returned no location");
  await expect(page.locator("[data-chain-result]")).not.toContainText("Borrowed");
  await expect(page.locator("[data-chain-geo-source]")).toHaveCount(0);
  await page.evaluate(() => {
    (window as any).__shardxMockChainResult = {
      exit_geo: { ip: "198.51.100.24", country: "", country_code: "", region: "", city: "", provider: "mock-only.example" },
    };
  });
  await page.getByRole("button", { name: "Test system proxy", exact: true }).click();
  await expect(page.locator("[data-chain-exit]")).toContainText("198.51.100.24");
  await expect(page.locator("[data-chain-location]")).toContainText("returned no location");
});

test("discarding a proxy draft writes nothing and restores the fields", async ({ page }) => {
  await openProxy(page);
  await manual(page, "Manual SOCKS5");
  await page.getByRole("textbox", { name: "Host or IP" }).fill("draft.example");
  await password(page).fill("mock-discard-only");
  await bar(page).getByRole("button", { name: "Don’t save" }).click();
  await expect(bar(page)).toHaveCount(0);
  await expect(page.getByRole("combobox", { name: "Configuration method" })).toContainText("Detect system proxy automatically");
  expect((await mock(page)).calls.settings_save ?? 0).toBe(0);
  expect((await mock(page)).state.settings.proxy_chain.enabled).toBe(false);
});

test("a failed save keeps the password draft for retry and a success consumes it", async ({ page }) => {
  await openProxy(page);
  await manual(page);
  await password(page).fill("mock-retry-only");
  await page.evaluate(() => { (window as any).__shardxMockFail = { settings_save: 1 }; });
  await save(page);
  await expect(bar(page)).toContainText("Couldn’t save");
  await expect(password(page)).toHaveValue("mock-retry-only");
  expect((await mock(page)).state.settings.proxy_chain.password_saved).toBe(false);
  await save(page);
  await expect(bar(page)).toHaveCount(0);
  await expect(password(page)).toHaveValue("");
  await expect(page.getByText("A password is saved securely.")).toBeVisible();
});

test("password intents are not sent again and clearing is an explicit saved change", async ({ page }) => {
  await openProxy(page);
  await manual(page);
  await password(page).fill("mock-once-only");
  await save(page);
  await expect(bar(page)).toHaveCount(0);
  await expect(password(page)).toHaveValue("");
  await page.getByRole("textbox", { name: "Host or IP" }).fill("changed.example");
  await save(page);
  await expect(bar(page)).toHaveCount(0);
  const saves = await page.evaluate(() => (window as unknown as Mock).__shardxMockSettingsSaves ?? []);
  expect(saves.at(-1)?.proxy_chain.password).toBeUndefined();
  expect((await mock(page)).state.settings.proxy_chain.password_saved).toBe(true);
  await page.getByRole("button", { name: "Clear password", exact: true }).click();
  await expect(page.getByText("The saved password will be cleared when you save.")).toBeVisible();
  expect((await mock(page)).state.settings.proxy_chain.password_saved).toBe(true);
  await save(page);
  await expect(bar(page)).toHaveCount(0);
  expect((await mock(page)).state.settings.proxy_chain.password_saved).toBe(false);
  await reloadApp(page);
  await tab(page, "Proxy").click();
  await expect(page.getByText("This password belongs to the system proxy.")).toBeVisible();
});

test("a password typed while saving remains a new unsaved replacement", async ({ page }) => {
  await openProxy(page);
  await manual(page);
  await password(page).fill("mock-first-intent");
  await page.evaluate(() => { (window as any).__shardxMockSlow = { settings_save: 650 }; });
  await save(page);
  await password(page).fill("mock-later-intent");
  await expect.poll(async () => (await mock(page)).state.settings.proxy_chain.password_saved).toBe(true);
  await expect(password(page)).toHaveValue("mock-later-intent");
  await expect(bar(page)).toContainText("Unsaved changes");
  await page.evaluate(() => { (window as any).__shardxMockSlow = {}; });
  await save(page);
  await expect(bar(page)).toHaveCount(0);
  await expect(password(page)).toHaveValue("");
  const saves = await page.evaluate(() => (window as unknown as Mock).__shardxMockSettingsSaves ?? []);
  expect(saves.map((s) => s.proxy_chain.password)).toEqual(["mock-first-intent", "mock-later-intent"]);
});

test("a working first hop cannot hide a failed second hop or invent an exit IP", async ({ page }) => {
  await openProxy(page);
  await page.evaluate(() => {
    (window as any).__shardxMockChainResult = {
      profile_proxy: { state: "failed", error_code: "profile_auth_failed", error_message: "Second proxy rejected the login" },
      exit_ip: null, exit_geo: null, exit_error: "Exit IP was not obtained",
    };
  });
  await page.getByRole("combobox", { name: "Profile", exact: true }).click();
  await page.getByRole("option").first().click();
  await page.getByRole("button", { name: "Test profile proxy chain" }).click();
  await expect(page.locator("[data-chain-stage='system']")).toContainText("Connected");
  await expect(page.locator("[data-chain-stage='profile']")).toContainText("Failed");
  await expect(page.locator("[data-chain-stage='profile']")).toContainText("Second proxy rejected the login");
  await expect(page.locator("[data-chain-exit]")).toContainText("Lookup failed");
  await expect(page.locator("[data-chain-exit-error]")).toContainText("Exit IP was not obtained");
});

test("a late test for an older draft cannot replace the current result", async ({ page }) => {
  await openProxy(page);
  await page.evaluate(() => {
    (window as any).__shardxMockLatency = 0;
    (window as any).__shardxMockLate = { proxy_chain_test: 800 };
    (window as any).__shardxMockChainResult = { proxy_address: "old.example:7890" };
  });
  await page.getByRole("button", { name: "Test system proxy" }).click();
  await expect.poll(async () => (await mock(page)).calls.proxy_chain_test).toBe(1);
  await manual(page);
  await page.getByRole("textbox", { name: "Host or IP" }).fill("current.example");
  await page.evaluate(() => {
    (window as any).__shardxMockLate = {};
    (window as any).__shardxMockChainResult = { proxy_address: "current.example:7890" };
  });
  await page.getByRole("button", { name: "Test system proxy" }).click();
  await expect(page.locator("[data-chain-result]")).toContainText("current.example:7890");
  await waitForIdle(page);
  await expect(page.locator("[data-chain-result]")).toContainText("current.example:7890");
  await expect(page.locator("[data-chain-result]")).not.toContainText("old.example");
});

test("PAC auto detection requests manual configuration without reporting a connection", async ({ page }) => {
  await openProxy(page);
  await page.evaluate(() => {
    (window as any).__shardxMockChainDetection = { state: "manual_required", source: "mock Windows Internet Settings", error_code: "pac_unsupported", error_message: "PAC requires manual configuration" };
  });
  await page.getByRole("button", { name: "Read Windows settings" }).click();
  await expect(page.locator("[data-chain-detection]")).toContainText("Manual configuration required");
  await expect(page.locator("[data-chain-detection]")).toContainText("PAC requires manual configuration");
  await expect(page.locator("[data-chain-result]")).toHaveCount(0);
  expect((await mock(page)).calls.proxy_chain_test ?? 0).toBe(0);
});

test("Chinese configuration errors use the same localized guidance in detection and connection results", async ({ page }) => {
  await page.addInitScript(() => localStorage.setItem("shardx.lang", "zh"));
  await openApp(page, { section: "settings" });
  await page.getByRole("tab", { name: "代理", exact: true }).click();
  await page.evaluate(() => {
    const error = {
      error_code: "proxy_chain_manual_required",
      error_message: "Windows uses PAC/WPAD or an ambiguous proxy mapping. Select manual HTTP or SOCKS5 configuration.",
    };
    (window as any).__shardxMockChainDetection = { state: "manual_required", source: "mock Windows Internet Settings", ...error };
    (window as any).__shardxMockChainResult = { system_proxy: { state: "failed", ...error }, exit_ip: null, exit_geo: null };
  });
  const guidance = "Windows 静态代理映射无法明确判断，请选择手动配置 HTTP 或 SOCKS5。";
  await page.getByRole("button", { name: "读取 Windows 设置" }).click();
  await expect(page.locator("[data-chain-detection]")).toContainText(guidance);
  await expect(page.locator("[data-chain-detection]")).not.toContainText("Windows uses PAC/WPAD");
  await page.getByRole("button", { name: "检测系统代理", exact: true }).click();
  await expect(page.locator("[data-chain-stage='system']")).toContainText(guidance);
  await expect(page.locator("[data-chain-stage='system']")).not.toContainText("Windows uses PAC/WPAD");
  await expect(page.locator("[data-chain-exit]")).toContainText("尚未检测");
  await expect(page.locator("[data-chain-geo-source]")).toHaveCount(0);
});

test("automatic resolution uses the selected second-hop target and displays its resolved route", async ({ page }) => {
  await openProxy(page);
  await page.getByRole("combobox", { name: "Profile", exact: true }).click();
  await page.getByRole("option").first().click();
  await page.evaluate(() => {
    (window as any).__shardxMockChainDetection = {
      state: "ok", kind: "http", host: "127.0.0.1", port: 7890,
      source: "windows_pac", resolved_for: "second.example:4104",
    };
  });
  await page.getByRole("button", { name: "Read Windows settings", exact: true }).click();
  await expect(page.locator("[data-chain-detection]")).toContainText("second.example:4104");
  await expect(page.locator("[data-chain-detection]")).toContainText("127.0.0.1:7890");
  const detectCall = (await mock(page)).log.filter((x: any) => x.cmd === "proxy_chain_detect").at(-1);
  expect(detectCall.args.profileId).toBeTruthy();
  await page.getByRole("button", { name: "Test system proxy", exact: true }).click();
  const testCall = (await mock(page)).log.filter((x: any) => x.cmd === "proxy_chain_test").at(-1);
  expect(testCall.args.profileId).toBe(detectCall.args.profileId);
  expect((await mock(page)).calls.settings_save ?? 0).toBe(0);
});

test("automatic DIRECT is a visible refusal, never a green connection or an exit", async ({ page }) => {
  await openProxy(page);
  await page.evaluate(() => {
    const error = { error_code: "proxy_chain_auto_direct_refused", error_message: "Windows automatic proxy configuration returned DIRECT." };
    (window as any).__shardxMockChainDetection = { state: "failed", source: "windows_pac", resolved_for: "second.example:4104", ...error };
    (window as any).__shardxMockChainResult = { system_proxy: { state: "failed", ...error }, exit_ip: null, exit_geo: null };
  });
  await page.getByRole("button", { name: "Read Windows settings", exact: true }).click();
  await expect(page.locator("[data-chain-detection]")).toContainText("selected a direct route");
  await page.getByRole("button", { name: "Test system proxy", exact: true }).click();
  await expect(page.locator("[data-chain-stage='system']")).toContainText("Failed");
  await expect(page.locator("[data-chain-stage='system']")).toContainText("selected a direct route");
  await expect(page.locator("[data-chain-exit]")).toContainText("Not tested");
});

test("proxy Peek labels TCP-chain UDP as unavailable rather than a failed relay", async ({ page }) => {
  await openApp(page, { section: "proxies" });
  await appModule(page, "/src/entities/proxy/index.ts", "proxyStore");
  await page.evaluate(() => {
    const st = (window as any).proxyStore.useProxy;
    const previous = st.getState().snapshots["px-jp-1"];
    st.setState({ snapshots: { ...st.getState().snapshots, "px-jp-1": { ...previous, transport: "proxy_chain", udp_state: "unsupported_by_chain", udp_ms: null } } });
  });
  await page.locator("[role='row'][data-proxy-id='px-jp-1'] [data-cell='name']").click();
  await expect(page.locator("[data-chain-udp-unsupported]")).toContainText("UDP unavailable in proxy chain");
  await expect(page.locator("aside[data-peek='proxy']")).toContainText("Test route: system proxy chain");
});

for (const lang of ["en", "zh", "ru"]) {
  for (const viewport of [{ width: 1600, height: 900 }, { width: 1024, height: 680 }]) {
    test(`proxy connection tests spacing ${lang} at ${viewport.width}`, async ({ page }, info) => {
      await page.setViewportSize(viewport);
      await page.addInitScript((lang) => localStorage.setItem("shardx.lang", lang), lang);
      await openApp(page, { section: "settings", theme: "light" });
      await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
      await page.getByRole("tab", { name: lang === "en" ? "Proxy" : lang === "zh" ? "代理" : "Прокси", exact: true }).click();
      const card = page.locator("[data-setting='proxy-chain-profile']").locator("..");
      await card.scrollIntoViewIfNeeded();
      const geometry = await card.evaluate((el) => {
        const help = el.querySelector<HTMLElement>(":scope > p")!;
        const actions = help.nextElementSibling!;
        const buttons = Array.from(actions.querySelectorAll("button"));
        const profile = el.querySelector<HTMLElement>("[data-setting='proxy-chain-profile']")!;
        const range = document.createRange();
        range.selectNodeContents(help);
        const text = range.getBoundingClientRect();
        const cardRect = el.getBoundingClientRect();
        const first = buttons[0].getBoundingClientRect();
        const second = buttons[1].getBoundingClientRect();
        const body = document.querySelector<HTMLElement>("[data-page-body]")!;
        return {
          topInset: text.top - cardRect.top,
          helpToButtons: first.top - text.bottom,
          buttonGap: Math.abs(first.top - second.top) < 1 ? second.left - first.right : second.top - first.bottom,
          buttonsToProfile: profile.getBoundingClientRect().top - Math.max(first.bottom, second.bottom),
          cardOverflow: el.scrollWidth - el.clientWidth,
          bodyOverflow: body.scrollWidth - body.clientWidth,
          pageOverflow: document.documentElement.scrollWidth - innerWidth,
          buttonOverflow: Math.max(...buttons.map((button) => button.scrollWidth - button.clientWidth)),
        };
      });
      await info.attach("connection-tests-geometry", { body: JSON.stringify({ lang, theme: "light", viewport, ...geometry }, null, 2), contentType: "application/json" });
      await page.screenshot({ path: info.outputPath(`proxy-spacing-${lang}-${viewport.width}-page.png`) });
      await card.screenshot({ path: info.outputPath(`proxy-spacing-${lang}-${viewport.width}-card.png`) });
      expect(geometry.topInset).toBeGreaterThanOrEqual(12);
      expect(geometry.helpToButtons).toBeGreaterThanOrEqual(12);
      // Preserve the launcher's existing 7px gap-2 and 10px py-3 scale.
      expect(geometry.buttonGap).toBeGreaterThanOrEqual(7);
      expect(geometry.buttonsToProfile).toBeGreaterThanOrEqual(10);
      expect(geometry.cardOverflow).toBeLessThanOrEqual(1);
      expect(geometry.bodyOverflow).toBeLessThanOrEqual(1);
      expect(geometry.pageOverflow).toBeLessThanOrEqual(1);
      expect(geometry.buttonOverflow).toBeLessThanOrEqual(1);
    });
  }
  test(`proxy settings visual ${lang} at the minimum desktop size`, async ({ page }, info) => {
    await page.setViewportSize({ width: 1024, height: 680 });
    await page.addInitScript((lang) => localStorage.setItem("shardx.lang", lang), lang);
    await openApp(page, { section: "settings" });
    await page.getByRole("tab").filter({ hasText: lang === "en" ? /^Proxy$/ : lang === "zh" ? /^代理$/ : /^Прокси$/ }).click();
    await page.locator("[data-setting='proxy-chain-mode'] [role='combobox']").click();
    await page.getByRole("option", { name: lang === "en" ? "Manual HTTP" : lang === "zh" ? "手动配置 HTTP" : "HTTP вручную", exact: true }).click();
    await page.locator("[data-page-body]").evaluate((el) => { el.scrollTop = 0; });
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    await page.screenshot({ path: info.outputPath(`proxy-chain-${lang}-1024x680-top.png`) });
    await page.locator("[data-setting='proxy-chain-profile'] [role='combobox']").click();
    await page.getByRole("option").first().click();
    await page.locator("[data-setting='proxy-chain-profile'] button").click();
    await expect(page.locator("[data-chain-exit]")).toContainText("203.0.113.42");
    await page.locator("[data-page-body]").evaluate((el) => { el.scrollTop = el.scrollHeight; });
    await page.screenshot({ path: info.outputPath(`proxy-chain-${lang}-1024x680-result.png`) });
  });
}

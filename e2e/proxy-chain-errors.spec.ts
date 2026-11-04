import { expect, test, type Page } from "@playwright/test";
import { appModule, openApp } from "./support/app";

async function failedChain(page: Page, failure: { code: string; layer: string; message: string }, tcp: number | null = null) {
  await appModule(page, "/src/entities/proxy/index.ts", "proxyStore");
  await page.evaluate(({ failure, tcp }) => {
    const store = (window as any).proxyStore.useProxy;
    const old = store.getState().snapshots["px-jp-1"];
    store.setState({ snapshots: { ...store.getState().snapshots, "px-jp-1": {
      ...old, transport: "proxy_chain", udp_state: "unsupported_by_chain", tcp_ms: tcp,
      udp_ms: null, ip: "", country: "", region: "", city: "", country_code: "", timezone: "",
      chain_error: failure, geo_error: failure.message,
    } } });
  }, { failure, tcp });
  await page.locator("[role='row'][data-proxy-id='px-jp-1'] [data-cell='name']").click();
}

test("chain refusal identifies first hop and never promises UTC startup", async ({ page }) => {
  await openApp(page, { section: "proxies" });
  await failedChain(page, { code: "system_proxy_connect_refused", layer: "system_proxy", message: "The system proxy refused CONNECT (HTTP 403)." });
  const peek = page.locator("aside[data-peek='proxy']");
  await expect(peek).toContainText("Test route: system proxy chain");
  await expect(peek.locator("[data-chain-test-error]")).toContainText("First hop:");
  await expect(peek.locator("[data-chain-test-error]")).toContainText("Starting is blocked");
  await expect(peek.locator("[data-chain-test-error]")).toContainText("system_proxy_connect_refused");
  await expect(peek).not.toContainText("start on UTC");
  await expect(peek).not.toContainText("Couldn’t connect. Check your internet connection.");
});

test("Chinese chain authentication refusal includes source restrictions without claiming a password fault", async ({ page }) => {
  await page.addInitScript(() => localStorage.setItem("shardx.lang", "zh"));
  await openApp(page, { section: "proxies" });
  await failedChain(page, { code: "proxy_chain_profile_auth_failed", layer: "profile", message: "The profile SOCKS5 proxy rejected its username or password." });
  const peek = page.locator("aside[data-peek='proxy']");
  await expect(peek).toContainText("检测路径：系统代理链");
  await expect(peek.locator("[data-chain-test-error]")).toContainText("第二层：配置代理拒绝认证");
  await expect(peek.locator("[data-chain-test-error]")).toContainText("允许的来源 IP 或地区");
  await expect(peek.locator("[data-chain-test-error]")).toContainText("会阻止启动");
  await expect(peek).not.toContainText("按 UTC 启动");
});

test("Geo failure keeps TCP success visible while the chain remains not ready", async ({ page }) => {
  await openApp(page, { section: "proxies" });
  await failedChain(page, { code: "proxy_chain_profile_geo_failed", layer: "exit", message: "The full chain connected, but no live exit IP was returned." }, 12);
  const peek = page.locator("aside[data-peek='proxy']");
  await expect(peek).toContainText("TCP 12 ms");
  await expect(peek.locator("[data-chain-test-error]")).toContainText("final exit IP and location lookup failed");
  await expect(peek.locator("[data-chain-test-error]")).toContainText("Starting is blocked");
  await expect(peek.locator("[data-peek-test-status]")).toContainText("Failed");
});

test("tagged launch chain error is localized before generic refusal matching", async ({ page }) => {
  await openApp(page);
  await appModule(page, "/src/shared/lib/errors.ts", "chainErrors");
  const detail = await page.evaluate(() => (window as any).chainErrors.explainError("[system_proxy_connect_refused] The system proxy refused SOCKS5 CONNECT: connection refused (code 5)."));
  expect(detail).toContain("First hop:");
  expect(detail).toContain("routing rules");
});

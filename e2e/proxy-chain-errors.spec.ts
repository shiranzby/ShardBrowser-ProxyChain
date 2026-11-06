import { expect, test, type Page } from "@playwright/test";
import { appModule, openApp } from "./support/app";
import { TEAM_WARNINGS } from "../src/shared/api/backendWarnings";

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

for (const route of ["first_hop", "assigned_proxy_fallback", "same_proxy_endpoint"] as const) {
  test(`settings reports the actual ${route} route without crediting an unused first hop`, async ({ page }, info) => {
    await openApp(page, { section: "settings", theme: "light" });
    await page.getByRole("tablist", { name: "Settings sections" }).getByRole("tab", { name: "Proxy", exact: true }).click();
    await page.getByRole("combobox", { name: "Profile", exact: true }).click();
    await page.getByRole("option", { name: "FB Ads acc-02", exact: true }).click();
    await page.evaluate((route) => {
      const system = route === "first_hop" ? { state: "ok", latency_ms: 12 }
        : route === "assigned_proxy_fallback" ? { state: "failed", error_code: "system_proxy_connect_refused" }
          : { state: "not_tested" };
      (window as any).__shardxMockChainResult = {
        route, first_hop_error_code: route === "assigned_proxy_fallback" ? "system_proxy_connect_refused" : null,
        system_proxy: system, profile_proxy: { state: "ok", latency_ms: 31 },
      };
    }, route);
    await page.getByRole("button", { name: "Test profile proxy chain", exact: true }).click();
    const result = page.locator("[data-chain-result]");
    await expect(result.locator(`[data-chain-route='${route}']`)).toBeVisible();
    await expect(result.locator("[data-chain-stage='profile']")).toContainText("Connected");
    await expect(result.locator("[data-chain-exit]")).toContainText("203.0.113.42");
    if (route === "first_hop") {
      await expect(result.locator("[data-chain-stage='system']")).toContainText("Connected");
      await expect(result.locator("[data-chain-route]")).toContainText("through the first hop");
    } else {
      await expect(result.locator("[data-chain-stage='system']")).not.toContainText("Connected");
      await expect(result.locator("[data-chain-route]")).not.toContainText("through the first hop");
      if (route === "assigned_proxy_fallback") {
        await expect(result.locator("[data-chain-stage='system']")).toContainText("Failed");
        await expect(result.locator("[data-chain-route]")).toContainText("websites still use that proxy");
        await expect(result.locator("[data-chain-first-attempt]")).toContainText("routing rules");
      } else {
        await expect(result.locator("[data-chain-stage='system']")).toContainText("Not tested");
        await expect(result.locator("[data-chain-route]")).toContainText("same endpoint and credentials");
        await expect(result.locator("[data-chain-first-attempt]")).toHaveCount(0);
      }
    }
    await result.screenshot({ path: info.outputPath(`route-settings-${route}.png`), animations: "disabled" });
  });
}

for (const route of ["assigned_proxy_fallback", "same_proxy_endpoint"] as const) {
  test(`proxy history explains ${route} and keeps successful single-proxy results usable`, async ({ page }, info) => {
    await openApp(page, { section: "proxies", theme: "light" });
    await appModule(page, "/src/entities/proxy/index.ts", "proxyStore");
    await page.evaluate((route) => {
      const store = (window as any).proxyStore.useProxy;
      const old = store.getState().snapshots["px-jp-1"];
      store.setState({ snapshots: { ...store.getState().snapshots, "px-jp-1": {
        ...old, transport: "proxy_chain", route, tcp_ms: 31, udp_ms: null, udp_state: "unsupported_by_chain",
        ip: "198.51.100.44", geo_error: null, chain_error: null,
        first_hop_error_code: route === "assigned_proxy_fallback" ? "system_proxy_connect_refused" : null,
      } } });
    }, route);
    await page.locator("[role='row'][data-proxy-id='px-jp-1'] [data-cell='name']").click();
    const peek = page.locator("aside[data-peek='proxy']");
    await expect(peek.locator(`[data-chain-route='${route}']`)).toBeVisible();
    await expect(peek).not.toContainText("Test route: system proxy chain");
    await expect(peek).toContainText("198.51.100.44");
    await expect(peek).toContainText("TCP 31 ms");
    await expect(peek.locator("[data-chain-test-error]")).toHaveCount(0);
    await expect(peek.locator("[data-peek-test-status]")).not.toContainText("Failed");
    await expect(peek.locator("[data-chain-udp-unsupported]")).toBeVisible();
    await peek.screenshot({ path: info.outputPath(`route-history-${route}.png`), animations: "disabled" });
  });
}

test("both route failures stay blocked and explain in Chinese that no website-direct route was attempted", async ({ page }, info) => {
  await page.addInitScript(() => localStorage.setItem("shardx.lang", "zh"));
  await openApp(page, { section: "proxies", theme: "light" });
  await failedChain(page, { code: "proxy_chain_routes_failed", layer: "profile", message: "Neither protected route passed." });
  const peek = page.locator("aside[data-peek='proxy']");
  await expect(peek.locator("[data-chain-test-error]")).toContainText("第一层路线和直接连接配置代理均未通过，已停止；未尝试直连网站。");
  await expect(peek.locator("[data-peek-test-status]")).toContainText("失败");
  await expect(peek.locator("[data-chain-test-error]")).not.toContainText("Neither protected route");
  await appModule(page, "/src/shared/lib/errors.ts", "chainErrors");
  const text = await page.evaluate(() => (window as any).chainErrors.explainError("[proxy_chain_routes_failed] Neither protected route passed."));
  expect(text).toContain("未尝试直连网站");
  await peek.screenshot({ path: info.outputPath("route-both-failed-zh.png"), animations: "disabled" });
});

test("native launcher route warnings arrive as Chinese route toasts without raw backend messages", async ({ page }, info) => {
  await page.addInitScript(() => localStorage.setItem("shardx.lang", "zh"));
  await openApp(page, { theme: "light" });
  await appModule(page, "/src/shared/model/toast.ts", "__routeToasts");
  for (const [raw, detail, name] of [
    [TEAM_WARNINGS.proxyAssignedFallback("Synthetic HK profile"), "第一层路线未通过，本次已验证并直接连接配置代理；仍经该代理出网。", "assigned-fallback"],
    [TEAM_WARNINGS.proxySameEndpoint("Synthetic HK profile"), "第一层与配置代理的入口及认证一致，本次跳过重复一层，保留配置代理。", "same-endpoint"],
  ]) {
    await page.evaluate((text) => (window as any).__shardxMock.emit("launcher-warning", text), raw);
    const toast = page.locator("[data-toast]").filter({ hasText: "“Synthetic HK profile”的代理路线" });
    await expect(toast).toBeVisible();
    await expect(toast).toContainText(detail);
    await expect(toast).not.toContainText("Proxy route for");
    await expect(toast).not.toContainText("直连网站");
    await expect(toast).not.toContainText("本次经第一层连接配置代理");
    await toast.screenshot({ path: info.outputPath(`route-launch-warning-${name}-zh.png`), animations: "disabled" });
    await page.evaluate(() => (window as any).__routeToasts.useToastStore.setState({ items: [] }));
  }
});

test("a shared endpoint failure describes one verified route without claiming both routes were tried", async ({ page }) => {
  await page.addInitScript(() => localStorage.setItem("shardx.lang", "zh"));
  await openApp(page, { section: "proxies" });
  await failedChain(page, { code: "proxy_chain_same_proxy_failed", layer: "profile", message: "The identical endpoint did not pass verification." });
  const failure = page.locator("aside[data-peek='proxy'] [data-chain-test-error]");
  await expect(failure).toContainText("proxy_chain_same_proxy_failed");
  await expect(failure).toContainText("相同");
  await expect(failure).not.toContainText("第一层路线和直接连接配置代理均未通过");
  await expect(failure).not.toContainText("The identical endpoint");
  await expect(page.locator("aside[data-peek='proxy'] [data-peek-test-status]")).toContainText("失败");
});

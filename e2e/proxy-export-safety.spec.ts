import { expect, test, type Page } from "@playwright/test";
import { appModule, openApp } from "./support/app";

const clipboard = (page: Page) => page.evaluate(() =>
  (window as { __shardxMockClipboard?: string }).__shardxMockClipboard ?? "",
);

async function exportFixture(page: Page) {
  await openApp(page, { section: "proxies" });
  await appModule(page, "/src/entities/proxy/index.ts", "__proxyExport");
  await page.evaluate(() => {
    const proxy = (window as unknown as {
      __proxyExport: { useProxy: { setState(value: { proxies: unknown[] }): void; getState(): {
        bulkExport(ids: string[]): Promise<void>;
      } } };
    }).__proxyExport.useProxy.getState();
    (window as unknown as { __proxyExport: { useProxy: { setState(value: { proxies: unknown[] }): void } } }).__proxyExport.useProxy.setState({ proxies: [{
      id: "export-safety-fixture", name: "Synthetic export", kind: "socks5",
      host: "relay.example.invalid", port: 1080,
      username: "fixture-account", password: "fixture-secret",
      country: "", notes: "",
    }] });
    // Resolve through the real confirmation UI, as the list command does.
    void proxy.bulkExport(["export-safety-fixture"]);
  });
  await expect(page.getByRole("dialog", { name: "Export proxies" })).toBeVisible();
}

test("proxy export defaults to addresses without login credentials", async ({ page }) => {
  await exportFixture(page);
  const dialog = page.getByRole("dialog", { name: "Export proxies" });
  await dialog.getByRole("button", { name: "Copy addresses only", exact: true }).click();
  await expect.poll(() => clipboard(page)).toBe("socks5://relay.example.invalid:1080  # Synthetic export");
  expect(await clipboard(page)).not.toContain("fixture-account");
  expect(await clipboard(page)).not.toContain("fixture-secret");
});

test("explicit credential export expires after a minute", async ({ page }) => {
  await exportFixture(page);
  await page.clock.install();
  await page.getByRole("dialog", { name: "Export proxies" })
    .getByRole("button", { name: "Copy with credentials", exact: true }).click();
  await expect.poll(() => clipboard(page)).toBe("socks5://fixture-account:fixture-secret@relay.example.invalid:1080  # Synthetic export");
  await page.clock.fastForward(60_001);
  await expect.poll(() => clipboard(page)).toBe("");
});

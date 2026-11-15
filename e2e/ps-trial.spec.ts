import { expect, test } from "@playwright/test";
import { openApp, reloadApp } from "./support/app";

// Marketing trial offers are intentionally suppressed in ShardBrowser. The
// server endpoint remains available for upstream compatibility, but it must
// never interrupt startup or take focus from the operator.
test("does not show the ProxyShard residential trial at startup", async ({ page }) => {
  await page.addInitScript(() => {
    (window as { __shardxMockTrialOffer?: unknown }).__shardxMockTrialOffer = {
      show: true,
      url: "https://dashboard.proxyshard.example/trial",
      trial_gb: 0.1,
    };
  });
  await openApp(page, { section: "profiles" });
  await page.evaluate(() => {
    const mock = (window as unknown as {
      __shardxMock: { state: { psKey: string }; save: () => void };
    }).__shardxMock;
    mock.state.psKey = "";
    mock.save();
  });
  await expect.poll(() => page.evaluate(() =>
    JSON.parse(localStorage.getItem("shardx-mock-state") ?? "{}").psKey,
  )).toBe("");
  await reloadApp(page);
  await expect(page.getByRole("heading", { name: "All profiles" })).toBeVisible();
  await expect(page.getByRole("dialog", { name: /100 MB|premium residential|高级住宅/ })).toHaveCount(0);
});

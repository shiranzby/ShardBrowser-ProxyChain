import { expect, test } from "@playwright/test";
import { openApp, reloadApp } from "./support/app";

test("hides the trial by default and restores it only after saving the appearance setting", async ({ page }) => {
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
  const trial = page.getByRole("dialog", { name: /100 MB/ });
  await expect(trial).toHaveCount(0);
  const requests = () => page.evaluate(() => (window as any).__shardxMock.log.filter((entry: any) => entry.cmd === "ps_trial_offer").length);
  expect(await requests()).toBe(0);

  await page.locator("[data-nav='settings']").click();
  await page.getByRole("tab", { name: "Appearance", exact: true }).click();
  const toggle = page.getByRole("switch", { name: "Hide ProxyShard trial offer", exact: true });
  await expect(toggle).toBeChecked();
  await toggle.click();
  await page.locator("[data-save-bar]").getByRole("button", { name: "Save", exact: true }).click();
  await expect(trial).toBeVisible();
  expect(await requests()).toBe(1);
  await trial.getByRole("button", { name: "Not now", exact: true }).click();
  await expect(trial).toHaveCount(0);
});

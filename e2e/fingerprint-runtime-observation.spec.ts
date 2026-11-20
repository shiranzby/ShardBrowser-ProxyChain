import { expect, test, type Page } from "@playwright/test";
import { openApp } from "./support/app";

const invoke = (page: Page, command: string, args: Record<string, unknown> = {}) =>
  page.evaluate(([command, args]) =>
    (window as unknown as {
      __TAURI_INTERNALS__: { invoke: (name: string, payload: Record<string, unknown>) => Promise<unknown> };
    }).__TAURI_INTERNALS__.invoke(command, args),
  [command, args] as const);

async function openPeek(page: Page) {
  const card = page.locator("[data-fingerprint-id]", { hasText: "macOS 14, Apple M1" }).first();
  await card.click();
  await expect(page.locator("aside[data-peek='fingerprint']")).toBeVisible();
  return card.getAttribute("data-fingerprint-id");
}

test("runtime observation keeps the dialog usable when no automation session is running", async ({ page }) => {
  await openApp(page, { section: "fingerprints" });
  await openPeek(page);

  await page.getByRole("button", { name: "Observe current runtime" }).click();
  const dialog = page.getByRole("dialog", { name: "Observe fingerprint runtime", exact: true });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByText("Choose a running profile attached through automation.", { exact: false })).toBeVisible();
  await expect(dialog.getByRole("button", { name: "Observe current runtime", exact: true })).toBeDisabled();
});

test("runtime observation records source metadata after an automation session succeeds", async ({ page }) => {
  await openApp(page, { section: "fingerprints" });
  await invoke(page, "automation_launch", { profileId: "a78a0eea", restart: true });
  const fingerprintId = await openPeek(page);

  await page.getByRole("button", { name: "Observe current runtime" }).click();
  const dialog = page.getByRole("dialog", { name: "Observe fingerprint runtime", exact: true });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole("combobox", { name: "Running profile", exact: true })).toHaveValue("a78a0eea");
  await dialog.getByRole("button", { name: "Observe current runtime", exact: true }).click();
  await expect(dialog).toHaveCount(0);
  await expect(page.locator("[data-peek-section='field-checks']").getByText("Observed at runtime, not proof of hardware authenticity").first()).toBeVisible();

  const entries = await invoke(page, "fingerprint_list");
  const observed = (entries as Array<{ id: string; field_sources?: Record<string, string> }>)
    .find((item) => item.id === fingerprintId);
  expect(observed?.field_sources).toMatchObject({ navigator: "runtime_observed", screen: "runtime_observed" });
});

test("runtime observation keeps the dialog open and reports backend errors", async ({ page }) => {
  await page.addInitScript(() => {
    (window as unknown as { __shardxMockRuntimeObserveError: string }).__shardxMockRuntimeObserveError = "runtime probe failed";
  });
  await openApp(page, { section: "fingerprints" });
  await invoke(page, "automation_launch", { profileId: "a78a0eea", restart: true });
  await openPeek(page);

  await page.getByRole("button", { name: "Observe current runtime" }).click();
  const dialog = page.getByRole("dialog", { name: "Observe fingerprint runtime", exact: true });
  await dialog.getByRole("button", { name: "Observe current runtime", exact: true }).click();
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole("alert")).toContainText("runtime probe failed");
});

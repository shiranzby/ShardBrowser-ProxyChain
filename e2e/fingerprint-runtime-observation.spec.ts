import { expect, test, type Page } from "@playwright/test";
import { openApp } from "./support/app";

const invoke = (page: Page, command: string, args: Record<string, unknown> = {}) =>
  page.evaluate(([command, args]) =>
    (window as unknown as {
      __TAURI_INTERNALS__: { invoke: (name: string, payload: Record<string, unknown>) => Promise<unknown> };
    }).__TAURI_INTERNALS__.invoke(command, args),
  [command, args] as const);

async function openPeek(page: Page) {
  await page.evaluate(() => {
    const mock = (window as unknown as {
      __shardxMock: {
        state: { fingerprints: Array<Record<string, unknown>> };
        save: () => void;
      };
    }).__shardxMock;
    if (mock.state.fingerprints.some((entry) => entry.id === "runtime-observation-imported")) return;
    const source = mock.state.fingerprints.find((entry) => entry.label === "macOS 14, Apple M1");
    if (!source) throw new Error("runtime observation fixture fingerprint is missing");
    mock.state.fingerprints.push({
      ...source,
      id: "runtime-observation-imported",
      label: "Imported runtime observation",
      builtin: false,
    });
    mock.save();
  });
  await page.reload();
  const card = page.locator("[data-fingerprint-id]", { hasText: "Imported runtime observation" }).first();
  await card.click();
  await expect(page.locator("aside[data-peek='fingerprint']")).toBeVisible();
  return card.getAttribute("data-fingerprint-id");
}

async function openBuiltinPeek(page: Page) {
  const card = page.locator("[data-fingerprint-id]", { hasText: "macOS 14, Apple M1" }).first();
  await card.click();
  await expect(page.locator("aside[data-peek='fingerprint']")).toBeVisible();
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

test("built-in fingerprints explain why runtime observation is unavailable", async ({ page }) => {
  await openApp(page, { section: "fingerprints" });
  await openBuiltinPeek(page);

  const fieldChecks = page.locator("[data-peek-section='field-checks']");
  await expect(fieldChecks.getByRole("button", { name: "Observe current runtime", exact: true })).toHaveCount(0);
  await expect(fieldChecks).toContainText("Built-in fingerprints cannot store runtime observations.");
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

import { expect, test, type Page } from "@playwright/test";
import { openApp } from "./support/app";

async function openExtensions(page: Page) {
  await openApp(page);
  await page.getByRole("button", { name: "New profile", exact: true }).first().click();
  const form = page.getByRole("dialog", { name: "New profile", exact: true });
  await form.getByRole("tab", { name: "Extensions", exact: true }).click();
  await form.getByRole("button", { name: "Add extension", exact: true }).click();
  await page.getByRole("menuitem", { name: "From a link", exact: true }).click();
  return form;
}

test("Profile extension import stays in the draft and reloads the extension picker", async ({ page }) => {
  const form = await openExtensions(page);
  await form.getByRole("textbox", { name: "Web Store page, extension id, or a .crx / .zip link" }).fill("abcdefghijklmnopabcdefghijklmnop");
  await form.getByRole("button", { name: "Download", exact: true }).click();
  await expect(form.locator("[data-extension-import]")).toHaveCount(0);
  await expect(form.getByRole("option", { name: /Web Store abcd/ })).toBeVisible();
  await form.getByRole("option", { name: /Web Store abcd/ }).click();
  await expect(form.getByRole("option", { name: /Web Store abcd/ })).toHaveAttribute("aria-selected", "true");
  await expect(form).toBeVisible();
  await expect(page.locator("main [data-page-title]")).toHaveText("All profiles");
});

test("Profile extension download failure preserves the entered link for retry", async ({ page }) => {
  const form = await openExtensions(page);
  const link = form.getByRole("textbox", { name: "Web Store page, extension id, or a .crx / .zip link" });
  await link.fill("abcdefghijklmnopabcdefghijklmnop");
  await page.evaluate(() => { (window as unknown as { __shardxMockFail: Record<string, number> }).__shardxMockFail = { extension_import_url: 1 }; });
  await form.getByRole("button", { name: "Download", exact: true }).click();
  await expect(page.locator("[data-toast]").getByText("Couldn’t download the extension. Check the link and your connection, then try again.", { exact: true })).toBeVisible();
  await expect(link).toHaveValue("abcdefghijklmnopabcdefghijklmnop");
  await expect(form).toBeVisible();
  await form.getByRole("button", { name: "Download", exact: true }).click();
  await expect(form.locator("[data-extension-import]")).toHaveCount(0);
  await expect(form.getByRole("option", { name: /Web Store abcd/ })).toBeVisible();
});

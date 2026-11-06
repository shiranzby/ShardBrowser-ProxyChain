import { expect, test, type Page } from "@playwright/test";
import { openApp, storedFolders } from "./support/app";
import { ADMIN_INVITE, joinTeam } from "./support/team";

const folder = (page: Page, name: string) => page.locator(`aside [data-folder='${name}']`);
const invoke = (page: Page, command: string, args: Record<string, unknown>) => page.evaluate(({ command, args }) =>
  (window as unknown as { __TAURI_INTERNALS__: { invoke: (name: string, args: Record<string, unknown>) => Promise<unknown> } }).__TAURI_INTERNALS__.invoke(command, args), { command, args });

async function child(page: Page, parent: string, name: string) {
  await folder(page, parent).click({ button: "right" });
  await page.getByRole("menuitem", { name: "New subfolder…", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "New folder", exact: true });
  await dialog.getByRole("textbox", { name: "Name", exact: true }).fill(name);
  await dialog.getByRole("button", { name: "Create", exact: true }).click();
  await expect(dialog).toHaveCount(0);
  await expect(folder(page, name)).toBeVisible();
}

test("nested folders persist, aggregate profiles, collapse, rename and promote children on delete", async ({ page }) => {
  await openApp(page);
  await child(page, "Ads", "Client");
  await child(page, "Client", "Campaign");
  await invoke(page, "profile_set_folder", { id: "f62a9d43", folder: "Campaign" });
  await page.reload();
  await expect(page.locator("[data-folder-depth]:has([data-folder='Campaign'])")).toHaveAttribute("data-folder-depth", "2");
  await expect(folder(page, "Ads").locator("[data-count]")).toHaveText("4");
  await folder(page, "Ads").click();
  await expect(page.locator("[role='row'][data-profile-id]")).toHaveCount(4);
  await page.getByRole("button", { name: "Collapse “Ads”", exact: true }).click();
  await expect(folder(page, "Client")).toHaveCount(0);
  await page.getByRole("button", { name: "Expand “Ads”", exact: true }).click();
  await expect(folder(page, "Campaign")).toBeVisible();

  await folder(page, "Client").click({ button: "right" });
  await page.getByRole("menuitem", { name: "Edit folder", exact: true }).click();
  const edit = page.getByRole("dialog", { name: "Edit folder", exact: true });
  await edit.getByRole("textbox", { name: "Name", exact: true }).fill("Renamed client");
  await edit.getByRole("button", { name: "Save", exact: true }).click();
  await expect(edit).toHaveCount(0);
  await expect.poll(() => storedFolders(page)).toMatchObject({ parents: { "Renamed client": "Ads", Campaign: "Renamed client" } });
  await folder(page, "Renamed client").click({ button: "right" });
  await page.getByRole("menuitem", { name: "Delete folder", exact: true }).click();
  const deletion = page.getByRole("alertdialog");
  await expect(deletion).toContainText("Subfolders and their profiles are kept");
  await deletion.getByRole("button", { name: "Delete folder", exact: true }).click();
  await expect(folder(page, "Renamed client")).toHaveCount(0);
  await expect(page.locator("[data-folder-depth]:has([data-folder='Campaign'])")).toHaveAttribute("data-folder-depth", "1");
  await folder(page, "Campaign").click();
  await expect(page.locator("[role='row'][data-profile-id]")).toHaveCount(1);
  await page.reload();
  await expect(folder(page, "Campaign").locator("[data-count]")).toHaveText("1");
});

test("parent editing moves a subtree, excludes descendants, and backend rejects a cycle", async ({ page }) => {
  await openApp(page);
  await child(page, "Ads", "Child");
  await child(page, "Child", "Leaf");
  const failed = await invoke(page, "folder_set_parent", { name: "Ads", parent: "Leaf" }).then(() => "", String);
  expect(failed).toContain("descendants");
  await folder(page, "Child").click({ button: "right" });
  await page.getByRole("menuitem", { name: "Edit folder", exact: true }).click();
  const edit = page.getByRole("dialog", { name: "Edit folder", exact: true });
  await edit.getByRole("combobox", { name: "Parent folder", exact: true }).click();
  await expect(page.getByRole("option", { name: /Leaf/ })).toHaveCount(0);
  await page.getByRole("option", { name: "Shops", exact: true }).click();
  await edit.getByRole("button", { name: "Save", exact: true }).click();
  await expect(edit).toHaveCount(0);
  await expect.poll(() => storedFolders(page)).toMatchObject({ parents: { Child: "Shops", Leaf: "Child" } });
  await expect(folder(page, "Leaf")).toHaveAttribute("title", "Shops / Child / Leaf");
});

test("failed hierarchy writes keep the dialog open without a false success", async ({ page }) => {
  await page.addInitScript(() => { (window as unknown as { __shardxMockFail: Record<string, number> }).__shardxMockFail = { folder_set_parent: 1 }; });
  await openApp(page);
  await folder(page, "Ads").click({ button: "right" });
  await page.getByRole("menuitem", { name: "New subfolder…", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "New folder", exact: true });
  await dialog.getByRole("textbox", { name: "Name", exact: true }).fill("Failed child");
  await dialog.getByRole("button", { name: "Create", exact: true }).click();
  await expect(page.locator("[data-toast]").filter({ hasText: "Mock failure: folder_set_parent" })).toBeVisible();
  await expect(dialog).toBeVisible();
  await expect(folder(page, "Failed child")).toHaveCount(0);
  await dialog.getByRole("button", { name: "Create", exact: true }).click();
  await expect(dialog).toHaveCount(0);
  await expect(folder(page, "Failed child")).toBeVisible();
});

test("folder reordering stays within siblings and survives reload without moving profiles", async ({ page }) => {
  await openApp(page);
  await child(page, "Ads", "First child");
  await child(page, "Ads", "Second child");
  await folder(page, "Second child").click({ button: "right" });
  await page.getByRole("menuitem", { name: "Move up", exact: true }).click();
  const order = () => page.locator("aside [data-folder]").evaluateAll((els) => els.map((el) => el.getAttribute("data-folder")));
  await expect.poll(order).toEqual(["Ads", "Second child", "First child", "Shops"]);
  await page.reload();
  await expect.poll(order).toEqual(["Ads", "Second child", "First child", "Shops"]);
  const target = (await folder(page, "First child").boundingBox())!;
  await folder(page, "Second child").dragTo(folder(page, "First child"), { targetPosition: { x: 30, y: target.height - 3 } });
  await expect.poll(order).toEqual(["Ads", "First child", "Second child", "Shops"]);
  await expect(folder(page, "Ads").locator("[data-count]")).toHaveText("3");
  await expect(folder(page, "Shops").locator("[data-count]")).toHaveText("3");
});

test("team folders stay flat and reject hierarchy writes even for an administrator", async ({ page }) => {
  await openApp(page);
  await child(page, "Ads", "Personal child");
  await joinTeam(page, ADMIN_INVITE);
  await expect(folder(page, "Personal child")).toHaveCount(0);
  await folder(page, "Ads").click({ button: "right" });
  await expect(page.getByRole("menuitem", { name: "New subfolder…", exact: true })).toHaveCount(0);
  await page.keyboard.press("Escape");
  const epoch = await page.evaluate(() => (window as unknown as { __shardxMock: { epoch: number } }).__shardxMock.epoch);
  const failed = await invoke(page, "folder_set_parent", { name: "Ads", parent: "Shops", epoch }).then(() => "", String);
  expect(failed).toContain("not offered");
  await expect(page.locator("aside [data-folder-depth='1']")).toHaveCount(0);
});

test("a pending folder save cannot discard newer edits or close a different dialog", async ({ page }) => {
  await page.addInitScript(() => { (window as unknown as { __shardxMockSlow: Record<string, number> }).__shardxMockSlow = { folder_set_parent: 1500 }; });
  await openApp(page);
  await folder(page, "Ads").click({ button: "right" });
  await page.getByRole("menuitem", { name: "New subfolder…", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "New folder", exact: true });
  const name = dialog.getByRole("textbox", { name: "Name", exact: true });
  await name.fill("Slow child");
  await dialog.getByRole("button", { name: "Create", exact: true }).click();
  await expect(name).toBeDisabled();
  await expect(dialog.getByRole("button", { name: "Cancel", exact: true })).toHaveCount(0);
  await page.keyboard.press("Escape");
  await expect(dialog).toBeVisible();
  await expect(dialog).toHaveCount(0);
  await expect(folder(page, "Slow child")).toBeVisible();
});

for (const action of ["Delete folder only", "Move profiles to Trash"]) {
  test(`an unreadable folder index stops ${action} before any profile mutation`, async ({ page }) => {
    await openApp(page);
    await folder(page, "Ads").click({ button: "right" });
    await page.getByRole("menuitem", { name: "Delete folder", exact: true }).click();
    const before = await page.evaluate(() => {
      const mock = (window as unknown as { __shardxMock: { state: { profiles: unknown; folders: unknown }; calls: Record<string, number> } }).__shardxMock;
      (window as unknown as { __shardxMockFail: Record<string, number> }).__shardxMockFail = { folder_list: 1 };
      return { profiles: JSON.stringify(mock.state.profiles), folders: JSON.stringify(mock.state.folders), calls: { ...mock.calls } };
    });
    await page.getByRole("alertdialog").getByRole("button", { name: action, exact: true }).click();
    await expect(page.locator("[data-toast]").filter({ hasText: "Mock failure: folder_list" })).toBeVisible();
    const after = await page.evaluate(() => {
      const mock = (window as unknown as { __shardxMock: { state: { profiles: unknown; folders: unknown }; calls: Record<string, number> } }).__shardxMock;
      return { profiles: JSON.stringify(mock.state.profiles), folders: JSON.stringify(mock.state.folders), calls: { ...mock.calls } };
    });
    expect(after.profiles).toBe(before.profiles);
    expect(after.folders).toBe(before.folders);
    for (const cmd of ["folder_delete", "profile_delete", "process_kill"]) expect(after.calls[cmd] ?? 0).toBe(before.calls[cmd] ?? 0);
    await expect(folder(page, "Ads").locator("[data-count]")).toHaveText("3");
  });
}

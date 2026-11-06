import { test } from "node:test";
import assert from "node:assert/strict";
import { folderPath, folderTree, forgetFolderParent, insideFolder, renameFolderParents } from "./folderTree.ts";

test("a folder filter includes descendants but never similarly named siblings", () => {
  const parents = { Child: "Root", Leaf: "Child" };
  assert.equal(insideFolder("Root", "Root", parents), true);
  assert.equal(insideFolder("Leaf", "Root", parents), true);
  assert.equal(insideFolder("Root/old", "Root", parents), false);
  assert.equal(insideFolder("Other", "Root", parents), false);
  assert.equal(insideFolder("", "Root", parents), false);
  assert.equal(insideFolder("", "", parents), true);
  assert.equal(insideFolder("Root", "", parents), false);
});

test("tree order keeps a parent before its children and uses the saved sibling order", () => {
  const result = folderTree(["Leaf", "Child B", "Other", "Root", "Child A"], { "Child A": "Root", "Child B": "Root", Leaf: "Child B" });
  assert.deepEqual(result.map(({ name, depth }) => [name, depth]), [["Other", 0], ["Root", 0], ["Child B", 1], ["Leaf", 2], ["Child A", 1]]);
  assert.equal(folderPath("Leaf", { Leaf: "Child B", "Child B": "Root" }), "Root / Child B / Leaf");
});

test("rename keeps descendants; delete promotes direct children and retains deeper edges", () => {
  const original = { Child: "Root", Leaf: "Child", Deep: "Leaf" };
  const renamed = renameFolderParents(original, "Child", "New");
  assert.deepEqual(renamed, { New: "Root", Leaf: "New", Deep: "Leaf" });
  assert.deepEqual(forgetFolderParent(renamed, "New"), { Leaf: "Root", Deep: "Leaf" });
  assert.deepEqual(forgetFolderParent(original, "Root"), { Leaf: "Child", Deep: "Leaf" });
  assert.deepEqual(original, { Child: "Root", Leaf: "Child", Deep: "Leaf" });
});

test("corrupt cycles, missing parents and object-property names never hide a folder or loop", () => {
  const parents = { A: "B", B: "A", Orphan: "Missing" };
  assert.equal(insideFolder("A", "Other", parents), false);
  const names = ["A", "B", "Orphan", "constructor", "__proto__"];
  const got = folderTree(names, parents);
  assert.deepEqual([...new Set(got.map((n) => n.name))].sort(), [...names].sort());
  assert.equal(folderPath("constructor", parents), "constructor");
  assert.equal(folderPath("__proto__", parents), "__proto__");
  assert.deepEqual(forgetFolderParent({ Child: "constructor" }, "constructor"), {});
});

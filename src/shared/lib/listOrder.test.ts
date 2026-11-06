import { test } from "node:test";
import assert from "node:assert/strict";
import { orderedIds, reorderIds } from "./listOrder.ts";

test("a filtered move changes only visible slots and keeps hidden ids", () => {
  const all = ["a", "hidden-1", "b", "hidden-2", "c"];
  assert.deepEqual(reorderIds(all, [], ["a", "b", "c"], ["c"], "a", false), ["c", "hidden-1", "a", "hidden-2", "b"]);
  assert.deepEqual(all, ["a", "hidden-1", "b", "hidden-2", "c"]);
});

test("a drag from a column sort starts in the displayed order and keeps selected rows together", () => {
  assert.deepEqual(reorderIds(["a", "b", "c", "d", "e"], [], ["e", "d", "c", "b", "a"], ["a", "c"], "e", false), ["c", "a", "e", "d", "b"]);
});

test("a page or group only moves its members, ignoring hidden selected ids", () => {
  assert.deepEqual(reorderIds(["a", "b", "c", "d"], [], ["b", "c"], ["a", "c"], "b", false), ["a", "c", "b", "d"]);
});

test("reload ignores deleted or duplicate saved ids and appends new records", () => {
  assert.deepEqual(orderedIds(["a", "c", "new"], ["c", "gone", "a", "c"]), ["c", "a", "new"]);
});

test("dropping onto the selection or a missing target is a no-op", () => {
  const ids = ["a", "b", "c"];
  assert.deepEqual(reorderIds(ids, [], ids, ["b"], "b", true), ids);
  assert.deepEqual(reorderIds(ids, [], ids, ["a"], "gone", false), ids);
});

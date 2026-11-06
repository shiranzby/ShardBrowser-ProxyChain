import assert from "node:assert/strict";
import test from "node:test";
import { isStartupUrl } from "./startupUrl.ts";

test("startup URL schemes, whitespace and control characters match the backend", () => {
  for (const value of ["", "about:blank", "https://example.com/", "http://localhost:8080/", "https://user:pass@example.com/a b", "\u0085https://example.com/\u0085"]) {
    assert.equal(isStartupUrl(value), true, JSON.stringify(value));
  }
  for (const value of ["example.com", "--no-sandbox", "javascript:alert(1)", "file:///C:/private", "https://example.com/\n--no-sandbox", "https://example.com/\tpage", "https://example.com/\u0085page"]) {
    assert.equal(isStartupUrl(value), false, JSON.stringify(value));
  }
});

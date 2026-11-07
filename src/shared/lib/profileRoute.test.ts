import test from "node:test";
import assert from "node:assert/strict";
import { routeExitProxyId, routeProxyIds, routeUsesRealIp, routeUsesProxy, profileRouteLabel, withRouteProxyId } from "./profileRoute.ts";
import { normalizedProfileRoute, profileRouteForEditor } from "../model/proxyChain.ts";

test("explicit routes keep an inactive binding without claiming it is the exit", () => {
  const proxy_id = "hk";
  for (const mode of ["direct", "system"] as const) {
    const p = { proxy_id, proxy_route: { mode } };
    assert.equal(routeExitProxyId(p), null);
    assert.deepEqual(routeProxyIds(p), []);
    assert.equal(routeUsesRealIp(p), mode === "direct");
  }
  const chain = { proxy_id, proxy_route: { mode: "chain", first: { mode: "proxy", proxy_id: "jp" }, second: { mode: "direct" } } } as const;
  assert.equal(routeExitProxyId(chain), "jp");
  assert.deepEqual(routeProxyIds(chain), ["jp"]);
  assert.equal(routeUsesRealIp(chain), false);
});

test("a two-proxy route requires both entries and exposes the second as its final exit", () => {
  const p = { proxy_id: "hk", proxy_route: { mode: "chain", first: { mode: "proxy", proxy_id: "jp" }, second: { mode: "proxy" } } } as const;
  assert.equal(routeExitProxyId(p), "hk");
  assert.deepEqual(routeProxyIds(p), ["jp", "hk"]);
  assert.equal(routeUsesRealIp(p), false);
  assert.equal(routeUsesRealIp({ proxy_id: null, protected: true }), false);
  assert.equal(routeUsesRealIp({ proxy_id: null }), true);
  assert.equal(routeUsesRealIp({ proxy_id: "missing" }), false);
});

test("route summaries never call a system route or a selected first-hop direct", () => {
  const proxies = [{ id: "hk", name: "HK", host: "hk.example", port: 1080 }, { id: "jp", name: "JP", host: "jp.example", port: 1080 }];
  const t = (key: string, vars?: Record<string, string | number>) => vars ? `${vars.first} / ${vars.second}` : key;
  assert.equal(profileRouteLabel({ proxy_id: "hk", proxy_route: { mode: "system" } }, proxies, t), "profileRoute.summarySystem");
  assert.equal(profileRouteLabel({ proxy_id: "hk", proxy_route: { mode: "chain", first: { mode: "proxy", proxy_id: "jp" }, second: { mode: "direct" } } }, proxies, t), "JP");
  assert.equal(profileRouteLabel({ proxy_id: "hk", proxy_route: { mode: "chain", first: { mode: "system" }, second: { mode: "proxy" } } }, proxies, t), "profileRoute.summaryFirstSystem / HK");
});

test("direct then system shares the system route without claiming the retained assignment", () => {
  const alias = { proxy_id: "retained-hk", proxy_route: { mode: "chain", first: { mode: "direct" }, second: { mode: "system" } } } as const;
  assert.deepEqual(normalizedProfileRoute(alias.proxy_route), { mode: "system" });
  assert.equal(routeExitProxyId(alias), null);
  assert.deepEqual(routeProxyIds(alias), []);
  assert.equal(routeUsesRealIp(alias), false);
  assert.equal(routeUsesProxy(alias), true);
  assert.equal(profileRouteLabel(alias, [], key => key), "profileRoute.summarySystem");
  for (const first of [{ mode: "system" }, { mode: "proxy", proxy_id: "first" }] as const) {
    const invalid = { proxy_id: "retained-hk", proxy_route: { mode: "chain", first, second: { mode: "system" } } } as const;
    assert.deepEqual(normalizedProfileRoute(invalid.proxy_route), { mode: "invalid" });
    assert.equal(profileRouteLabel(invalid, [], key => key), "profileRoute.invalid");
    assert.equal(routeExitProxyId(invalid), null);
    assert.deepEqual(routeProxyIds(invalid), []);
    assert.equal(routeUsesRealIp(invalid), false);
  }
});

test("empty layers appear as their single connection without rewriting the saved route", () => {
  const cases = [
    [{ mode: "chain", first: { mode: "direct" }, second: { mode: "direct" } }, "direct"],
    [{ mode: "chain", first: { mode: "direct" }, second: { mode: "proxy" } }, "proxy"],
    [{ mode: "chain", first: { mode: "direct" }, second: { mode: "system" } }, "system"],
    [{ mode: "chain", first: { mode: "system" }, second: { mode: "direct" } }, "system"],
  ] as const;
  for (const [route, mode] of cases) {
    const before = JSON.stringify(route);
    assert.deepEqual(profileRouteForEditor(route), { mode });
    assert.equal(JSON.stringify(route), before);
  }
});

test("a first-proxy-only legacy route edits A while retaining inactive B", () => {
  const saved = { proxy_id: "retained-b", proxy_route: { mode: "chain", first: { mode: "proxy", proxy_id: "active-a" }, second: { mode: "direct" } } } as const;
  assert.deepEqual(profileRouteForEditor(saved.proxy_route), { mode: "proxy" });
  assert.equal(routeExitProxyId(saved), "active-a");
  assert.deepEqual(routeProxyIds(saved), ["active-a"]);
  const changed = withRouteProxyId(saved, "selected-c");
  assert.equal(changed.proxy_id, "retained-b");
  assert.deepEqual(changed.proxy_route, { mode: "chain", first: { mode: "proxy", proxy_id: "selected-c" }, second: { mode: "direct" } });
  assert.equal(routeExitProxyId(changed), "selected-c");
  assert.equal(saved.proxy_route.first.proxy_id, "active-a");
  assert.equal(withRouteProxyId({ proxy_id: "a", proxy_route: { mode: "proxy" } }, "c").proxy_id, "c");
});

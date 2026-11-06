import test from "node:test";
import assert from "node:assert/strict";
import { routeExitProxyId, routeProxyIds, routeUsesRealIp, profileRouteLabel } from "./profileRoute.ts";

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

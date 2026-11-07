import { test } from "node:test";
import assert from "node:assert/strict";
import { proxyReadyWithUnknownGeo, proxyStatus } from "./status.ts";
import type { ProxyTestSnapshot } from "../model/types.ts";

const snapshot = (extra: Partial<ProxyTestSnapshot>): ProxyTestSnapshot => ({
  first_seen: "@1", last_seen: "@1", ip: "", country_code: "", country: "", region: "", city: "", isp: "",
  timezone: "", latitude: 0, longitude: 0, tcp_ms: null, udp_ms: null, udp_error: null, geo_error: null, provider: "", ...extra,
});
const syntheticError = "safe failure";

test("chain readiness requires both a real probe and live exit without a layer error", () => {
  assert.equal(proxyStatus(snapshot({ transport: "proxy_chain", tcp_ms: 1, ip: "203.0.113.1" })), "ok");
  assert.equal(proxyStatus(snapshot({ transport: "proxy_chain", tcp_ms: 1, geo_error: "safe failure" })), "failed");
  assert.equal(proxyStatus(snapshot({ transport: "proxy_chain", ip: "203.0.113.1", chain_error: { code: "proxy_chain_profile_connect_failed", layer: "profile", message: syntheticError } })), "failed");
  assert.equal(proxyStatus(snapshot({ transport: "proxy_chain", ip: "203.0.113.1" })), "failed");
});

test("non-chain legacy snapshots still accept either connectivity or live Geo", () => {
  assert.equal(proxyStatus(snapshot({ tcp_ms: 1 })), "ok");
  assert.equal(proxyStatus(snapshot({ ip: "203.0.113.1" })), "ok");
  assert.equal(proxyStatus(snapshot({})), "failed");
  assert.equal(proxyStatus(undefined), "untested");
});

test("unknown Geo stays usable only with an explicit current route proof", () => {
  const ready = snapshot({ transport: "proxy_chain", tcp_ms: 17, geo_error: syntheticError,
    chain_error: { code: "proxy_chain_profile_geo_failed", layer: "exit", message: syntheticError, route_ready: true } });
  assert.equal(proxyStatus(ready), "ok");
  assert.equal(proxyReadyWithUnknownGeo(ready), true);
  for (const extra of [
    { chain_error: { ...ready.chain_error!, route_ready: undefined } },
    { chain_error: { ...ready.chain_error!, route_ready: false } },
    { chain_error: { ...ready.chain_error!, code: "proxy_chain_profile_auth_failed" } },
    { chain_error: { ...ready.chain_error!, layer: "profile" } },
    { tcp_ms: null }, { tcp_ms: -1 }, { tcp_ms: Number.NaN },
  ]) {
    const invalid = { ...ready, ...extra };
    assert.equal(proxyReadyWithUnknownGeo(invalid), false);
    assert.equal(proxyStatus(invalid), "failed");
  }
});

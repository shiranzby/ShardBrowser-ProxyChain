import test from "node:test";
import assert from "node:assert/strict";
import { protocolCapabilities, proxyCheckSummary } from "./capabilities.ts";
import type { ProxyTestSnapshot } from "../model/types.ts";

test("SOCKS5 is not TLS just because a destination uses HTTPS", () => {
  assert.equal(protocolCapabilities("socks5").socks5, true);
  assert.equal(protocolCapabilities("socks5").https, false);
  assert.equal(protocolCapabilities("socks5").socks5_tls, false);
  assert.equal(protocolCapabilities("https").https, true);
  assert.equal(protocolCapabilities("https").connect, true);
  assert.equal(protocolCapabilities("http").https, false);
});

test("proxy check summary keeps failure stage and legacy evidence limits", () => {
  assert.deepEqual(proxyCheckSummary(null), { stage: "untested", state: "unverified" });
  const snapshot = { tcp_ms: 1, ip: "", geo_error: null } as ProxyTestSnapshot;
  assert.deepEqual(proxyCheckSummary(snapshot), { stage: "content", state: "unverified" });
  assert.deepEqual(proxyCheckSummary({ ...snapshot, content: { status: "failed", error: "body" } }), { stage: "content", state: "failed" });
  assert.deepEqual(proxyCheckSummary({ ...snapshot, ip: "203.0.113.7", content: { status: "passed", http_status: 200, bytes: 128 } }), { stage: "exit", state: "passed" });
  assert.deepEqual(proxyCheckSummary({ ...snapshot, chain_error: { code: "proxy_chain_profile_auth_failed", layer: "profile", message: "" } }), { stage: "authentication", state: "failed" });
  assert.deepEqual(proxyCheckSummary({ ...snapshot, chain_error: { code: "proxy_chain_profile_geo_failed", layer: "exit", message: "", route_ready: true } }), { stage: "exit", state: "failed" });
});

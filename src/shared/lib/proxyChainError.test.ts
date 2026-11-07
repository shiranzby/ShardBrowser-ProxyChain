import { test } from "node:test";
import assert from "node:assert/strict";
import { proxyChainErrorFromText, proxyChainErrorText } from "./proxyChainError.ts";

const key = (name: string) => name;

test("chain errors keep first-hop, second-hop and exit failures distinct", () => {
  assert.equal(proxyChainErrorText(key, { code: "system_proxy_auth_failed" }), "proxyChain.errorFirstAuth");
  assert.equal(proxyChainErrorText(key, { code: "proxy_chain_profile_auth_failed" }), "proxyChain.errorSecondAuth");
  assert.equal(proxyChainErrorText(key, { error_code: "proxy_chain_profile_geo_failed" }), "proxyChain.profileGeoFailed");
  assert.equal(proxyChainErrorText(key, { code: "proxy_chain_auto_direct_refused" }), "proxyChain.errorAutoDirect");
});

test("tagged launch errors retain their code before generic offline or refused matching", () => {
  const error = proxyChainErrorFromText("[proxy_chain_profile_auth_failed] Authentication refused.");
  assert.equal(error?.code, "proxy_chain_profile_auth_failed");
  assert.equal(proxyChainErrorText(key, error), "proxyChain.errorSecondAuth");
  assert.equal(proxyChainErrorText(key, proxyChainErrorFromText("The system proxy refused SOCKS5 CONNECT: connection refused (code 5).")), "proxyChain.errorFirstRefused");
  assert.equal(proxyChainErrorFromText("connection refused by an unrelated service"), undefined);
});

test("unknown safe chain errors retain detail and empty errors remain empty", () => {
  assert.equal(proxyChainErrorText(key, proxyChainErrorFromText("[proxy_chain_future_error] Safe future detail.")), "Safe future detail.");
  assert.equal(proxyChainErrorText(key, { error_message: "Safe legacy detail." }), "Safe legacy detail.");
  assert.equal(proxyChainErrorText(key, null), undefined);
});

test("single selected-proxy errors do not refer to a chain or its first hop", () => {
  assert.equal(proxyChainErrorText(key, { code: "proxy_chain_profile_connect_failed" }, "proxy"), "profileRoute.errorProxyConnect");
  assert.equal(proxyChainErrorText(key, { code: "proxy_chain_profile_auth_failed" }, "proxy"), "profileRoute.errorProxyAuth");
  assert.equal(proxyChainErrorText(key, { code: "proxy_chain_profile_timeout" }, "proxy"), "profileRoute.errorProxyTimeout");
  assert.equal(proxyChainErrorText(key, { code: "proxy_chain_profile_tls_failed" }, "proxy"), "profileRoute.errorProxyTls");
  assert.equal(proxyChainErrorText(key, { code: "proxy_chain_profile_geo_failed" }, "proxy"), "profileRoute.errorProxyGeo");
  assert.equal(proxyChainErrorText(key, { code: "proxy_chain_profile_kind_unsupported" }, "proxy"), "profileRoute.errorProxyKind");
});

test("single system-proxy errors identify the system connection without hop numbers", () => {
  assert.equal(proxyChainErrorText(key, { code: "system_proxy_connect_refused" }, "system"), "profileRoute.errorSystemConnect");
  assert.equal(proxyChainErrorText(key, { code: "system_proxy_auth_failed" }, "system"), "profileRoute.errorSystemAuth");
  assert.equal(proxyChainErrorText(key, { code: "system_proxy_timeout" }, "system"), "profileRoute.errorSystemTimeout");
  assert.equal(proxyChainErrorText(key, { code: "system_proxy_protocol_error" }, "system"), "profileRoute.errorSystemProtocol");
  assert.equal(proxyChainErrorText(key, { code: "system_proxy_tls_failed" }, "system"), "profileRoute.errorSystemTls");
  assert.equal(proxyChainErrorText(key, { code: "proxy_chain_profile_geo_failed" }, "system"), "proxyChain.systemGeoFailed");
});

test("explicit local-network failure is translated without inventing a proxy layer", () => {
  assert.equal(proxyChainErrorText(key, { code: "proxy_chain_local_network_failed" }), "proxyChain.localGeoFailed");
});

test("verified route with unknown Geo uses the same nonblocking message in every scope", () => {
  for (const code of ["proxy_chain_profile_geo_failed", "proxy_chain_system_geo_failed"]) {
    for (const scope of [undefined, "proxy", "system"] as const) {
      assert.equal(proxyChainErrorText(key, { code, route_ready: true }, scope), "proxyChain.readyGeoUnknown");
    }
  }
  const ready = { code: "proxy_chain_profile_geo_failed", route_ready: true };
  assert.equal(proxyChainErrorText(key, { ...ready, route_ready: false }), "proxyChain.profileGeoFailed");
  assert.equal(proxyChainErrorText(key, { code: "proxy_chain_profile_auth_failed", route_ready: true }), "proxyChain.errorSecondAuth");
});

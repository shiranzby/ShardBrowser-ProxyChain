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

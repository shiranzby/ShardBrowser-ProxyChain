import type { ProxyCapabilities, ProxyEntry, ProxyTestSnapshot } from "../model/types";

/** A transport promise, never inferred from the destination website's scheme. */
export function protocolCapabilities(kind: ProxyEntry["kind"]): ProxyCapabilities {
  return {
    http: kind === "http",
    https: kind === "https",
    socks5: kind === "socks5",
    socks5_tls: false,
    connect: kind === "http" || kind === "https",
  };
}

export type ProxyCheckStage = "untested" | "configuration" | "tcp" | "tls" | "authentication" | "target" | "content" | "exit";
export type ProxyCheckSummary = { stage: ProxyCheckStage; state: "unverified" | "failed" | "passed" };

/** Old history is deliberately weaker evidence than a current verified route. */
export function proxyCheckSummary(snapshot: ProxyTestSnapshot | null | undefined): ProxyCheckSummary {
  if (!snapshot) return { stage: "untested", state: "unverified" };
  const error = snapshot.chain_error;
  if (error) {
    const code = error.code;
    if (error.route_ready) return { stage: "exit", state: "failed" };
    if (/auth|credential/.test(code)) return { stage: "authentication", state: "failed" };
    if (/tls|certificate/.test(code)) return { stage: "tls", state: "failed" };
    if (/config|manual_required|windows_required/.test(code)) return { stage: "configuration", state: "failed" };
    if (/connect|timeout|unavailable/.test(code)) return { stage: "tcp", state: "failed" };
    return { stage: "target", state: "failed" };
  }
  if (snapshot.content?.status === "failed") return { stage: "content", state: "failed" };
  if (snapshot.content?.status !== "passed") return { stage: "content", state: "unverified" };
  if (snapshot.geo_error) return { stage: "exit", state: "failed" };
  if (snapshot.tcp_ms == null) return { stage: "tcp", state: "failed" };
  // Existing successful observations prove the proxy exchange and Geo response;
  // they do not prove a browser rendered an arbitrary page or UDP/IPv6 support.
  return { stage: "exit", state: snapshot.ip ? "passed" : "unverified" };
}

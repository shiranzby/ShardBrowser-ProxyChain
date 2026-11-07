import type { ChainPolicy, ProxyChainRoute } from "../../../shared/model/proxyChain";

export type ProxyEntry = {
  proxy_chain_policy?: ChainPolicy;
  id: string;
  name: string;
  kind: "socks5" | "http" | "https";
  host: string;
  port: number;
  username: string;
  password: string;
  country: string;
  notes: string;
};

export type ProxyTestSnapshot = {
  first_seen: string;
  last_seen: string;
  ip: string;
  country_code: string;
  country: string;
  region: string;
  city: string;
  isp: string;
  timezone: string;
  latitude: number;
  longitude: number;
  tcp_ms: number | null;
  udp_ms: number | null;
  udp_error: string | null;
  /** Why this snapshot has no location. */
  geo_error: string | null;
  provider: string;
  /** Missing in observations from before the proxy-chain patch. */
  transport?: "direct" | "proxy_chain";
  /** The selected route for this observation, never inferred from its exit IP. */
  route?: ProxyChainRoute | null;
  first_hop_error_code?: string | null;
  udp_state?: "unsupported_by_chain";
  /** Safe failure details from the exact chain route; absent in old history. */
  chain_error?: { code: string; layer: string; message: string; route_ready?: boolean | null } | null;
};

export type BulkRowState = {
  entry: ProxyEntry;
  selected: boolean;
  status: "idle" | "testing" | "ok" | "fail";
  tcp_ms?: number | null;
  udp_ms?: number | null;
  country?: string;
  error?: string;
};

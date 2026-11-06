export type ChainPolicy = "inherit" | "enabled" | "disabled" | "prefer_first_hop";
export type ProxyChainRoute = "first_hop" | "assigned_proxy_fallback" | "same_proxy_endpoint" | "explicit_direct" | "explicit_system" | "explicit_proxy" | "explicit_chain";

export type ProfileFirstHop = { mode: "direct" } | { mode: "system" } | { mode: "proxy"; proxy_id: string };
/** Absent on legacy profiles, whose saved chain policy keeps its original meaning. */
export type ProfileProxyRoute = { mode: "direct" } | { mode: "system" } | { mode: "proxy" } | { mode: "invalid" }
  | { mode: "chain"; first: ProfileFirstHop; second: { mode: "direct" } | { mode: "proxy" } };

export const routeUsesAssignedProxy = (route?: ProfileProxyRoute): boolean =>
  !route || route.mode === "proxy" || (route.mode === "chain" && route.second.mode === "proxy");

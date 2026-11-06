export type ChainPolicy = "inherit" | "enabled" | "disabled" | "prefer_first_hop";
export type ProxyChainRoute = "first_hop" | "assigned_proxy_fallback" | "same_proxy_endpoint" | "same_exit_verified" | "explicit_direct" | "explicit_system" | "explicit_proxy" | "explicit_chain";

export type ProfileFirstHop = { mode: "direct" } | { mode: "system" } | { mode: "proxy"; proxy_id: string };
export type ProfileSecondHop = { mode: "direct" } | { mode: "proxy" } | { mode: "system" };
/** Absent on legacy profiles, whose saved chain policy keeps its original meaning. */
export type ProfileProxyRoute = { mode: "direct" } | { mode: "system" } | { mode: "proxy" } | { mode: "invalid" }
  | { mode: "chain"; first: ProfileFirstHop; second: ProfileSecondHop };

/** A chain with no first proxy and a system second choice uses the one system route. */
export const normalizedProfileRoute = (route?: ProfileProxyRoute | null): ProfileProxyRoute | null | undefined =>
  route?.mode === "chain" && route.second.mode === "system" ? { mode: route.first.mode === "direct" ? "system" : "invalid" } : route;

export const routeUsesAssignedProxy = (route?: ProfileProxyRoute): boolean =>
  !route || route.mode === "proxy" || (route.mode === "chain" && route.second.mode === "proxy");

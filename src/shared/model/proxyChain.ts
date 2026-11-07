export type ChainPolicy = "inherit" | "enabled" | "disabled" | "prefer_first_hop";
export type ProxyChainRoute = "first_hop" | "assigned_proxy_fallback" | "same_proxy_endpoint" | "same_exit_verified" | "explicit_direct" | "explicit_system" | "explicit_proxy" | "explicit_chain";

export type ProfileFirstHop = { mode: "direct" } | { mode: "system" } | { mode: "proxy"; proxy_id: string };
export type ProfileSecondHop = { mode: "direct" } | { mode: "proxy" } | { mode: "system" };
/** Absent on legacy profiles, whose saved chain policy keeps its original meaning. */
export type ProfileProxyRoute = { mode: "direct" } | { mode: "system" } | { mode: "proxy" } | { mode: "invalid" }
  | { mode: "chain"; first: ProfileFirstHop; second: ProfileSecondHop };

/** Read old empty-layer choices as the single connection they execute. */
export function normalizedProfileRoute(route?: ProfileProxyRoute | null): ProfileProxyRoute | null | undefined {
  if (route?.mode !== "chain") return route;
  if (route.second.mode === "system") return { mode: route.first.mode === "direct" ? "system" : "invalid" };
  if (route.first.mode === "direct") return { mode: route.second.mode === "proxy" ? "proxy" : "direct" };
  if (route.first.mode === "system" && route.second.mode === "direct") return { mode: "system" };
  // A selected first proxy with no second layer keeps its proxy id in `first`.
  // Returning an unqualified `proxy` here would silently use the retained B.
  return route;
}

/** Five editor choices; the old selected-first-only wire remains untouched. */
export function profileRouteForEditor(route?: ProfileProxyRoute): ProfileProxyRoute | undefined {
  const normalized = normalizedProfileRoute(route);
  return normalized?.mode === "chain" && normalized.second.mode === "direct"
    ? { mode: "proxy" } : normalized ?? undefined;
}

export const routeUsesAssignedProxy = (route?: ProfileProxyRoute): boolean =>
  !route || route.mode === "proxy" || (route.mode === "chain" && route.second.mode === "proxy");

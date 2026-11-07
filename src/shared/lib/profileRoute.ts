import type { ProfileProxyRoute } from "../model/proxyChain";
import { normalizedProfileRoute } from "../model/proxyChain.ts";

type RoutedProfile = { proxy_id: string | null; proxy_route?: ProfileProxyRoute | null; protected?: boolean };
type T = (key: string, vars?: Record<string, string | number>) => string;
type NamedProxy = { id: string; name: string; host: string; port: number };

/** Selecting a single proxy must not replace an old unused B with first-only A. */
export function withRouteProxyId<T extends RoutedProfile>(p: T, proxyId: string | null): T {
  const route = p.proxy_route;
  return route?.mode === "chain" && route.first.mode === "proxy" && route.second.mode === "direct"
    ? { ...p, proxy_route: { ...route, first: { mode: "proxy", proxy_id: proxyId ?? "" } } }
    : { ...p, proxy_id: proxyId };
}

/** The selected proxy that actually provides the final exit, not a retained inactive binding. */
export function routeExitProxyId(p: RoutedProfile): string | null {
  const route = normalizedProfileRoute(p.proxy_route);
  if (!route || route.mode === "proxy") return p.proxy_id;
  if (route.mode !== "chain") return null;
  return route.second.mode === "proxy" ? p.proxy_id
    : route.first.mode === "proxy" ? route.first.proxy_id : null;
}

/** Every selected library proxy this explicit route needs to start. */
export function routeProxyIds(p: RoutedProfile): string[] {
  const route = normalizedProfileRoute(p.proxy_route);
  if (!route || route.mode === "proxy") return p.proxy_id ? [p.proxy_id] : [];
  if (route.mode !== "chain") return [];
  return [...new Set([
    ...(route.first.mode === "proxy" ? [route.first.proxy_id] : []),
    ...(route.second.mode === "proxy" && p.proxy_id ? [p.proxy_id] : []),
  ])];
}

/** A missing selected proxy is a failure, never an intended real-IP route. */
export function routeUsesRealIp(p: RoutedProfile): boolean {
  if (p.protected) return false;
  const route = normalizedProfileRoute(p.proxy_route);
  if (!route) return !p.proxy_id;
  return route.mode === "direct" || (route.mode === "chain" && route.first.mode === "direct" && route.second.mode === "direct");
}

/** System/PAC routes have a proxy even though no library proxy id names it. */
export function routeUsesProxy(p: RoutedProfile): boolean {
  return !routeUsesRealIp(p) && !p.protected;
}

export function profileRouteLabel(p: RoutedProfile, proxies: readonly NamedProxy[], t: T): string | null {
  const route = normalizedProfileRoute(p.proxy_route);
  if (!route) return null;
  const name = (id: string | null) => {
    const proxy = proxies.find(x => x.id === id);
    return proxy ? proxy.name || `${proxy.host}:${proxy.port}` : t("profileRow.proxyMissing");
  };
  if (route.mode === "direct") return t("profileRoute.summaryDirect");
  if (route.mode === "system") return t("profileRoute.summarySystem");
  if (route.mode === "proxy") return name(p.proxy_id);
  if (route.mode !== "chain") return t("profileRoute.invalid");
  const first = route.first.mode === "system" ? t("profileRoute.summaryFirstSystem")
    : route.first.mode === "direct" ? t("profileRoute.summaryFirstDirect") : name(route.first.proxy_id);
  return route.second.mode === "direct" ? first
    : t("profileRoute.summaryChain", { first, second: name(p.proxy_id) });
}

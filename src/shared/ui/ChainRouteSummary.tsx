import { useT } from "../i18n";
import type { ProxyChainRoute } from "../model/proxyChain";
import { proxyChainErrorText } from "../lib/proxyChainError";

/** Describes the selected transport, not a guess based on a shared public IP. */
export function ChainRouteSummary({ route, firstHopError }: { route?: ProxyChainRoute | null; firstHopError?: string | null }) {
  const t = useT();
  if (!route) return null;
  const description = route === "first_hop" ? t("proxyChain.routeFirstHop")
    : route === "same_proxy_endpoint" ? t("proxyChain.routeSameEndpoint")
      : route === "assigned_proxy_fallback" ? t("proxyChain.routeAssignedFallback") : null;
  if (!description) return null;
  const error = firstHopError ? proxyChainErrorText(t, { code: firstHopError }) : undefined;
  return <div data-chain-route={route} className="flex flex-col gap-2 text-help text-text-sub-600">
    <p className="m-0 break-words">{description}</p>
    {error && <p data-chain-first-attempt className="m-0 break-words">{t("proxyChain.firstAttemptFailed", { reason: error })}</p>}
  </div>;
}

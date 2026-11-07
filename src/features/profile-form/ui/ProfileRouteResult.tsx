import type { ProxyChainStage, ProxyChainTestResult } from "../../../entities/settings";
import { useT } from "../../../shared/i18n";
import { fmtCountry } from "../../../shared/lib/format";
import { fmtTs } from "../../../shared/lib/utils";
import { proxyChainErrorText } from "../../../shared/lib/proxyChainError";
import type { ProfileProxyRoute } from "../../../shared/model/proxyChain";
import { profileRouteForEditor } from "../../../shared/model/proxyChain";
import { ChainRouteSummary } from "../../../shared/ui/ChainRouteSummary";

export function ProfileRouteResult({ result, route: selectedRoute }: { result: ProxyChainTestResult; route?: ProfileProxyRoute }) {
  const t = useT();
  const route = profileRouteForEditor(selectedRoute);
  const scope = route?.mode === "proxy" ? "proxy" : route?.mode === "system" ? "system" : undefined;
  const errorText = (error?: ProxyChainStage | null) => proxyChainErrorText(t, error, scope);
  const firstAddressUsed = !route || route.mode === "system" || route.mode === "chain" && route.first.mode !== "direct";
  const assignedAddressUsed = !route || route.mode === "proxy" || route.mode === "chain" && route.second.mode === "proxy";
  const stage = (value?: ProxyChainStage | null) => value?.state === "ok"
    ? value.latency_ms != null ? t("proxyChain.stageLatency", { ms: value.latency_ms }) : t("proxyChain.stageOk")
    : value?.state === "failed" ? t("proxyChain.stageFailed") : t("profileRoute.stageUnused");
  const exitIp = result.exit_error ? null : result.exit_ip;
  const geoFailure = (phase?: string, httpStatus?: number) => {
    switch (phase) {
      case "http_status": return Number.isInteger(httpStatus) && httpStatus! >= 100 && httpStatus! <= 599
        ? t("profileRoute.geoHttpStatus", { status: httpStatus! }) : t("profileRoute.geoUnknown");
      case "timeout": return t("profileRoute.geoTimeout");
      case "connect": return t("profileRoute.geoConnect");
      case "tls": return t("profileRoute.geoTls");
      case "json": return t("profileRoute.geoJson");
      case "response_body": return t("profileRoute.geoResponseBody");
      case "redirect": return t("profileRoute.geoRedirect");
      case "configuration": return t("profileRoute.geoConfiguration");
      case "request": return t("profileRoute.geoRequest");
      case "provider_refused": return t("profileRoute.geoRefused");
      case "invalid_ip": return t("profileRoute.geoInvalidIp");
      default: return t("profileRoute.geoUnknown");
    }
  };
  const geo = result.exit_ip && !result.exit_error && result.exit_geo?.ip === result.exit_ip ? result.exit_geo : null;
  const location = geo ? [...new Set([geo.country_code ? fmtCountry(geo.country_code) : geo.country, geo.region, geo.city].filter(Boolean))].join(", ") : "";
  const rows = result.local_network ? [{ label: t("profileRoute.directLabel"), value: result.local_network }]
    : route?.mode === "proxy" ? [{ label: t("profileRoute.proxyLabel"), value: result.profile_proxy }]
    : route?.mode === "system" ? [{ label: t("profileRoute.systemLabel"), value: result.system_proxy }]
    : [{ label: t("profileRoute.firstLayer"), value: result.system_proxy }, { label: t("profileRoute.secondLayer"), value: result.profile_proxy }];
  return <div data-profile-route-result role="status" aria-live="polite" className="flex flex-col gap-2 text-help text-text-sub-600">
    <ChainRouteSummary route={result.route} firstHopError={result.first_hop_error_code} />
    <dl className="m-0 flex flex-col gap-2">
      {rows.map(({ label, value }) => <div key={label} className="flex flex-wrap gap-2">
        <dt>{label}</dt><dd className={`m-0 ${value?.state === "failed" ? "text-error-text" : "text-text-strong-950"}`}>
          {stage(value)}{errorText(value) && <span className="block break-words">{errorText(value)}</span>}
          {value?.state === "failed" && value.error_details && <span data-profile-route-protocol className="block break-words">
            {value.error_details.phase === "authentication"
              ? t("profileRoute.protocolAuthReply", { protocol: value.error_details.protocol === "socks5" ? "SOCKS5" : "HTTP", code: value.error_details.response_code })
              : t("profileRoute.protocolConnectReply", { protocol: value.error_details.protocol === "socks5" ? "SOCKS5" : "HTTP", code: value.error_details.response_code })}
            {value.error_details.phase === "connect" && value.error_details.target && <span className="block">{t("profileRoute.probeTarget", { target: value.error_details.target })}</span>}
          </span>}
        </dd>
      </div>)}
      {firstAddressUsed && result.proxy_address && <div className="flex flex-wrap gap-2"><dt>{route?.mode === "system" ? t("profileRoute.systemAddress") : t("proxyChain.firstProxyAddress")}</dt><dd className="mono m-0 break-all">{result.proxy_address}</dd></div>}
      {assignedAddressUsed && result.profile_proxy_address && <div className="flex flex-wrap gap-2"><dt>{route?.mode === "proxy" ? t("profileRoute.proxyAddress") : t("proxyChain.profileProxyAddress")}</dt><dd className="mono m-0 break-all">{result.profile_proxy_address}</dd></div>}
      <div className="flex flex-wrap gap-2" data-profile-route-exit><dt>{t("proxyChain.exitIp")}</dt><dd className="mono m-0 break-all">{exitIp || t("proxyChain.exitUnavailable")}</dd></div>
      <div className="flex flex-wrap gap-2" data-profile-route-location><dt>{t("proxyChain.exitLocation")}</dt><dd className="m-0 break-words">{location || (exitIp ? t("proxyChain.locationNotReturned") : t("proxyChain.exitUnavailable"))}</dd></div>
      {result.tested_at && <div className="flex flex-wrap gap-2"><dt>{t("proxyChain.testedAt")}</dt><dd className="m-0">{fmtTs(result.tested_at)}</dd></div>}
    </dl>
    {result.exit_error && <p className={`m-0 break-words ${result.route_ready === true ? "text-warning-text" : "text-error-text"}`}>{proxyChainErrorText(t, { code: result.exit_error_code, message: result.exit_error, route_ready: result.route_ready }, scope)}</p>}
    {result.exit_error && result.exit_error_details?.length ? <ul data-profile-route-geo-details className="m-0 flex list-none flex-col gap-2 p-0 text-error-text">
      {result.exit_error_details.map((failure, index) => <li key={index} className="break-words">
        {t("profileRoute.geoSourceFailure", {
          provider: ["ip-api.com", "ipapi.co", "ipwho.is"].includes(failure.provider) ? failure.provider : t("profileRoute.geoUnknownSource"),
          reason: geoFailure(failure.phase, failure.http_status),
        })}
      </li>)}
    </ul> : null}
  </div>;
}

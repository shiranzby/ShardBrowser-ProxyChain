import type { ProxyEntry, ProxyTestSnapshot } from "../../entities/proxy";
import { useT } from "../i18n";
import { fmtCountry } from "../lib/format";
import { CountryFlag } from "./CountryFlag";

export function proxyLocation(proxy: ProxyEntry, snapshot?: ProxyTestSnapshot) {
  const code = (snapshot?.country_code || proxy.country || "").trim().toUpperCase();
  if (!/^[A-Z]{2}$/.test(code) || code === "ZZ") return null;
  const region = snapshot?.country_code?.toUpperCase() === code ? snapshot : undefined;
  return { code, label: [fmtCountry(code), region?.region, region?.city].filter(Boolean).join(" / ") };
}

export function ProxyLocation({ location, unknown }: { location: ReturnType<typeof proxyLocation>; unknown?: string }) {
  if (!location) return <span className="shrink-0 text-text-soft-400">{unknown ?? "—"}</span>;
  return <span className="inline-flex min-w-0 items-center gap-1.5">
    <CountryFlag cc={location.code} height={14} />
    <span className="shrink-0 rounded-chip-sm bg-bg-weak-50 px-1 py-0.5 text-micro font-medium text-text-sub-600">{location.code}</span>
    <span className="truncate" title={location.label}>{location.label}</span>
  </span>;
}

export function proxySummaryLabel(proxy: ProxyEntry) {
  const host = proxy.host.includes(":") && !proxy.host.startsWith("[") ? `[${proxy.host}]` : proxy.host;
  const address = `${host}:${proxy.port}`;
  return `${proxy.name || address} (${proxy.kind.toUpperCase()} ${address})`;
}

/** Text used by searchable proxy selectors. It carries the same location
 * identity shown by ProxySummary, so a selector remains understandable when
 * its option renderer is collapsed to plain text. */
export function proxyOptionLabel(proxy: ProxyEntry, snapshot?: ProxyTestSnapshot, unknownLocation = "Unknown location") {
  const location = proxyLocation(proxy, snapshot);
  const summary = proxySummaryLabel(proxy);
  return location ? `${location.code} ${location.label} ${summary}` : `${unknownLocation} ${summary}`;
}

export function ProxySummary({ proxy, snapshot }: { proxy: ProxyEntry; snapshot?: ProxyTestSnapshot }) {
  const t = useT();
  const location = proxyLocation(proxy, snapshot);
  const label = proxySummaryLabel(proxy);
  return <span className="flex min-w-0 items-center gap-1.5" title={`${location?.label ?? t("proxyRow.unknownLocation")} ${label}`}>
    <ProxyLocation location={location} unknown={t("proxyRow.unknownLocation")} />
    <span className="min-w-0 truncate">{label}</span>
  </span>;
}

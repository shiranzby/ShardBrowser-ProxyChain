import { Checkbox, Input, Select } from "@proxyshard/shardx-ui-kit";
import { useT } from "../../../shared/i18n";
import type { TnbExtractRequest } from "../../../entities/proxy";
import { TnbGeoFields, type TnbGeo } from "./TnbGeoFields";

export type TnbDraft = Omit<TnbExtractRequest, "count" | "ttl" | "geo"> & TnbGeo & {
  count: string;
  ttl: string;
};

export const newTnbDraft = (): TnbDraft => ({
  server: "http://usdata.adv.tnbproxy.com:9090", plan_name: "advanced_traffic", kind: "http",
  count: "1", ttl: "0", pre: false, route: "direct", country: null, region: null, city: null,
  pool: "", isp: "", asn: "", ip: "", sid: "",
});

/** UI validation keeps a bad request editable; the backend validates it again. */
export function tnbRequest(draft: TnbDraft): TnbExtractRequest | null {
  const count = Number(draft.count), ttl = Number(draft.ttl);
  if (!/^\d+$/.test(draft.count) || !Number.isInteger(count) || count < 1 || count > 1000) return null;
  if (!/^\d+$/.test(draft.ttl) || !Number.isInteger(ttl) || ttl < 0 || ttl > 180) return null;
  let server: URL;
  try { server = new URL(draft.server.trim()); } catch { return null; }
  if (!["http:", "https:"].includes(server.protocol) || server.username || server.password || server.search || server.hash || !["/", "/gen"].includes(server.pathname)) return null;
  if (!/^[A-Za-z0-9_-]{1,64}$/.test(draft.plan_name.trim())) return null;
  const optional = [draft.country?.code, draft.region?.code, draft.city?.code, draft.pool, draft.isp, draft.asn, draft.ip, draft.sid];
  if (optional.some((s) => s !== undefined && (s.length > 256 || /\p{Cc}/u.test(s)))) return null;
  const country = draft.country?.code, region = draft.region?.code, city = draft.city?.code;
  if ((region && !country) || (city && !region)) return null;
  if ([country, region, city].filter(Boolean).join("_").length > 256) return null;
  return {
    server: draft.server.trim(), plan_name: draft.plan_name.trim(), kind: draft.kind,
    count, ttl, pre: draft.pre, route: draft.route,
    // TNB's public export formatter keeps selected levels and normalizes hyphens.
    geo: [country, region, city].filter((code): code is string => !!code).map((code) => code.replace(/-/g, "_")).join("_"),
    ...Object.fromEntries((["pool", "isp", "asn", "ip", "sid"] as const).map((key) => [key, draft[key]?.trim() || undefined])),
  };
}

export function tnbErrorText(t: (key: string) => string, error: unknown): string {
  switch (String(error instanceof Error ? error.message : error)) {
    case "tnb_invalid_request": return t("tnb.errorInvalid");
    case "tnb_timeout": return t("tnb.errorTimeout");
    case "tnb_route_failed": return t("tnb.errorRoute");
    case "tnb_http_status": return t("tnb.errorHttp");
    case "tnb_response_too_large": return t("tnb.errorLarge");
    case "tnb_invalid_response": return t("tnb.errorResponse");
    case "tnb_no_proxies": return t("tnb.errorEmpty");
    case "tnb_busy": return t("tnb.errorBusy");
    default: return t("tnb.errorRequest");
  }
}

export function TnbExtractForm({ value, onChange, disabled, error }: {
  value: TnbDraft;
  onChange: (next: TnbDraft) => void;
  disabled: boolean;
  error: string | null;
}) {
  const t = useT();
  const set = <K extends keyof TnbDraft>(key: K, next: TnbDraft[K]) => onChange({ ...value, [key]: next });
  return (
    <div className="flex flex-col gap-3" data-tnb-form>
      <div className="grid grid-cols-2 gap-3">
        <Input label={t("tnb.server")} inputSize="xsmall" value={value.server} disabled={disabled} onChange={(e) => onChange({ ...value, server: e.target.value, country: null, region: null, city: null })} />
        <Input label={t("tnb.plan")} inputSize="xsmall" value={value.plan_name} disabled={disabled} onChange={(e) => set("plan_name", e.target.value)} />
        <Select label={t("tnb.protocol")} size="xsmall" value={value.kind} disabled={disabled} onChange={(v) => set("kind", v as TnbDraft["kind"])} options={[{ value: "http", label: "HTTP" }, { value: "socks5", label: "SOCKS5" }]} />
        <Input label={t("tnb.count")} inputSize="xsmall" type="number" min={1} max={1000} step={1} value={value.count} disabled={disabled} onChange={(e) => set("count", e.target.value)} />
      </div>
      <TnbGeoFields server={value.server} route={value.route} value={value} disabled={disabled} onChange={(geo) => onChange({ ...value, ...geo })} />
      <p className="m-0 text-secondary text-text-soft-400">{t("tnbGeo.hint")}</p>
      <div className="grid grid-cols-2 gap-3">
        <Input label={t("tnb.ttl")} hint={t("tnb.ttlHint")} inputSize="xsmall" type="number" min={0} max={180} step={1} value={value.ttl} disabled={disabled} onChange={(e) => set("ttl", e.target.value)} />
        <Select label={t("tnb.route")} size="xsmall" value={value.route} disabled={disabled} onChange={(v) => set("route", v as TnbDraft["route"])} options={[{ value: "direct", label: t("tnb.direct") }, { value: "first_hop", label: t("tnb.firstHop") }]} />
      </div>
      <p className="m-0 text-secondary text-text-sub-600">{value.route === "first_hop" ? t("tnb.firstHopHint") : t("tnb.directHint")}</p>
      <p className="m-0 text-secondary text-text-sub-600">{t("tnb.whitelist")}</p>
      <details>
        <summary className="cursor-pointer text-ui text-text-sub-600">{t("tnb.advanced")}</summary>
        <div className="mt-3 grid grid-cols-2 gap-3">
          <Input label={t("tnb.pool")} inputSize="xsmall" value={value.pool ?? ""} disabled={disabled} onChange={(e) => set("pool", e.target.value)} />
          <Input label={t("tnb.isp")} inputSize="xsmall" value={value.isp ?? ""} disabled={disabled} onChange={(e) => set("isp", e.target.value)} />
          <Input label={t("tnb.asn")} inputSize="xsmall" value={value.asn ?? ""} disabled={disabled} onChange={(e) => set("asn", e.target.value)} />
          <Input label={t("tnb.ip")} inputSize="xsmall" value={value.ip ?? ""} disabled={disabled} onChange={(e) => set("ip", e.target.value)} />
          <Input label={t("tnb.sid")} inputSize="xsmall" value={value.sid ?? ""} disabled={disabled} onChange={(e) => set("sid", e.target.value)} />
          <Checkbox label={t("tnb.pre")} checked={value.pre} disabled={disabled} onCheckedChange={(checked) => set("pre", checked)} />
        </div>
      </details>
      {error && <p role="alert" className="m-0 text-secondary text-error-text">{error}</p>}
    </div>
  );
}

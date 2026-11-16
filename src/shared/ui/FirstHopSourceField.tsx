import { useProxy } from "../../entities/proxy";
import { useEffect } from "react";
import type { ProxyChainSettings } from "../../entities/settings";
import { useT } from "../i18n";
import { SelectField } from "./SelectField";

/** Shared global first-hop choice. A library reference never copies credentials into settings. */
export function FirstHopSourceField({ config, onChange, disabled = false }: {
  config: ProxyChainSettings; onChange: (patch: Partial<ProxyChainSettings>) => void; disabled?: boolean;
}) {
  const t = useT();
  const proxies = useProxy((s) => s.proxies);
  const proxyStatus = useProxy((s) => s.status);
  useEffect(() => { void useProxy.getState().init(); }, []);
  return <div className="flex min-w-0 flex-col gap-2" data-global-first-hop>
    <p className="m-0 text-help text-text-sub-600">{t("proxyChain.firstSource")}</p>
    <SelectField aria-label={t("proxyChain.mode")} value={config.mode} disabled={disabled}
      options={[
        { value: "auto", label: t("proxyChain.modeAuto") },
        { value: "selected", label: t("proxyChain.modeSelected") },
        { value: "http", label: t("proxyChain.modeHttp") },
        { value: "https", label: t("proxyChain.modeHttps") },
        { value: "socks5", label: t("proxyChain.modeSocks") },
      ]}
      onChange={(mode: ProxyChainSettings["mode"]) => onChange({ mode,
        ...(mode === "selected" ? { selected_proxy_id: config.selected_proxy_id || proxies[0]?.id || "" } : {}) })} />
    {config.mode === "selected" && proxyStatus === "loading" && <p className="m-0 text-help text-text-sub-600">{t("proxyChain.loadingFirst")}</p>}
    {config.mode === "selected" && proxyStatus !== "loading" && <SelectField aria-label={t("proxyChain.selectedFirst")}
      disabled={disabled} value={config.selected_proxy_id ?? ""}
      options={[{ value: "", label: t("proxyChain.selectFirst") }, ...proxies.map((p) => ({ value: p.id, label: `${p.name || p.host} (${p.kind.toUpperCase()} ${p.host}:${p.port})` }))]}
      onChange={(selected_proxy_id: string) => onChange({ selected_proxy_id })} />}
    {config.mode === "selected" && proxyStatus === "ready" && proxies.length === 0 && <p className="m-0 text-help text-warning-text">{t("proxyChain.noFirstProxy")}</p>}
    <p className="m-0 text-help text-text-sub-600">{t("proxyChain.defaultFirstHelp")}</p>
  </div>;
}

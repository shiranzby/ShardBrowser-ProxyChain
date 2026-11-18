import { useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Button, Input } from "@proxyshard/shardx-ui-kit";
import { providerDefaultsGet, providerDefaultsSave, type ProviderDefaults, type ProviderUpdate } from "../../../entities/proxy";
import { SelectField } from "../../../shared/ui/SelectField";
import { useT } from "../../../shared/i18n";
import { toast } from "../../../shared/model/toast";

/** Explicit save: provider defaults never change existing proxy bindings. */
export function ProviderDefaultsPanel({ provider = "all", onSaved }: { provider?: "all" | "1024proxy" | "tnbproxy" | "custom"; onSaved?: (value: ProviderDefaults) => void }) {
  const t = useT();
  const [value, setValue] = useState<ProviderDefaults | null>(null);
  const [secrets, setSecrets] = useState<Pick<ProviderUpdate, "password" | "tnb_api_password" | "tnb_proxy_password" | "custom_api_password" | "custom_proxy_password">>({});
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState(false);
  const [saved, setSaved] = useState(false);
  useEffect(() => { let live = true; void providerDefaultsGet().then((v) => { if (live) setValue(v); }).catch(() => { if (live) setError(true); }); return () => { live = false; }; }, []);
  const patch = (part: Partial<ProviderDefaults>) => { setValue((v) => v ? { ...v, ...part } : v); setSaved(false); };
  const save = async () => {
    if (!value || busy) return;
    setBusy(true); setError(false); setSaved(false);
    try { const next = await providerDefaultsSave({ ...value, ...secrets }); setValue(next); setSecrets({}); setSaved(true); onSaved?.(next); }
    catch { setError(true); }
    finally { setBusy(false); }
  };
  const website = (url: string) => <a href={url} className="text-primary-text underline underline-offset-2" onClick={(e) => { e.preventDefault(); void openUrl(url).catch(() => toast.err(t("provider.openWebsiteFailed"))); }}>{url.replace(/^https?:\/\//, "").replace(/\/$/, "")}</a>;
  const password = (key: "password" | "tnb_api_password" | "tnb_proxy_password" | "custom_api_password" | "custom_proxy_password", has: boolean, label: string) => <div className="flex flex-col gap-1.5">
    <Input secret type="password" inputSize="xsmall" label={label} value={secrets[key] ?? ""} autoComplete="new-password" placeholder={has ? t("provider.keepPassword") : ""} onChange={(e) => { setSecrets((v) => ({ ...v, [key]: e.target.value })); setSaved(false); }} />
    {has && <Button variant="neutral" mode="ghost" size="3xsmall" onClick={() => { setSecrets((v) => ({ ...v, [key]: "" })); setSaved(false); }}>{secrets[key] === "" ? t("provider.willClear") : t("provider.clearPassword")}</Button>}
  </div>;
  const [selected, setSelected] = useState<"1024proxy" | "tnbproxy" | "custom">(provider === "1024proxy" ? "1024proxy" : provider === "custom" ? "custom" : "tnbproxy");
  const active = provider === "all" ? selected : provider;
  return <fieldset disabled={busy} className="m-0 min-w-0 border-0 p-0" data-provider-defaults>
    <div className="flex flex-col gap-4">
      <p className="m-0 text-help text-text-sub-600">{t("provider.defaultsHelp")}</p>
      {value && <>
        {provider === "all" && <div className="flex items-center gap-1 border-b border-stroke-soft-200 pb-2" role="tablist" aria-label={t("provider.managementTitle")}>
          <Button type="button" variant={active === "tnbproxy" ? "primary" : "neutral"} mode={active === "tnbproxy" ? "filled" : "ghost"} size="xsmall" role="tab" aria-selected={active === "tnbproxy"} data-provider-tab="tnbproxy" onClick={() => setSelected("tnbproxy")}>{t("tnb.provider")}</Button>
          <Button type="button" variant={active === "1024proxy" ? "primary" : "neutral"} mode={active === "1024proxy" ? "filled" : "ghost"} size="xsmall" role="tab" aria-selected={active === "1024proxy"} data-provider-tab="1024proxy" onClick={() => setSelected("1024proxy")}>{t("provider.name1024")}</Button>
          <Button type="button" variant={active === "custom" ? "primary" : "neutral"} mode={active === "custom" ? "filled" : "ghost"} size="xsmall" role="tab" aria-selected={active === "custom"} data-provider-tab="custom" onClick={() => setSelected("custom")}>{t("provider.custom")}</Button>
          {active === "tnbproxy" ? website("https://dash.tnbproxy.com") : active === "1024proxy" ? website("https://1024proxy.io") : value.custom_website ? website(value.custom_website) : null}
        </div>}
        {active === "custom" ? <div className="flex flex-col gap-3" role="tabpanel" data-provider-panel="custom">
          <div className="flex items-center justify-between gap-3"><h3 className="m-0 text-section-label text-text-strong-950">{value.custom_website ? website(value.custom_website) : t("provider.custom")}</h3>{value.custom_whitelist ? <a href={value.custom_whitelist} className="text-primary-text underline underline-offset-2" onClick={(e) => { e.preventDefault(); void openUrl(value.custom_whitelist).catch(() => toast.err(t("provider.openWebsiteFailed"))); }}>{value.custom_whitelist.replace(/^https?:\/\//, "").replace(/\/$/, "")}</a> : null}</div>
          <div className="grid grid-cols-2 gap-3"><Input inputSize="xsmall" label={t("provider.customName")} value={value.custom_name} onChange={(e) => patch({ custom_name: e.target.value })} /><Input inputSize="xsmall" label={t("provider.customServer")} value={value.custom_server} onChange={(e) => patch({ custom_server: e.target.value })} /></div>
          <div className="grid grid-cols-2 gap-3"><Input inputSize="xsmall" label={t("provider.customPlan")} value={value.custom_plan} onChange={(e) => patch({ custom_plan: e.target.value })} /><SelectField label={t("provider.customProtocol")} value={value.custom_kind} onChange={(custom_kind) => patch({ custom_kind })} options={[{ value: "http" as const, label: "HTTP" }, { value: "https" as const, label: "HTTPS" }, { value: "socks5" as const, label: "SOCKS5" }]} /></div>
          <div className="grid grid-cols-2 gap-3"><Input inputSize="xsmall" label={t("provider.customWebsite")} value={value.custom_website} onChange={(e) => patch({ custom_website: e.target.value })} /><Input inputSize="xsmall" label={t("provider.customWhitelist")} value={value.custom_whitelist} onChange={(e) => patch({ custom_whitelist: e.target.value })} /></div>
          <p className="m-0 text-help text-text-sub-600">{t("provider.customHelp")}</p>
          <div className="grid grid-cols-2 gap-3"><Input inputSize="xsmall" label={t("provider.apiUsername")} value={value.custom_api_username} onChange={(e) => patch({ custom_api_username: e.target.value })} />{password("custom_api_password", value.custom_api_password_saved, t("provider.apiPassword"))}</div>
          <div className="grid grid-cols-2 gap-3"><Input inputSize="xsmall" label={t("provider.proxyUsername")} value={value.custom_proxy_username} onChange={(e) => patch({ custom_proxy_username: e.target.value })} />{password("custom_proxy_password", value.custom_proxy_password_saved, t("provider.proxyPassword"))}</div>
        </div> : active === "1024proxy" ? <div className="flex flex-col gap-3" role="tabpanel" data-provider-panel="1024proxy">
          <div className="flex items-center justify-between gap-3"><h3 className="m-0 text-section-label text-text-strong-950">{website("https://1024proxy.io")}</h3></div>
          <div className="grid grid-cols-2 gap-3">
            <Input inputSize="xsmall" label={t("provider.relay")} value={value.relay} onChange={(e) => patch({ relay: e.target.value })} />
            <Input inputSize="xsmall" label={t("provider.account")} value={value.username} onChange={(e) => patch({ username: e.target.value })} />
          </div>
          <div className="grid grid-cols-2 gap-3">
            {password("password", value.password_saved, t("proxyEditor.passwordLabel"))}
            <Input inputSize="xsmall" label={t("provider.template")} value={value.username_template} onChange={(e) => patch({ username_template: e.target.value })} />
          </div>
          <p className="m-0 text-help text-text-sub-600">{t("provider.templateHelp")}</p>
        </div> : <div className="flex flex-col gap-3" role="tabpanel" data-provider-panel="tnbproxy">
          <div className="flex items-center justify-between gap-3"><h3 className="m-0 text-section-label text-text-strong-950">{website("https://dash.tnbproxy.com")}</h3></div>
          <div className="grid grid-cols-2 gap-3">
            <Input inputSize="xsmall" label={t("tnb.server")} value={value.tnb_server} onChange={(e) => patch({ tnb_server: e.target.value })} />
            <Input inputSize="xsmall" label={t("tnb.plan")} value={value.tnb_plan} onChange={(e) => patch({ tnb_plan: e.target.value })} />
          </div>
          <div className="grid grid-cols-2 gap-3">
            <SelectField label={t("tnb.protocol")} value={value.tnb_kind} onChange={(tnb_kind) => patch({ tnb_kind })} options={[{ value: "http" as const, label: "HTTP" }, { value: "socks5" as const, label: "SOCKS5" }]} />
            <Input inputSize="xsmall" label={t("provider.apiUsername")} value={value.tnb_api_username} onChange={(e) => patch({ tnb_api_username: e.target.value })} />
          </div>
          <p className="m-0 text-help text-text-sub-600">{t("provider.authSeparation")}</p>
          <div className="grid grid-cols-2 gap-3">
            {password("tnb_api_password", value.tnb_api_password_saved, t("provider.apiPassword"))}
            <Input inputSize="xsmall" label={t("provider.proxyUsername")} value={value.tnb_proxy_username} onChange={(e) => patch({ tnb_proxy_username: e.target.value })} />
          </div>
          <div className="max-w-[50%]">{password("tnb_proxy_password", value.tnb_proxy_password_saved, t("provider.proxyPassword"))}</div>
        </div>}
        <div className="flex items-center gap-3"><Button variant="neutral" mode="stroke" size="xsmall" isLoading={busy} onClick={() => void save()}>{t("provider.saveDefaults")}</Button>{saved && <span role="status" className="text-help text-text-sub-600">{t("provider.saved")}</span>}</div>
      </>}
      {error && <p role="alert" className="m-0 text-help text-error-text">{t("provider.saveFailed")}</p>}
    </div>
  </fieldset>;

}

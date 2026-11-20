import { useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Button, Input } from "@proxyshard/shardx-ui-kit";
import { providerDefaultsGet, providerDefaultsSave, providerDeleteCustom, ProxyFolderSelect, type ProviderDefaults, type ProviderUpdate } from "../../../entities/proxy";
import { SelectField } from "../../../shared/ui/SelectField";
import { useT } from "../../../shared/i18n";
import { toast } from "../../../shared/model/toast";

/** Explicit save: provider defaults never change existing proxy bindings. */
type Provider = "1024proxy" | "tnbproxy" | "custom";
type PanelProps = { provider?: "all" | Provider; customProviderId?: string; onSaved?: (value: ProviderDefaults) => void };

export function ProviderDefaultsPanel({ provider = "all", customProviderId, onSaved }: PanelProps) {
  return <div data-provider-defaults>
    {provider === "all" ? <ProviderRegistry onSaved={onSaved} /> : <ProviderFields key={customProviderId ?? provider} provider={provider} customProviderId={customProviderId} onSaved={onSaved} />}
  </div>;
}

function ProviderRegistry({ onSaved }: Pick<PanelProps, "onSaved">) {
  const t = useT();
  const [defaults, setDefaults] = useState<ProviderDefaults | null>(null);
  const [selected, setSelected] = useState<string>("tnbproxy");
  const [dirty, setDirty] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState(false);
  useEffect(() => {
    let live = true;
    void providerDefaultsGet().then((value) => { if (live) setDefaults(value); })
      .catch(() => { if (live) setError(true); });
    return () => { live = false; };
  }, []);
  const select = (id: string) => {
    if (id === selected || (dirty && !window.confirm(t("provider.discardChanges")))) return;
    setSelected(id);
    setDirty(false);
    setError(false);
  };
  const items = defaults?.custom_providers ?? [];
  const current = items.find((item) => item.id === selected);
  const customId = selected === "tnbproxy" || selected === "1024proxy" || selected === "custom" ? undefined : selected;
  const remove = async () => {
    if (!current || !window.confirm(t("provider.deleteConfirm", { name: current.name }))) return;
    setBusy(true);
    setError(false);
    try {
      await providerDeleteCustom(current.id);
      setDefaults((value) => value && ({ ...value, custom_providers: value.custom_providers.filter((item) => item.id !== current.id) }));
      setSelected("tnbproxy");
      setDirty(false);
    } catch { setError(true); }
    finally { setBusy(false); }
  };
  const tab = (id: string, name: string) => <Button key={id} type="button" disabled={busy} variant={selected === id ? "primary" : "neutral"} mode={selected === id ? "filled" : "ghost"} size="xsmall" role="tab" aria-selected={selected === id} data-provider-tab={id} onClick={() => select(id)}>{name}</Button>;
  return <div className="flex min-w-0 flex-col gap-3">
    <div className="flex min-w-0 items-center gap-2 border-b border-stroke-soft-200 pb-2">
      <div className="flex min-w-0 items-center gap-1 overflow-x-auto" role="tablist" aria-label={t("provider.managementTitle")}>
        {tab("tnbproxy", t("tnb.provider"))}
        {tab("1024proxy", t("provider.name1024"))}
        {(defaults?.custom_server || defaults?.custom_extract_url) && tab("custom", defaults.custom_name || t("provider.custom"))}
        {items.map(({ id, name }) => tab(id, name))}
        {customId && !current && tab(customId, t("provider.newCustom"))}
      </div>
      <Button type="button" variant="neutral" mode="stroke" size="xsmall" disabled={busy || !defaults} onClick={() => select(crypto.randomUUID())}>{t("provider.addCustom")}</Button>
    </div>
    {error && <p role="alert" className="m-0 text-help text-error-text">{current ? t("provider.deleteFailed") : t("provider.defaultsUnreadable")}</p>}
    {defaults && <ProviderFields key={selected} provider={selected === "1024proxy" || selected === "tnbproxy" ? selected : "custom"} customProviderId={customId}
      create={Boolean(customId && !current)} onDirty={setDirty} onBusy={setBusy} onDelete={current ? remove : undefined}
      onSaved={(saved) => {
        setDefaults((value) => customId && value ? { ...value, custom_providers: saved.custom_providers } : saved);
        onSaved?.(saved);
      }} />}
  </div>;
}

function ProviderFields({ provider: active, customProviderId, create = false, onSaved, onDirty, onBusy, onDelete }: {
  provider: Provider; customProviderId?: string; create?: boolean; onSaved?: PanelProps["onSaved"];
  onDirty?: (dirty: boolean) => void; onBusy?: (busy: boolean) => void; onDelete?: () => void;
}) {
  const t = useT();
  const [value, setValue] = useState<ProviderDefaults | null>(null);
  const [secrets, setSecrets] = useState<Pick<ProviderUpdate, "password" | "tnb_api_password" | "tnb_proxy_password" | "custom_api_password" | "custom_proxy_password">>({});
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState(false);
  const [saved, setSaved] = useState(false);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const tnb = active === "tnbproxy";
  useEffect(() => {
    let live = true;
    void providerDefaultsGet({ customProviderId, create, tnb }).then((v) => { if (live) setValue(v); }).catch((error) => {
      if (!live) return;
      setError(true);
      setErrorMessage(t(String(error).includes("provider_not_found") ? "provider.notFound" : "provider.defaultsUnreadable"));
    });
    return () => { live = false; };
  }, [customProviderId, create, tnb, t]);
  const edited = () => { setSaved(false); onDirty?.(true); };
  const patch = (part: Partial<ProviderDefaults>) => { setValue((v) => v ? { ...v, ...part } : v); edited(); };
  const errorText = (error: unknown) => {
    const code = String(error);
    if (code.includes("provider_defaults_invalid")) return t("provider.defaultsInvalid");
    if (code.includes("provider_invalid_extract_url")) return t("provider.invalidExtractUrl");
    if (code.includes("provider_invalid_relay")) return t("provider.invalidGateway");
    if (code.includes("provider_credentials_unavailable")) return t("provider.credentialsUnavailable");
    if (code.includes("provider_defaults_busy")) return t("provider.defaultsBusy");
    if (code.includes("provider_defaults_unreadable")) return t("provider.defaultsUnreadable");
    if (code.includes("provider_not_found")) return t("provider.notFound");
    return t("provider.saveFailed");
  };
  const save = async () => {
    if (!value || busy) return;
    setBusy(true); onBusy?.(true); setError(false); setErrorMessage(null); setSaved(false);
    try { const next = await providerDefaultsSave({ ...value, ...secrets }, { customProviderId, create, tnb }); setValue(next); setSecrets({}); setSaved(true); onDirty?.(false); onSaved?.(next); }
    catch (error) { setError(true); setErrorMessage(errorText(error)); }
    finally { setBusy(false); onBusy?.(false); }
  };
  const website = (url: string, name: string) => <a href={url} className="text-primary-text underline underline-offset-2" onClick={(e) => { e.preventDefault(); void openUrl(url).catch(() => toast.err(t("provider.openWebsiteFailed"))); }}>{name}</a>;
  const password = (key: "password" | "tnb_api_password" | "tnb_proxy_password" | "custom_api_password" | "custom_proxy_password", has: boolean, label: string) => <div className="flex flex-col gap-1.5">
    <Input secret type="password" inputSize="xsmall" label={label} value={secrets[key] ?? ""} autoComplete="new-password" placeholder={has ? t("provider.keepPassword") : ""} onChange={(e) => { setSecrets((v) => ({ ...v, [key]: e.target.value })); edited(); }} />
    {has && <Button variant="neutral" mode="ghost" size="3xsmall" onClick={() => { setSecrets((v) => ({ ...v, [key]: "" })); edited(); }}>{secrets[key] === "" ? t("provider.willClear") : t("provider.clearPassword")}</Button>}
  </div>;
  const custom = active === "custom";
  const displayName = custom ? value?.custom_name || t("provider.custom") : t(tnb ? "tnb.provider" : "provider.name1024");
  const websiteUrl = custom ? value?.custom_website : tnb ? "https://dash.tnbproxy.com" : "https://1024proxy.io";
  const gatewayKey = custom ? "custom_server" : tnb ? "tnb_server" : "relay";
  const usernameKey = custom ? "custom_proxy_username" : tnb ? "tnb_proxy_username" : "username";
  const passwordKey = custom ? "custom_proxy_password" : tnb ? "tnb_proxy_password" : "password";
  const passwordSavedKey = custom ? "custom_proxy_password_saved" : tnb ? "tnb_proxy_password_saved" : "password_saved";
  const apiUsernameKey = custom ? "custom_api_username" : "tnb_api_username";
  const apiPasswordKey = custom ? "custom_api_password" : "tnb_api_password";
  const apiPasswordSavedKey = custom ? "custom_api_password_saved" : "tnb_api_password_saved";
  return <fieldset disabled={busy} className="m-0 min-w-0 border-0 p-0">
    <div className="flex flex-col gap-4">
      <p className="m-0 text-help text-text-sub-600">{t("provider.defaultsHelp")}</p>
      {value && <>
        <div key={active} className="flex max-w-3xl flex-col gap-3" role="tabpanel" data-provider-panel={active}>
          <h3 className="m-0 text-section-label text-text-strong-950">{websiteUrl ? website(websiteUrl, displayName) : displayName}</h3>
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
            <Input inputSize="xsmall" label={t("provider.name")} value={value.name} placeholder={t("provider.namePlaceholder")} onChange={(e) => patch({ name: e.target.value })} />
            <ProxyFolderSelect value={value.folder} onChange={(folder) => patch({ folder })} disabled={busy} allowCreate={false} />
            <Input inputSize="xsmall" label={t("provider.country")} value={value.country} placeholder={t("provider.countryPlaceholder")} onChange={(e) => patch({ country: e.target.value.toUpperCase() })} />
            <Input inputSize="xsmall" label={t("provider.state")} value={value.state} onChange={(e) => patch({ state: e.target.value })} />
            <Input inputSize="xsmall" label={t("provider.city")} value={value.city} onChange={(e) => patch({ city: e.target.value })} />
          </div>
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
            <SelectField label={t("provider.sessionType")} value={value.session_type} onChange={(session_type) => patch({ session_type })} options={[
              { value: "sticky" as const, label: t("provider.sessionSticky") },
              { value: "rotating" as const, label: t("provider.sessionRotating") },
              { value: "per_request" as const, label: t("provider.sessionPerRequest") },
            ]} />
            <Input inputSize="xsmall" type="number" label={t("provider.sessionMinutes")} value={String(value.session_minutes)} onChange={(e) => patch({ session_minutes: Math.max(1, Number(e.target.value) || 1) })} />
            <Input inputSize="xsmall" type="number" label={t("provider.generationCount")} value={String(value.generation_count)} onChange={(e) => patch({ generation_count: Math.max(1, Number(e.target.value) || 1) })} />
          </div>
          {custom && <Input inputSize="xsmall" label={t("provider.customName")} value={value.custom_name} onChange={(e) => patch({ custom_name: e.target.value })} />}
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
            <Input inputSize="xsmall" label={(custom ? t("provider.customServer") : tnb ? t("tnb.server") : t("provider.relay"))} value={value[gatewayKey]} onChange={(e) => patch({ [gatewayKey]: e.target.value })} />
            {custom ? <SelectField label={t("provider.customProtocol")} value={value.custom_kind} onChange={(custom_kind) => patch({ custom_kind })} options={[{ value: "http" as const, label: "HTTP" }, { value: "https" as const, label: "HTTPS" }, { value: "socks5" as const, label: "SOCKS5" }]} />
              : tnb ? <SelectField label={t("tnb.protocol")} value={value.tnb_kind} onChange={(tnb_kind) => patch({ tnb_kind })} options={[{ value: "http" as const, label: "HTTP" }, { value: "socks5" as const, label: "SOCKS5" }]} />
                : <Input inputSize="xsmall" label={t("tnb.protocol")} value="SOCKS5" readOnly />}
          </div>
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
            <Input inputSize="xsmall" label={(tnb || custom ? t("provider.proxyUsername") : t("provider.account"))} value={value[usernameKey]} onChange={(e) => patch({ [usernameKey]: e.target.value })} />
            {password(passwordKey, value[passwordSavedKey], (tnb || custom ? t("provider.proxyPassword") : t("proxyEditor.passwordLabel")))}
          </div>
          <details>
            <summary className="cursor-pointer py-1 text-secondary text-text-sub-600">{t("provider.advancedOptions")}</summary>
            <div className="flex flex-col gap-3 pt-2">
              {custom || tnb ? <>
                {custom
                  ? <Input inputSize="xsmall" label={t("provider.customExtractUrl")} value={value.custom_extract_url ?? ""} onChange={(e) => patch({ custom_extract_url: e.target.value })} />
                  : <Input inputSize="xsmall" label={t("tnb.plan")} value={value.tnb_plan} onChange={(e) => patch({ tnb_plan: e.target.value })} />}
                <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                  <Input inputSize="xsmall" label={t("provider.apiUsername")} value={value[apiUsernameKey]} onChange={(e) => patch({ [apiUsernameKey]: e.target.value })} />
                  {password(apiPasswordKey, value[apiPasswordSavedKey], t("provider.apiPassword"))}
                </div>
                {custom && <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                  <Input inputSize="xsmall" label={t("provider.customWebsite")} value={value.custom_website} onChange={(e) => patch({ custom_website: e.target.value })} />
                  <Input inputSize="xsmall" label={t("provider.customWhitelist")} value={value.custom_whitelist} onChange={(e) => patch({ custom_whitelist: e.target.value })} />
                </div>}
                <p className="m-0 text-help text-text-sub-600">{(custom ? t("provider.customHelp") : t("provider.authSeparation"))}</p>
              </> : <>
                <Input inputSize="xsmall" label={t("provider.template")} value={value.username_template} onChange={(e) => patch({ username_template: e.target.value })} />
                <p className="m-0 text-help text-text-sub-600">{t("provider.templateHelp")}</p>
              </>}
            </div>
          </details>
        </div>
        <div className="flex items-center gap-3"><Button variant="neutral" mode="stroke" size="xsmall" isLoading={busy} onClick={() => void save()}>{t("provider.saveDefaults")}</Button>{onDelete && <Button variant="neutral" mode="ghost" size="xsmall" onClick={onDelete}>{t("provider.deleteCustom")}</Button>}{saved && <span role="status" className="text-help text-text-sub-600">{t("provider.saved")}</span>}</div>
      </>}
      {error && <p role="alert" className="m-0 text-help text-error-text">{errorMessage ?? t("provider.saveFailed")}</p>}
    </div>
  </fieldset>;

}

import { useEffect, useRef, useState } from "react";
import { Button, Input, Switch } from "@proxyshard/shardx-ui-kit";
import {
  defaultProxyChain, proxyChainDetect, proxyChainTest,
  type ProxyChainDetection, type ProxyChainSettings, type ProxyChainStage,
  type ProxyChainTestKind, type ProxyChainTestResult,
} from "../../../entities/settings";
import { may, useProfile } from "../../../entities/profile";
import { useSpace } from "../../../entities/space";
import { useT, appLocale } from "../../../shared/i18n";
import { explainError } from "../../../shared/lib/errors";
import { proxyChainErrorText } from "../../../shared/lib/proxyChainError";
import { fmtTs } from "../../../shared/lib/utils";
import { fmtCountry } from "../../../shared/lib/format";
import { NumField } from "../../../shared/ui/NumField";
import { SelectField } from "../../../shared/ui/SelectField";
import { SettingRow, SettingsGroup, type SettingsProps } from "../SettingRow";
import { holdSave } from "../useSettingsDraft";

const DEFAULT = defaultProxyChain();
type Busy = ProxyChainTestKind | "detect";

export function ProxySection({ s, setS }: SettingsProps) {
  const t = useT();
  const config = s.proxy_chain ?? DEFAULT;
  const profiles = useProfile((st) => st.profiles);
  const spaceEpoch = useSpace((st) => st.state?.epoch);
  const [profileId, setProfileId] = useState("");
  const selected = profiles.find((p) => p.id === profileId && may(p, "start"));
  const context = JSON.stringify([config, selected?.id, spaceEpoch]);
  const sequence = useRef(0);
  const [busy, setBusy] = useState<{ kind: Busy; context: string } | null>(null);
  const [result, setResult] = useState<{ value: ProxyChainTestResult; context: string } | null>(null);
  const [detection, setDetection] = useState<{ value: ProxyChainDetection; context: string } | null>(null);
  const [error, setError] = useState<{ text: string; context: string } | null>(null);
  // Neither a reply for an older draft nor one from another space may replace this page's facts.
  useEffect(() => () => { sequence.current++; }, [context]);
  const pending = busy?.context === context ? busy.kind : null;
  const shown = result?.context === context ? result.value : null;
  const detected = detection?.context === context ? detection.value : null;
  const detectionError = proxyChainErrorText(t, detected);
  const failure = error?.context === context ? error.text : null;
  const patch = (value: Partial<ProxyChainSettings>) => {
    sequence.current++;
    setS({ ...s, proxy_chain: { ...config, ...value, invalid: false } });
  };

  const detect = async () => {
    const request = ++sequence.current;
    setBusy({ kind: "detect", context });
    setError(null);
    try {
      const value = await proxyChainDetect(selected?.id);
      if (sequence.current === request) setDetection({ value, context });
    } catch (e) {
      if (sequence.current === request) setError({ text: explainError(e) ?? String(e), context });
    } finally {
      if (sequence.current === request) setBusy(null);
    }
  };

  const test = async (kind: ProxyChainTestKind) => {
    const request = ++sequence.current;
    setBusy({ kind, context });
    setError(null);
    setResult(null);
    try {
      const value = await proxyChainTest(kind, config, kind === "local" ? undefined : selected?.id);
      if (sequence.current === request) setResult({ value, context });
    } catch (e) {
      if (sequence.current === request) setError({ text: explainError(e) ?? String(e), context });
    } finally {
      if (sequence.current === request) setBusy(null);
    }
  };

  return (
    <div className="flex flex-col gap-[32px]">
      <SettingsGroup title={t("proxyChain.connectionTitle")}>
        <SettingRow
          id="proxy-chain-enabled"
          title={t("proxyChain.enable")}
          help={t("proxyChain.enableHelp")}
          control={({ labelId, helpId }) => (
            <Switch aria-labelledby={labelId} aria-describedby={helpId} checked={config.enabled} onChange={(enabled) => patch({ enabled })} />
          )}
          below={<p className="m-0 text-help text-text-sub-600">{t("proxyChain.nextStart")}</p>}
        />
        <div className="pb-3 text-help text-text-sub-600" data-chain-tcp-note>{t("proxyChain.tcpOnly")}</div>
      </SettingsGroup>

      <SettingsGroup title={t("proxyChain.configurationTitle")}>
        <SettingRow
          id="proxy-chain-mode"
          title={t("proxyChain.mode")}
          control={({ labelId, helpId }) => (
            <div className="w-[240px]">
              <SelectField
                aria-labelledby={labelId} aria-describedby={helpId} value={config.mode}
                onChange={(mode) => patch({ mode })}
                options={[
                  { value: "auto", label: t("proxyChain.modeAuto") },
                  { value: "http", label: t("proxyChain.modeHttp") },
                  { value: "socks5", label: t("proxyChain.modeSocks") },
                ]}
              />
            </div>
          )}
        />
        {config.mode === "auto" ? (
          <SettingRow
            id="proxy-chain-detect"
            title={t("proxyChain.windowsSettings")}
            help={t("proxyChain.detectHelp")}
            control={() => (
              <Button variant="neutral" mode="stroke" size="xsmall" disabled={!!pending} isLoading={pending === "detect"} onClick={() => void detect()}>
                {t("proxyChain.detect")}
              </Button>
            )}
            below={detected ? (
              <div data-chain-detection role="status" className="flex flex-col gap-1 text-help text-text-sub-600">
                <span>{detected.state === "ok" ? t("proxyChain.detected") : detected.state === "manual_required" ? t("proxyChain.manualRequired") : t("proxyChain.detectionFailed")}</span>
                {detected.kind && detected.host && <span className="mono break-all">{detected.kind === "http" ? "HTTP" : "SOCKS5"} {detected.host}:{detected.port}</span>}
                {detected.resolved_for && <span className="mono break-all">{t("proxyChain.resolvedFor")}: {detected.resolved_for}</span>}
                <span>{t("proxyChain.source", { source: detected.source })}</span>
                {detectionError && <span>{detectionError}</span>}
                {detected.has_bypass && <span>{t("proxyChain.bypassIgnored")}</span>}
              </div>
            ) : undefined}
          />
        ) : (
          <>
            <SettingRow id="proxy-chain-host" title={t("proxyChain.host")} control={({ labelId }) => (
              <div className="w-[240px]">
                <Input aria-labelledby={labelId} inputSize="xsmall" technical value={config.host} placeholder="127.0.0.1" onChange={(e) => patch({ host: e.target.value })} />
              </div>
            )} />
            <SettingRow id="proxy-chain-port" title={t("proxyChain.port")} control={({ labelId }) => (
              <div className="w-[120px]">
                <NumField
                  aria-labelledby={labelId} value={config.port} min={1} max={65535}
                  onChange={(port) => patch({ port })}
                  onInvalid={(bad) => holdSave("proxy_chain.port", bad ? t("numField.invalidRange", { min: 1, max: 65535 }) : null)}
                />
              </div>
            )} />
            <SettingRow id="proxy-chain-username" title={t("proxyChain.username")} control={({ labelId }) => (
              <div className="w-[240px]">
                <Input aria-labelledby={labelId} inputSize="xsmall" technical value={config.username} autoComplete="off" onChange={(e) => patch({ username: e.target.value })} />
              </div>
            )} />
            <SettingRow
              id="proxy-chain-password" title={t("proxyChain.password")}
              help={config.password === null ? t("proxyChain.passwordCleared") : config.password_saved ? t("proxyChain.passwordSaved") : t("proxyChain.passwordHelp")}
              control={({ labelId, helpId }) => (
                <div className="w-[240px]">
                  <Input
                    aria-labelledby={labelId} aria-describedby={helpId} inputSize="xsmall" secret type="password" autoComplete="new-password"
                    value={config.password ?? ""} placeholder={t("proxyChain.passwordPlaceholder")}
                    onChange={(e) => patch({ password: e.target.value || undefined })}
                  />
                </div>
              )}
              below={
                config.password === null && config.password_saved ? (
                  <Button variant="neutral" mode="ghost" size="xsmall" onClick={() => patch({ password: undefined })}>{t("proxyChain.keepPassword")}</Button>
                ) : (
                  <Button variant="neutral" mode="ghost" size="xsmall" disabled={!config.password_saved && !config.password} onClick={() => patch({ password: null })}>{t("proxyChain.clearPassword")}</Button>
                )
              }
            />
          </>
        )}
      </SettingsGroup>

      <SettingsGroup title={t("proxyChain.testsTitle")} help={t("proxyChain.testsHelp")}>
        <p className="m-0 pt-stack-md text-help text-text-sub-600">{t("proxyChain.systemRoutingHelp")}</p>
        <div className="flex flex-wrap items-center gap-2 py-3">
          <Button variant="neutral" mode="stroke" size="xsmall" disabled={!!pending} isLoading={pending === "local"} onClick={() => void test("local")}>{t("proxyChain.testLocal")}</Button>
          <Button variant="neutral" mode="stroke" size="xsmall" disabled={!!pending} isLoading={pending === "system"} onClick={() => void test("system")}>{t("proxyChain.testSystem")}</Button>
        </div>
        <SettingRow
          id="proxy-chain-profile" title={t("proxyChain.profile")}
          help={t("proxyChain.profileHelp")}
          control={({ labelId }) => (
            <div className="w-[240px]">
              <SelectField
                aria-labelledby={labelId} value={selected?.id ?? ""} placeholder={t("proxyChain.chooseProfile")} isSearchable
                options={profiles.filter((p) => may(p, "start")).map((p) => ({ value: p.id, label: p.name }))}
                onChange={(id) => { sequence.current++; setProfileId(id); }}
              />
            </div>
          )}
          below={<Button variant="neutral" mode="stroke" size="xsmall" disabled={!!pending || !selected} isLoading={pending === "profile"} onClick={() => void test("profile")}>{t("proxyChain.testProfile")}</Button>}
        />
        {failure && <p role="alert" className="m-0 pb-3 text-help text-error-text" data-chain-error>{failure}</p>}
        {shown && <ChainResult result={shown} />}
      </SettingsGroup>
    </div>
  );
}

function ChainResult({ result }: { result: ProxyChainTestResult }) {
  const t = useT();
  const exitLabel = result.kind === "system" ? t("proxyChain.systemExitIp") : result.kind === "local" ? t("proxyChain.localExitIp") : t("proxyChain.exitIp");
  const exitError = result.exit_error_code === "proxy_chain_system_geo_failed" ? t("proxyChain.systemGeoFailed")
    : result.exit_error_code === "proxy_chain_profile_geo_failed" ? t("proxyChain.profileGeoFailed")
    : result.exit_error_code === "proxy_chain_local_geo_failed" ? t("proxyChain.localGeoFailed") : result.exit_error;
  // A stale or mismatched Geo response must not lend a location to another IP.
  const geo = result.exit_ip && !exitError && result.exit_geo?.ip === result.exit_ip ? result.exit_geo : null;
  const location = geo ? [...new Set([
    geo.country_code ? fmtCountry(geo.country_code) : geo.country,
    geo.region, geo.city,
  ].map((part) => part.trim()).filter(Boolean))].join(", ") : "";
  const missingExit = exitError ? t("proxyChain.exitUnavailable") : t("proxyChain.stageUntested");
  const stage = (value?: ProxyChainStage | null) => value?.state === "ok"
    ? value.latency_ms != null ? t("proxyChain.stageLatency", { ms: value.latency_ms }) : t("proxyChain.stageOk")
    : value?.state === "failed" ? t("proxyChain.stageFailed") : t("proxyChain.stageUntested");
  const time = result.tested_at.startsWith("@") ? fmtTs(result.tested_at) : new Date(result.tested_at).toLocaleString(appLocale());
  const rows = [
    { id: "local", label: t("proxyChain.localNetwork"), value: stage(result.local_network), detail: result.local_network },
    { id: "system", label: t("proxyChain.systemProxy"), value: stage(result.system_proxy), detail: result.system_proxy },
    { id: "profile", label: t("proxyChain.profileProxy"), value: stage(result.profile_proxy), detail: result.profile_proxy },
  ];
  return (
    <div data-chain-result role="status" aria-live="polite" className="pb-3 text-help text-text-sub-600">
      <dl className="m-0 flex flex-col gap-2">
        {result.proxy_kind && <div className="flex flex-wrap gap-2"><dt>{t("proxyChain.proxyType")}</dt><dd className="m-0">{result.proxy_kind === "http" ? "HTTP" : "SOCKS5"}</dd></div>}
        {result.proxy_address && <div className="flex flex-wrap gap-2"><dt>{t("proxyChain.firstProxyAddress")}</dt><dd className="mono m-0 break-all">{result.proxy_address}</dd></div>}
        {result.profile_proxy_address && <div className="flex flex-wrap gap-2"><dt>{t("proxyChain.profileProxyAddress")}</dt><dd className="mono m-0 break-all">{result.profile_proxy_address}</dd></div>}
        {result.resolved_for && <div className="break-all">{t("proxyChain.resolvedFor")}: {result.resolved_for}</div>}
        {result.source && <div>{t("proxyChain.source", { source: result.source })}</div>}
        {result.profile_name && <div className="flex flex-wrap gap-2"><dt>{t("proxyChain.profile")}</dt><dd className="m-0 break-all">{result.profile_name}</dd></div>}
        {rows.map((row) => {
          const error = proxyChainErrorText(t, row.detail);
          return (
            <div key={row.id} data-chain-stage={row.id} className="flex flex-wrap gap-2">
              <dt>{row.label}</dt>
              <dd className={`m-0 min-w-0 ${row.detail?.state === "failed" ? "text-error-text" : "text-text-strong-950"}`}>
                {row.value}
                {error && <span className="block break-words">{error}</span>}
                {row.detail?.error_code && <code className="block break-all">{row.detail.error_code}</code>}
              </dd>
            </div>
          );
        })}
        <div data-chain-exit className="flex flex-wrap gap-2"><dt>{exitLabel}</dt><dd className="mono m-0 break-all">{result.exit_ip || missingExit}</dd></div>
        <div data-chain-location className="flex flex-wrap gap-2"><dt>{t("proxyChain.exitLocation")}</dt><dd className="m-0 break-words">{location || (result.exit_ip ? t("proxyChain.locationNotReturned") : missingExit)}</dd></div>
        {geo?.provider && <div data-chain-geo-source className="flex flex-wrap gap-2"><dt>{t("proxyChain.geoProvider")}</dt><dd className="m-0 break-all">{geo.provider}</dd></div>}
        {result.kind === "system" && <div data-chain-exit-scope>{t("proxyChain.systemExitHelp")}</div>}
        {exitError && <div className="text-error-text" data-chain-exit-error>{exitError}</div>}
        {result.total_ms != null && <div className="flex flex-wrap gap-2"><dt>{t("proxyChain.totalLatency")}</dt><dd className="m-0">{t("proxyChain.milliseconds", { ms: result.total_ms })}</dd></div>}
        <div className="flex flex-wrap gap-2"><dt>{t("proxyChain.testedAt")}</dt><dd className="m-0">{time}</dd></div>
      </dl>
    </div>
  );
}

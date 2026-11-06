import { useEffect, useRef, useState } from "react";
import { Button, Switch } from "@proxyshard/shardx-ui-kit";
import { profileRouteTest } from "../../../entities/profile/model/api";
import { proxyLabel } from "../../../entities/proxy";
import { useProfile } from "../../../entities/profile";
import type { ProxyChainTestResult } from "../../../entities/settings";
import { bindRefusal, useInTeam, useRightsChange, useSpace } from "../../../entities/space";
import { PropertySheet } from "../../../shared/ui/peek";
import { useT } from "../../../shared/i18n";
import { explainError } from "../../../shared/lib/errors";
import type { ProfileProxyRoute } from "../../../shared/model/proxyChain";
import { routeUsesAssignedProxy } from "../../../shared/model/proxyChain";
import { SelectField } from "../../../shared/ui/SelectField";
import { patchDraft, patchForm, problemsOf, useProfileForm, type ProfileFormDraft } from "../model/useProfileForm";
import { ProxyRow } from "./fields";
import { ChainPolicyField } from "../../../shared/ui/ChainPolicyField";
import { ProfileRouteResult } from "./ProfileRouteResult";

/** The editor's explicit route is a draft until Save, and tests use that same draft. */
export function ProxyTab({ draft }: { draft: ProfileFormDraft }) {
  const t = useT();
  const route = draft.form.proxy_route;
  const { id, session, unlocked } = useProfileForm();
  const spaceEpoch = useSpace((s) => s.state?.epoch);
  const proxies = useProfile((s) => s.proxies);
  const team = useInTeam();
  useRightsChange();
  const sequence = useRef(0);
  const context = JSON.stringify([session, id, spaceEpoch, route, draft.form.proxy_id, draft.form.proxy_chain_policy, draft.newProxyLine]);
  const current = useRef(context);
  current.current = context;
  const [busy, setBusy] = useState<string | null>(null);
  const [result, setResult] = useState<{ context: string; value: ProxyChainTestResult } | null>(null);
  const [error, setError] = useState<{ context: string; value: string } | null>(null);
  const lastChain = useRef<Extract<ProfileProxyRoute, { mode: "chain" }>>({ mode: "chain", first: { mode: "system" }, second: { mode: "proxy" } });
  const lastSingle = useRef<"direct" | "system" | "proxy">("proxy");
  if (route?.mode === "chain") lastChain.current = route;
  else if (route && route.mode !== "invalid") lastSingle.current = route.mode;
  useEffect(() => () => { sequence.current++; }, [context]);
  const change = (proxy_route?: ProfileProxyRoute) => patchForm({ proxy_route });
  const problem = problemsOf(draft).proxy_route;
  const shown = result?.context === context ? result.value : null;
  const failure = error?.context === context ? error.value : null;
  const test = async () => {
    if (busy === context || problem) return;
    const request = ++sequence.current;
    setBusy(context); setError(null); setResult(null);
    try {
      const value = await profileRouteTest({ route, proxyId: draft.form.proxy_id, newProxyLine: draft.newProxyLine,
        policy: draft.form.proxy_chain_policy, profileId: id ?? undefined, password: unlocked ?? undefined });
      if (request === sequence.current && current.current === context) setResult({ context, value });
    } catch (e) {
      if (request === sequence.current && current.current === context) setError({ context, value: explainError(e) ?? t("profileRoute.testFailed") });
    } finally {
      if (request === sequence.current && current.current === context) setBusy(null);
    }
  };
  const proxyPicker = <PropertySheet><ProxyRow value={draft.form.proxy_id}
    onChange={(proxy_id) => { patchDraft({ newProxyLine: "" }); patchForm({ proxy_id }); }}
    newLine={draft.newProxyLine} onNewLine={(newProxyLine) => patchDraft({ newProxyLine })} /></PropertySheet>;
  const firstOptions = [{ value: "", label: t("profileRoute.chooseProxy") }, ...proxies.map((p) => ({ value: p.id, label: proxyLabel(p),
    hint: team && bindRefusal(p.id) ? bindRefusal(p.id)! : `${p.kind.toUpperCase()} ${p.host}:${p.port}`, disabled: !!(team && bindRefusal(p.id)), }))];
  const sourceOptions = [
    { value: "system" as const, label: t("profileRoute.systemLabel") },
    { value: "proxy" as const, label: t("profileRoute.proxyLabel") },
    { value: "direct" as const, label: t("profileRoute.directLabel") },
  ];
  return <div data-tour="form-proxy" data-profile-proxy-route className="flex flex-col gap-3">
    {!route ? <>
      <div className="flex flex-col gap-2">
        <p className="m-0 text-secondary text-text-strong-950">{t("profileRoute.legacyTitle")}</p>
        <p className="m-0 text-help text-text-sub-600">{t("profileRoute.legacyHelp")}</p>
        <Button variant="neutral" mode="stroke" size="xsmall" onClick={() => change({ mode: draft.form.proxy_id || draft.newProxyLine ? "proxy" : "direct" })}>{t("profileRoute.customize")}</Button>
      </div>
      {proxyPicker}
      <ChainPolicyField profile value={draft.form.proxy_chain_policy} onChange={(proxy_chain_policy) => patchForm({ proxy_chain_policy })} />
      <p className="m-0 text-help text-text-sub-600">{t("profileForm.proxyHelp")}</p>
    </> : <>
      <div className="flex items-center justify-between gap-3">
        <span id="profile-route-chain-label" className="text-secondary text-text-strong-950">{t("profileRoute.enableChain")}</span>
        <Switch aria-labelledby="profile-route-chain-label" checked={route.mode === "chain"} onChange={(enabled) => change(enabled ? lastChain.current : { mode: lastSingle.current })} />
      </div>
      <p className="m-0 text-help text-text-sub-600">{t("profileRoute.overrideHelp")}</p>
      {route.mode === "invalid" ? <Button variant="neutral" mode="stroke" size="xsmall" onClick={() => change({ mode: "proxy" })}>{t("profileRoute.resetInvalid")}</Button> : route.mode !== "chain" ? <>
        <SelectField label={t("profileRoute.connection")} value={route.mode} options={sourceOptions} onChange={(mode) => change({ mode })} />
        {route.mode === "proxy" && proxyPicker}
        <p className="m-0 text-help text-text-sub-600">{route.mode === "system" ? t("profileRoute.systemHelp") : route.mode === "direct" ? t("profileRoute.directHelp") : t("profileRoute.proxyHelp")}</p>
      </> : <>
        <div className="flex flex-col gap-2" data-profile-first-hop>
          <SelectField label={t("profileRoute.firstLayer")} value={route.first.mode} options={[
            { value: "system" as const, label: t("profileRoute.firstSystem") }, { value: "proxy" as const, label: t("profileRoute.firstProxy") }, { value: "direct" as const, label: t("profileRoute.firstDirect") },
          ]} onChange={(mode) => change({ ...route, first: mode === "proxy" ? { mode, proxy_id: lastChain.current.first.mode === "proxy" ? lastChain.current.first.proxy_id : "" } : { mode } })} />
          {route.first.mode === "proxy" && <SelectField label={t("profileRoute.firstProxyChoice")} value={route.first.proxy_id} options={firstOptions} isSearchable onChange={(proxy_id) => change({ ...route, first: { mode: "proxy", proxy_id } })} />}
          <p className="m-0 text-help text-text-sub-600">{route.first.mode === "system" ? t("profileRoute.systemHelp") : route.first.mode === "direct" ? t("profileRoute.firstDirectHelp") : t("profileRoute.firstProxyHelp")}</p>
        </div>
        <div className="flex flex-col gap-2" data-profile-second-hop>
          <SelectField label={t("profileRoute.secondLayer")} value={route.second.mode} options={[
            { value: "proxy" as const, label: t("profileRoute.secondProxy") }, { value: "direct" as const, label: t("profileRoute.secondDirect") },
          ]} onChange={(mode) => change({ ...route, second: { mode } })} />
          {route.second.mode === "proxy" && proxyPicker}
          <p className="m-0 text-help text-text-sub-600">{route.second.mode === "direct" ? t("profileRoute.secondDirectHelp") : t("profileRoute.secondProxyHelp")}</p>
        </div>
        {route.first.mode === "proxy" && route.second.mode === "proxy" && route.first.proxy_id === draft.form.proxy_id && <p data-profile-route-same className="m-0 text-help text-text-sub-600">{t("profileRoute.sameProxyHelp")}</p>}
        {route.second.mode === "proxy" && route.first.mode !== "direct" && <div className="flex flex-col gap-2">
          <div className="flex items-center justify-between gap-3">
            <span id="profile-route-fallback-label" className="text-secondary text-text-strong-950">{t("profileRoute.fallbackLabel")}</span>
            <Switch aria-labelledby="profile-route-fallback-label" checked={draft.form.proxy_chain_policy === "prefer_first_hop"} onChange={(enabled) => patchForm({ proxy_chain_policy: enabled ? "prefer_first_hop" : "enabled" })} />
          </div>
          <p className="m-0 text-help text-text-sub-600">{t("profileRoute.fallbackHelp")}</p>
        </div>}
      </>}
      <Button variant="neutral" mode="link" size="3xsmall" onClick={() => change(undefined)}>{t("profileRoute.useLegacy")}</Button>
    </>}
    {problem && <p role="alert" className="m-0 text-help text-error-text">{problem}</p>}
    <div className="flex flex-wrap items-center gap-2">
      <Button variant="neutral" mode="stroke" size="xsmall" disabled={!!problem || busy === context} isLoading={busy === context} onClick={() => void test()}>{t("profileRoute.test")}</Button>
      <span className="text-help text-text-soft-400">{t("profileRoute.testHelp")}</span>
    </div>
    {failure && <p role="alert" className="m-0 text-help text-error-text">{failure}</p>}
    {shown && <ProfileRouteResult result={shown} route={route} />}
    {route && !routeUsesAssignedProxy(route) && draft.form.proxy_id && <p className="m-0 text-help text-text-soft-400">{t("profileRoute.bindingKept")}</p>}
  </div>;
}

import { useEffect, useId, useRef, useState } from "react";
import { Button, Switch } from "@proxyshard/shardx-ui-kit";
import { defaultProxyChain, proxyChainSetEnabled, proxyChainTest, settingsGet, type ProxyChainSettings, type ProxyChainTestResult } from "../../entities/settings";
import { useNav } from "../../shared/model/navigation";
import { RouteIcon } from "../../shared/icons";
import { useT } from "../../shared/i18n";
import { proxyChainErrorFromText, proxyChainErrorText } from "../../shared/lib/proxyChainError";
import { explainError } from "../../shared/lib/errors";
import { fmtTs } from "../../shared/lib/utils";
import { StatusDot } from "../../shared/ui/StatusDot";

/** Saved global routing and a fresh first-hop probe, never a profile's exit. */
export function ProxyQuickControl() {
  const t = useT();
  const id = useId();
  const root = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const sequence = useRef(0);
  const lifetime = useRef(0);
  const alive = useRef(false);
  const writePending = useRef(false);
  const [open, setOpen] = useState(false);
  const [config, setConfig] = useState<ProxyChainSettings | null>(null);
  const [result, setResult] = useState<ProxyChainTestResult | null>(null);
  const [busy, setBusy] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const errorText = (e: unknown) => {
    const raw = e instanceof Error ? e.message : String(e);
    return proxyChainErrorText(t, proxyChainErrorFromText(raw) ?? { code: raw }) ?? explainError(e) ?? raw;
  };
  const refresh = async (probe: boolean) => {
    if (!alive.current) return;
    const request = ++sequence.current;
    setBusy(true); setError(null);
    try {
      const settings = await settingsGet();
      const value = settings.proxy_chain ?? defaultProxyChain();
      if (request !== sequence.current) return;
      setConfig(value);
      if (probe) {
        setResult(null);
        const next = await proxyChainTest(value.enabled ? "system" : "local", value);
        if (request === sequence.current) setResult(next);
      }
    } catch (e) { if (request === sequence.current) setError(errorText(e)); }
    finally { if (request === sequence.current) setBusy(false); }
  };
  useEffect(() => {
    alive.current = true; void refresh(false);
    return () => { alive.current = false; lifetime.current++; sequence.current++; clearTimeout(timer.current); };
  }, []);
  useEffect(() => {
    if (!open) return;
    void refresh(true);
    const dismiss = (event: PointerEvent) => { if (!root.current?.contains(event.target as Node)) setOpen(false); };
    const escape = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || event.isComposing) return;
      event.preventDefault(); event.stopPropagation(); setOpen(false); trigger.current?.focus();
    };
    document.addEventListener("pointerdown", dismiss);
    document.addEventListener("keydown", escape);
    return () => { document.removeEventListener("pointerdown", dismiss); document.removeEventListener("keydown", escape); };
  }, [open]);
  const toggle = async (enabled: boolean) => {
    if (writePending.current || !config) return;
    const generation = lifetime.current;
    writePending.current = true;
    ++sequence.current; setSaving(true); setBusy(false); setError(null); setResult(null);
    let saved = false;
    try {
      const value = await proxyChainSetEnabled(enabled);
      if (!alive.current || generation !== lifetime.current) return;
      ++sequence.current; setConfig(value); saved = true;
    } catch (e) { if (alive.current && generation === lifetime.current) setError(errorText(e)); }
    finally { writePending.current = false; if (alive.current && generation === lifetime.current) setSaving(false); }
    if (saved && alive.current) void refresh(true);
  };
  const keepOpen = () => { clearTimeout(timer.current); setOpen(true); };
  const failure = result ? proxyChainErrorText(t, result.kind === "local" ? result.local_network : result.system_proxy)
    ?? proxyChainErrorText(t, { code: result.exit_error_code, message: result.exit_error }) : null;
  return <div ref={root} className="relative flex items-center" data-proxy-quick
    onMouseEnter={keepOpen}
    onMouseLeave={() => { timer.current = setTimeout(() => { if (!root.current?.contains(document.activeElement)) setOpen(false); }, 250); }}
    onKeyDown={(e) => { if (e.key === "Escape" && open) { e.preventDefault(); e.stopPropagation(); setOpen(false); trigger.current?.focus(); } }}>
    <Button ref={trigger} size="xsmall" mode="stroke" variant="neutral" leftIcon={<span className="relative flex">
      <RouteIcon className="size-3.5" /><StatusDot tone={config?.enabled ? "success" : "outline"} className="absolute -right-1 -bottom-1" />
    </span>}
      aria-expanded={open} aria-controls={id} aria-haspopup="dialog" onClick={keepOpen}>
      {config?.enabled ? t("proxyQuick.enabled") : t("proxyQuick.disabled")}
    </Button>
    {open && <div id={id} role="dialog" aria-label={t("proxyQuick.title")}
      className="absolute right-0 top-full z-(--layer-overlay) mt-1.5 w-[320px] max-w-[calc(100vw-32px)] rounded-surface bg-bg-popover p-4 shadow-(--shadow-float) white-surface">
      <div className="flex items-center justify-between gap-3">
        <span className="text-ui">{t("proxyChain.enable")}</span>
        <Switch aria-label={t("proxyChain.enable")} checked={config?.enabled ?? false} disabled={!config || saving} onChange={(on) => void toggle(on)} />
      </div>
      <p className="my-2 text-help text-text-sub-600">{t("proxyQuick.nextLaunch")}</p>
      <p className="my-2 text-ui">{config?.enabled ? t("proxyQuick.firstExit") : t("proxyQuick.localExit")}</p>
      <div role="status" aria-live="polite" className="text-help text-text-sub-600">
        {busy ? t("proxyQuick.checking") : result?.exit_ip ? <>
          <div className="text-body text-text-strong-950" data-quick-ip>{result.exit_ip}</div>
          <div>{result.exit_geo ? [result.exit_geo.country, result.exit_geo.region, result.exit_geo.city].filter(Boolean).join(" / ") : t("proxyQuick.noRegion")}</div>
          <div>{t("proxyQuick.checkedAt", { time: fmtTs(result.tested_at) })}</div>
        </> : t("proxyQuick.noResult")}
      </div>
      {(error || failure) && <p role="alert" className="my-2 text-help text-error-text">{error || failure}</p>}
      <div className="mt-3 flex items-center justify-between gap-2">
        <Button size="xsmall" mode="ghost" variant="neutral" disabled={busy || saving} onClick={() => void refresh(true)}>{t("proxyQuick.refresh")}</Button>
        <Button size="xsmall" mode="ghost" variant="neutral" onClick={() => { setOpen(false); void useNav.getState().openSettings("proxy"); }}>{t("proxyQuick.settings")}</Button>
      </div>
    </div>}
  </div>;
}

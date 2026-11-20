import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Badge, Button, Checkbox, IconButton, Modal, Select, Textarea, Tooltip } from "@proxyshard/shardx-ui-kit";
import { RefreshIcon } from "../../../shared/icons";
import { CountryFlag } from "../../../shared/ui/CountryFlag";
import { ProxyKind } from "../../../shared/ui/ProxyAddress";
import { WindowedRows } from "../../../shared/ui/WindowedRows";
import { toast } from "../../../shared/model/toast";
import { appLocale, useLang, useT } from "../../../shared/i18n";
import type { ProxyEntry, BulkRowState } from "../../../entities/proxy";
import { proxyBulkParse, proxyBulkSave, proxyFullTest, proxyStatus, sameProxy, tnbProxyExtract, useProxy, type TnbExtractResult } from "../../../entities/proxy";
import { providerDefaultsGet, providerExtractCustom, providerGenerate, providerGenerateMany, type ProviderDefaults } from "../../../entities/proxy";
import { ProxyFolderSelect } from "../../../entities/proxy";
import { Input } from "@proxyshard/shardx-ui-kit";
import { ProviderDefaultsPanel } from "./ProviderDefaultsPanel";
import { ProviderSessionFields } from "./ProviderSessionFields";
import { NumField } from "../../../shared/ui/NumField";
import { SelectField } from "../../../shared/ui/SelectField";
import { newTnbDraft, TnbExtractForm, tnbErrorText, tnbGeoDefaults, tnbRequest, tnbDefaultTtl } from "./TnbExtractForm";

const ISO_COUNTRY_CODES = `AF AL DZ AS AD AO AI AQ AG AR AM AW AU AT AZ BS BH BD BB BY BE BZ BJ BM BT BO BQ BA BW BV BR IO BN BG BF BI CV KH CM CA KY CF TD CL CN CX CC CO KM CG CD CK CR CI HR CU CW CY CZ DK DJ DM DO EC EG SV GQ ER EE SZ ET FK FO FJ FI FR GF PF TF GA GM GE DE GH GI GR GL GD GP GU GT GG GN GW GY HT HM VA HN HK HU IS IN ID IR IQ IE IM IL IT JM JP JE JO KZ KE KI KP KR KW KG LA LV LB LS LR LY LI LT LU MO MG MW MY MV ML MT MH MQ MR MU YT MX FM MD MC MN ME MS MA MZ MM NA NR NP NL NC NZ NI NE NG NU NF MK MP NO OM PK PW PS PA PG PY PE PH PN PL PT PR QA RE RO RU RW BL SH KN LC MF PM VC WS SM ST SA SN RS SC SL SG SX SK SI SB SO ZA GS SS ES LK SD SR SJ SE CH SY TW TJ TZ TH TL TG TK TO TT TN TR TM TC TV UG UA AE GB US UM UY UZ VU VE VN VG VI WF EH YE ZM ZW`.split(" ");

/// `onClose` gets the id of the proxy saved when it was the only one, for the
/// page to open in Peek (or not, when the setup checklist opened the importer).
export function ProxyBulkImporter({ open, onClose }: { open: boolean; onClose: (added?: string) => void }) {
  const t = useT();
  const lang = useLang((s) => s.lang);
  const countryOptions = useMemo(() => {
    const names = new Intl.DisplayNames([appLocale()], { type: "region" });
    return ISO_COUNTRY_CODES.map((value) => ({ value, label: `${value}, ${names.of(value) ?? value}` }));
  }, [lang]);
  const [text, setText] = useState("");
  const [kind, setKind] = useState<ProxyEntry["kind"]>("socks5");
  const [rows, setRows] = useState<BulkRowState[]>([]);
  const [portInvalid, setPortInvalid] = useState(false);
  const [busy, setBusy] = useState(false);
  const [source, setSource] = useState("custom");
  const [defaults, setDefaults] = useState<ProviderDefaults | null>(null);
  const [tnbDefaults, setTnbDefaults] = useState<ProviderDefaults | null>(null);
  const [tnbDefaultsFailed, setTnbDefaultsFailed] = useState(false);
  const customId = source.startsWith("custom-provider:") ? source.slice("custom-provider:".length) : undefined;
  const isCustomProvider = source === "custom-provider" || Boolean(customId);
  const [customDefaults, setCustomDefaults] = useState<{ id: string; value: ProviderDefaults | null; missing?: boolean } | null>(null);
  const activeDefaults = customId ? (customDefaults?.id === customId ? customDefaults.value : null) : source === "tnb" ? tnbDefaults : defaults;
  const customExtraction = isCustomProvider && Boolean(activeDefaults?.custom_extract_url);
  const [providerName, setProviderName] = useState("");
  const [providerFolder, setProviderFolder] = useState<string | null>(null);
  const destinationFolder = providerFolder ?? activeDefaults?.folder ?? "";
  const [tnb, setTnb] = useState(newTnbDraft);
  const [country, setCountry] = useState("");
  const [state, setState] = useState("");
  const [city, setCity] = useState("");
  const [batchCount, setBatchCount] = useState<string | null>(null);
  const [relaySession, setRelaySession] = useState<"sticky" | "per_request" | null>(null);
  const [relayMinutes, setRelayMinutes] = useState<string | null>(null);
  const relayMode = relaySession ?? ((activeDefaults?.session_type ?? "sticky") === "sticky" ? "sticky" : "per_request");
  const relayMinutesText = relayMinutes ?? String(Math.min(activeDefaults?.session_minutes ?? 120, 120));
  const sessionMinutes = relayMode === "sticky" ? Number(relayMinutesText) : 0;
  const invalidSession = source === "1024proxy" && relayMode === "sticky" && (!Number.isInteger(sessionMinutes) || sessionMinutes < 1 || sessionMinutes > 120);
  const canGenerateBatch = source === "1024proxy" && relayMode === "sticky";
  const countText = batchCount ?? String(activeDefaults?.generation_count ?? 1);
  const generationCount = canGenerateBatch || customExtraction ? Number(countText) : 1;
  const invalidCount = !Number.isInteger(generationCount) || generationCount < 1 || generationCount > 1000;
  const authSwitchTouched = useRef({ api: false, proxy: false });
  useEffect(() => {
    if (!open) return;
    setTnbDefaults(null);
    setTnbDefaultsFailed(false);
    setProviderFolder(null);
    setBatchCount(null);
    setRelaySession(null);
    setRelayMinutes(null);
    authSwitchTouched.current = { api: false, proxy: false };
    let live = true;
    void providerDefaultsGet().then((v) => {
      if (!live) return;
      setDefaults(v);
      setCountry(v.country);
      setState(v.state);
      setCity(v.city);
    }).catch(() => {});
    void providerDefaultsGet({ tnb: true }).then((v) => {
      if (!live) return;
      setTnbDefaults(v);
      setTnb((d) => ({
        ...d,
        ...tnbGeoDefaults(v),
        server: v.tnb_server,
        plan_name: v.tnb_plan,
        provider_name: v.name,
        kind: v.tnb_kind,
        // Saved credentials enable their matching authentication switch by default.
        // The user can still clear either switch before requesting /gen.
        use_api_credentials: Boolean(v.tnb_api_username.trim() || v.tnb_api_password_saved),
        use_proxy_credentials: Boolean(v.tnb_proxy_username.trim() || v.tnb_proxy_password_saved),
        count: String(v.generation_count),
        ttl: tnbDefaultTtl(v),
      }));
    }).catch(() => { if (live) setTnbDefaultsFailed(true); });
    return () => { live = false; };
  }, [open]);
  useEffect(() => {
    if (!open || !customId) return;
    setCustomDefaults(null);
    let live = true;
    void providerDefaultsGet({ customProviderId: customId }).then((value) => {
      if (live) setCustomDefaults({ id: customId, value });
    }).catch((error) => {
      if (live) setCustomDefaults({ id: customId, value: null, missing: String(error).includes("provider_not_found") });
    });
    return () => { live = false; };
  }, [open, customId]);
  const generate = async () => {
    if (extraction.current || invalidCount || invalidSession || !activeDefaults) return;
    extraction.current = true; setExtracting(true); setExtractError(null);
    try {
      const count = generationCount;
      const entries = customExtraction
        ? await providerExtractCustom(customId, count)
        : count > 1
        ? await providerGenerateMany(
            country,
            state.trim(),
            city.trim(),
            count,
            sessionMinutes,
            isCustomProvider,
            customId,
          )
        : [await providerGenerate(
          isCustomProvider ? activeDefaults.country : country,
          isCustomProvider ? activeDefaults.state : state.trim(),
          isCustomProvider ? activeDefaults.city : city.trim(),
          isCustomProvider,
          customId,
          source === "1024proxy" ? sessionMinutes : undefined,
        )];
      setRows(entries.map((entry, index) => ({
        entry: {
          ...entry,
          folder: destinationFolder,
          name: providerName.trim()
            ? (entries.length === 1 ? providerName.trim() : `${providerName.trim()} ${index + 1}`)
            : entry.name,
        },
        selected: true,
        status: "idle",
      })));
    } catch (e) {
      const error = String(e);
      setExtractError(error.includes("provider_invalid_extract_url") ? t("provider.invalidExtractUrl")
        : error.includes("provider_invalid_relay") ? t("provider.invalidGateway")
        : error.includes("provider_defaults_invalid") ? t("provider.defaultsInvalid")
        : error.includes("provider_credentials_unavailable") ? t("provider.credentialsUnavailable")
        : error.includes("provider_defaults_busy") ? t("provider.defaultsBusy")
        : error.includes("provider_defaults_unreadable") ? t("provider.defaultsUnreadable")
        : error.includes("provider_not_found") ? t("provider.notFound")
        : error.includes("provider_single_gateway") ? t("provider.singleGateway")
        : error.includes("provider_region_template_required") ? t("provider.templateRequired")
        : error.includes("provider_region_invalid") ? t("provider.regionInvalid")
        : customExtraction ? customExtractError(error) : t("provider.credentialsRequired"));
    }
    finally { extraction.current = false; setExtracting(false); }
  };
  const customExtractError = (error: string) => {
    switch (error) {
      case "tnb_api_credentials_required": return t("provider.credentialsRequired");
      case "tnb_auth_required": return t("provider.authFailed");
      case "tnb_whitelist_required": return t("provider.extractAllowlist");
      default: return t("provider.extractFailed");
    }
  };
  const [extracting, setExtracting] = useState(false);
  const [extractError, setExtractError] = useState<string | null>(null);
  const [extracted, setExtracted] = useState<TnbExtractResult | null>(null);
  const extraction = useRef(false);
  // A second press while the first is saving would save it again.
  const adding = useRef(false);
  // Closed while Add saved (Esc, the backdrop, Cancel): the proxies are
  // still added, but no Peek opens for a dialog the person already left.
  const openRef = useRef(open);
  openRef.current = open;

  const extract = async () => {
    if (extraction.current) return;
    if (!tnbDefaults) return;
    const request = tnbRequest(tnb);
    if (!request) { setExtractError(t("tnb.errorInvalid")); return; }
    extraction.current = true;
    setExtracting(true);
    setExtractError(null);
    try {
      const result = await tnbProxyExtract(request);
      if (result.entries.length === 0) { setExtractError(t("tnb.errorEmpty")); return; }
      setExtracted(result);
      const name = providerName.trim();
      setRows(result.entries.map((entry, index) => ({
        entry: {
          ...entry,
          folder: destinationFolder,
          name: name ? (result.entries.length === 1 ? name : `${name} ${index + 1}`) : entry.name,
        },
        selected: true,
        status: "idle",
      })));
    } catch (e) { setExtractError(tnbErrorText(t, e)); }
    finally { extraction.current = false; setExtracting(false); }
  };

  const parseText = async (value: string, defaultKind: ProxyEntry["kind"]) => {
    if (!value.trim()) { toast.err(t("proxyBulkImporter.nothingToParse")); return; }
    try {
      const parsed = await proxyBulkParse(value, defaultKind);
      if (parsed.length === 0) { toast.err(t("proxyBulkImporter.noValidLines")); return; }
      setPortInvalid(false);
      setRows(parsed.map((e) => ({ entry: e, selected: true, status: "idle" })));
    } catch (e) { toast.fail({ title: t("proxyBulkImporter.previewFailed"), detail: e }); }
  };

  const enterManually = () => {
    setPortInvalid(false);
    setRows([{ selected: true, status: "idle", entry: {
      id: crypto.randomUUID(), name: "", host: "", port: 0, kind,
      username: "", password: "", country: "", notes: "", provider_id: "manual", folder: "",
    } }]);
  };

  // Test results wait in a buffer and reach the rows once per frame: "Test
  // all" over 1,000 lines renders about once a frame, not once per result
  // over every row (PERF-19).
  const rowsRef = useRef(rows);
  rowsRef.current = rows;
  const pending = useRef(new Map<number, Partial<BulkRowState>>());
  const frame = useRef(0);
  const put = useCallback((idx: number, patch: Partial<BulkRowState>) => {
    pending.current.set(idx, { ...pending.current.get(idx), ...patch });
    if (frame.current) return;
    frame.current = requestAnimationFrame(() => {
      frame.current = 0;
      const batch = pending.current;
      pending.current = new Map();
      setRows((rs) => rs.map((r, i) => (batch.has(i) ? { ...r, ...batch.get(i) } : r)));
    });
  }, []);
  useEffect(() => () => cancelAnimationFrame(frame.current), []);

  const testOne = useCallback(async (idx: number) => {
    const entry = rowsRef.current[idx]?.entry;
    if (!entry) return;
    put(idx, { status: "testing" });
    try {
      const snap = await proxyFullTest(entry);
      put(idx, {
        status: proxyStatus(snap) === "ok" ? "ok" : "fail",
        tcp_ms: snap.tcp_ms,
        udp_ms: snap.udp_ms,
        country: snap.country_code || rowsRef.current[idx]?.country,
        entry: { ...entry, country: snap.country_code || entry.country },
      });
    } catch (e) {
      put(idx, { status: "fail", error: String(e) });
    }
  }, [put]);

  const toggle = useCallback((idx: number) => {
    setRows((rs) => rs.map((x, j) => (j === idx ? { ...x, selected: !x.selected } : x)));
  }, []);

  const testAll = async () => {
    setBusy(true);
    const CONCURRENCY = 5;
    const queue = rows.map((_, i) => i);
    let cursor = 0;
    await Promise.all(
      Array.from({ length: Math.min(CONCURRENCY, queue.length) }, async () => {
        while (cursor < queue.length) {
          const i = queue[cursor++];
          if (i == null) break;
          await testOne(i);
        }
      }),
    );
    setBusy(false);
  };

  // Every paste goes through the preview first, one line too, so a proxy
  // can be tested before it is added. One proxy added opens in Peek, as New
  // profile does (CRAFT-26); several say how many in a toast.
  const saveSelected = async () => {
    if (adding.current) return;
    const entries = rows.filter((r) => r.selected).map((r) => {
      const entry = r.entry;
      return source === "custom" && rows.length === 1
        ? {
          ...entry,
          name: entry.name.trim() || `${entry.host.trim()}:${entry.port}`,
          host: entry.host.trim(),
          country: entry.country.trim().toUpperCase(),
          provider_options: { ...(entry.provider_options ?? {}), folder: entry.folder?.trim() ?? "" },
        }
        : entry;
    });
    if (entries.length === 0) { toast.err(t("proxyBulkImporter.nothingSelected")); return; }
    adding.current = true;
    setBusy(true);
    try {
      const n = await proxyBulkSave(entries);
      await useProxy.getState().reload();
      // The entry keeps the id it was parsed with; one the save skipped (an
      // identical proxy was already there) opens that proxy instead. Never
      // by host, port and user alone: several proxies can share them and
      // differ by password (a session in it).
      let added: string | undefined;
      if (entries.length === 1) {
        const [entry] = entries;
        const { proxies } = useProxy.getState();
        added = (proxies.find((p) => p.id === entry.id) ?? proxies.find((p) => sameProxy(p, entry)))?.id;
      }
      // Several, one not in the list read back, or the dialog already gone:
      // say how many, rather than end with nothing to show for it.
      if (!added || !openRef.current) toast.ok(n === 1 ? t("proxyBulkImporter.importedOne", { n }) : t("proxyBulkImporter.importedMany", { n }));
      if (openRef.current) onClose(added);
    } catch (e) { toast.fail({ title: t("proxyBulkImporter.addFailed"), detail: e }); }
    finally {
      adding.current = false;
      setBusy(false);
    }
  };

  const allSel = rows.length > 0 && rows.every((r) => r.selected);
  const selCount = rows.filter((r) => r.selected).length;
  const single = source === "custom" && rows.length === 1 ? rows[0] : null;
  const invalid = single !== null && (portInvalid || !single.entry.port || !single.entry.host.trim());
  const actionLabel = source === "tnb" || customExtraction ? t("tnb.extract") : source === "custom" ? t("proxyBulkImporter.parse") : t("provider.generate");
  const changeSingle = (patch: Partial<ProxyEntry>) => {
    setRows(([row]) => [{ selected: row.selected, status: "idle", entry: { ...row.entry, ...patch } }]);
  };

  return (
    <Modal
      open={open}
      onClose={() => { if (!extraction.current) onClose(); }}
      title={t("proxyBulkImporter.title")}
      size="lg"
      // Pasted lines or parsed rows: Esc and the backdrop ask first (IFACE-08).
      dirty={text.trim() !== "" || rows.length > 0 || source !== "custom"}
      footer={
        <div className="flex items-center justify-end gap-2">
          {/* Cancel cannot stop a save that has started (Esc and the backdrop close without a Peek). */}
          <Button variant="neutral" mode="stroke" size="xsmall" disabled={extracting || (busy && rows.length === 0)} onClick={() => onClose()}>{t("proxyBulkImporter.cancel")}</Button>
          {rows.length === 0 ? (
            <Button variant="primary" mode="filled" size="xsmall" aria-label={actionLabel} disabled={busy || extracting || invalidSession || invalidCount || (source !== "custom" && !activeDefaults)} isLoading={extracting} onClick={source === "tnb" ? extract : source === "custom" ? () => void parseText(text, kind) : generate}>
              {actionLabel}
            </Button>
          ) : (
            <Button variant="primary" mode="filled" size="xsmall" disabled={busy || invalid || selCount === 0} onClick={saveSelected}>
              {selCount === 1 ? t("proxyBulkImporter.importCountOne") : t("proxyBulkImporter.importCount", { n: selCount })}
            </Button>
          )}
        </div>
      }
    >
      <div className="flex min-h-0 flex-col gap-3">
        {rows.length === 0 ? (
          <>
            <Select label={t("tnb.source")} size="xsmall" value={source} disabled={extracting} onChange={(next) => { if (next === source) return; setSource(next); setCustomDefaults(null); setProviderFolder(null); setBatchCount(null); setExtractError(null); }} options={[
              { value: "custom", label: t("tnb.custom") },
              { value: "tnb", label: t("tnb.provider") },
              { value: "1024proxy", label: t("provider.name1024") },
              ...(defaults?.custom_server || defaults?.custom_extract_url ? [{ value: "custom-provider", label: defaults.custom_name || t("provider.custom") }] : []),
              ...(defaults?.custom_providers ?? []).map(({ id, name }) => ({ value: `custom-provider:${id}`, label: name })),
            ]} />
            {source !== "custom" && <>
              <details className="text-secondary">
                <summary className="cursor-pointer py-1 text-text-sub-600">{t("provider.defaultsTitle")}</summary>
                <div className="py-3">
                  <ProviderDefaultsPanel key={source} customProviderId={customId} provider={source === "tnb" ? "tnbproxy" : source === "1024proxy" ? "1024proxy" : "custom"} onSaved={(v) => {
                    if (customId) {
                      setCustomDefaults({ id: customId, value: v });
                      setDefaults((current) => current && ({ ...current, custom_providers: v.custom_providers }));
                      return;
                    }
                    if (source !== "tnb") {
                      if (source === "1024proxy" && country === (defaults?.country ?? "") && state === (defaults?.state ?? "") && city === (defaults?.city ?? "")) {
                        setCountry(v.country);
                        setState(v.state);
                        setCity(v.city);
                      }
                      setDefaults(v);
                      return;
                    }
                    setTnbDefaults(v);
                    setTnbDefaultsFailed(false);
                    const priorGeo = tnbDefaults ? tnbGeoDefaults(tnbDefaults) : newTnbDraft();
                    setTnb((d) => ({
                      ...d,
                      server: v.tnb_server,
                      plan_name: v.tnb_plan,
                      kind: v.tnb_kind,
                      ...(d.server !== v.tnb_server ||
                        (d.country?.code === priorGeo.country?.code && d.region?.code === priorGeo.region?.code && d.city?.code === priorGeo.city?.code)
                        ? tnbGeoDefaults(v) : {}),
                      count: d.count === String(tnbDefaults?.generation_count ?? 1) ? String(v.generation_count) : d.count,
                      ttl: d.ttl === (tnbDefaults ? tnbDefaultTtl(tnbDefaults) : newTnbDraft().ttl) ? tnbDefaultTtl(v) : d.ttl,
                      use_api_credentials: authSwitchTouched.current.api ? d.use_api_credentials : Boolean(v.tnb_api_username.trim() || v.tnb_api_password_saved),
                      use_proxy_credentials: authSwitchTouched.current.proxy ? d.use_proxy_credentials : Boolean(v.tnb_proxy_username.trim() || v.tnb_proxy_password_saved),
                    }));
                  }} />
                </div>
              </details>
              <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                <ProxyFolderSelect value={destinationFolder} onChange={setProviderFolder} disabled={extracting || !activeDefaults} allowCreate={false} />
                <Input label={t("proxyEditor.nameLabel")} inputSize="xsmall" value={providerName} placeholder={t("proxyBulkImporter.namePlaceholder")} disabled={extracting} onChange={(e) => setProviderName(e.target.value)} />
              </div>
              {source !== "tnb" && <Input
                label={customExtraction ? t("provider.customExtractUrl") : t("provider.gateway")}
                inputSize="xsmall"
                value={(customExtraction ? activeDefaults?.custom_extract_url : source === "1024proxy" ? activeDefaults?.relay : activeDefaults?.custom_server) ?? ""}
                readOnly
              />}
              {customId && customDefaults?.id === customId && !activeDefaults && <p role="alert" className="m-0 text-help text-error-text">{customDefaults.missing ? t("provider.notFound") : t("provider.defaultsUnreadable")}</p>}
            {isCustomProvider && !customExtraction && <p className="m-0 text-help text-text-sub-600">{t("provider.singleGateway")}</p>}
            </>}
            {source === "1024proxy" ? <div className="flex flex-col gap-3">
              <div className="grid grid-cols-3 gap-3">
                <SelectField
                  label={t("tnbGeo.country")}
                  value={country}
                  isSearchable
                  disabled={extracting}
                  searchPlaceholder={t("tnbGeo.searchCountry")}
                  onChange={(v) => { setCountry(v); setState(""); setCity(""); }}
                  options={[{ value: "", label: t("tnbGeo.anyCountry") }, ...countryOptions]}
                />
                <Input inputSize="xsmall" label={t("provider.stateCode")} placeholder={t("provider.statePlaceholder")} value={state} disabled={extracting || !country} onChange={(e) => { setState(e.target.value); setCity(""); }} />
                <Input inputSize="xsmall" label={t("provider.city")} placeholder={t("provider.statePlaceholder")} value={city} disabled={extracting || !country || !state.trim()} onChange={(e) => setCity(e.target.value)} />
              </div>
              <p className="m-0 text-help text-text-sub-600">{t("provider.regionHelp")}</p>
              <ProviderSessionFields alternate="per_request" mode={relayMode} minutes={relayMinutesText}
                count={countText} maxMinutes={120} disabled={extracting} invalidCount={invalidCount}
                onMode={setRelaySession} onMinutes={setRelayMinutes} onCount={setBatchCount} />
              {!canGenerateBatch && <p className="m-0 text-help text-text-sub-600">{t("provider.singleGateway")}</p>}
              {extractError && <p role="alert" className="m-0 text-help text-error-text">{extractError}</p>}
            </div> : source === "tnb" ? <TnbExtractForm value={tnb} onChange={(next) => { if (next.use_api_credentials !== tnb.use_api_credentials) authSwitchTouched.current.api = true; if (next.use_proxy_credentials !== tnb.use_proxy_credentials) authSwitchTouched.current.proxy = true; setTnb(next); setExtractError(null); }} disabled={extracting || !tnbDefaults} error={tnbDefaultsFailed ? t("provider.defaultsUnreadable") : extractError} /> : isCustomProvider ? <>
              {customExtraction && <Input
                inputSize="xsmall" type="number" min={1} max={1000}
                label={t("provider.customExtractLimit")} value={countText}
                onChange={(e) => setBatchCount(e.target.value)} disabled={extracting}
              />}
              {extractError && <p role="alert" className="m-0 text-help text-error-text">{extractError}</p>}
            </> : <>
            <Select
              label={t("proxyBulkImporter.defaultTypeLabel")}
              size="xsmall"
              value={kind}
              onChange={(v) => setKind(v as ProxyEntry["kind"])}
              options={[
                { value: "socks5", label: "SOCKS5" },
                { value: "http", label: "HTTP" },
                { value: "https", label: "HTTPS" },
              ]}
            />
            <Textarea
              label={t("proxyBulkImporter.pasteLabel")}
              rows={12}
              className="mono"
              value={text}
              onChange={(e) => setText(e.target.value)}
              onPaste={(e) => {
                const pasted = e.clipboardData.getData("text");
                if (!pasted.trim()) return;
                e.preventDefault();
                const input = e.currentTarget;
                const merged = input.value.slice(0, input.selectionStart) + pasted + input.value.slice(input.selectionEnd);
                setText(merged);
                void parseText(merged, kind);
              }}
              placeholder={t("proxyBulkImporter.pastePlaceholder")}
            />
            <div className="flex flex-wrap items-center justify-between gap-2">
              <p className="m-0 text-secondary text-text-soft-400">{t("proxyBulkImporter.duplicatesNote")}</p>
              <Button variant="neutral" mode="stroke" size="3xsmall" onClick={enterManually}>{t("proxyBulkImporter.enterManually")}</Button>
            </div>
            </>}
          </>
        ) : (
          <>
            {source === "tnb" && extracted && <p className="m-0 text-secondary text-text-sub-600" data-tnb-preview>{t("tnb.preview", { n: rows.length, route: extracted.route === "first_hop" ? t("tnb.firstHop") : t("tnb.direct") })}</p>}
            {single && <fieldset disabled={busy || single.status === "testing"} className="m-0 grid min-w-0 grid-cols-2 gap-3 border-0 p-0">
              <div className="col-span-2">
                <ProxyFolderSelect value={single.entry.folder ?? ""} onChange={(folder) => changeSingle({ folder })} />
              </div>
              <Input inputSize="xsmall" wrapperClassName="col-span-2" label={t("proxyEditor.nameLabel")} value={single.entry.name} placeholder={single.entry.host && single.entry.port ? `${single.entry.host}:${single.entry.port}` : t("proxyBulkImporter.namePlaceholder")} onChange={(e) => changeSingle({ name: e.target.value })} />
              <SelectField label={t("proxyEditor.typeLabel")} value={single.entry.kind} onChange={(kind) => changeSingle({ kind })} options={[
                { value: "socks5" as const, label: "SOCKS5" },
                { value: "http" as const, label: "HTTP" },
                { value: "https" as const, label: "HTTPS" },
              ]} />
              <Input inputSize="xsmall" label={t("proxyEditor.countryLabel")} value={single.entry.country} onChange={(e) => changeSingle({ country: e.target.value })} />
              <Input inputSize="xsmall" label={t("proxyEditor.hostLabel")} value={single.entry.host} onChange={(e) => changeSingle({ host: e.target.value })} required />
              <NumField label={t("proxyEditor.portLabel")} value={single.entry.port || null} onClear={() => changeSingle({ port: 0 })} min={1} max={65535} onInvalid={setPortInvalid} onChange={(port) => changeSingle({ port })} />
              <Input inputSize="xsmall" label={t("proxyEditor.usernameLabel")} value={single.entry.username} onChange={(e) => changeSingle({ username: e.target.value })} />
              <Input inputSize="xsmall" label={t("proxyEditor.passwordLabel")} type="password" secret autoComplete="new-password" value={single.entry.password} onChange={(e) => changeSingle({ password: e.target.value })} />
            </fieldset>}
            <div className="flex items-center gap-3 pb-1.5">
              <Checkbox
                checked={allSel}
                label={t("proxyBulkImporter.selectedCount", { n: selCount, total: rows.length })}
                onChange={(e) =>
                  setRows((rs) => rs.map((r) => ({ ...r, selected: e.target.checked })))
                }
              />
              <div className="ml-auto flex gap-1.5">
                <Button variant="neutral" mode="stroke" size="3xsmall" disabled={busy || single?.status === "testing"} onClick={() => setRows([])}>{t("proxyBulkImporter.back")}</Button>
                <Button
                  variant="neutral"
                  mode="stroke"
                  size="3xsmall"
                  leftIcon={<RefreshIcon className="size-3.5" />}
                  onClick={testAll}
                  disabled={busy || invalid || single?.status === "testing"}
                  isLoading={busy}
                >
                  {busy ? t("proxyBulkImporter.testingAll") : t("proxyBulkImporter.testAll")}
                </Button>
                <Tooltip content={t("proxyBulkImporter.keepWorkingHint")}>
                  <Button
                    variant="neutral"
                    mode="stroke"
                    size="3xsmall"
                    onClick={() =>
                      setRows((rs) =>
                        rs.map((r) => ({ ...r, selected: r.status === "ok" }))
                      )
                    }
                  >
                    {t("proxyBulkImporter.keepWorkingOnly")}
                  </Button>
                </Tooltip>
              </div>
            </div>
            <WindowedRows
              className="max-h-[380px] overflow-y-auto py-2  overflow-x-hidden tile"
              count={rows.length}
              estimate={37}
              row={(i) => (
                <ImportRow key={`${rows[i].entry.host}:${rows[i].entry.port}:${i}`} r={rows[i]} i={i} onToggle={toggle} onTest={testOne} disabled={invalid} />
              )}
            />
          </>
        )}
      </div>
    </Modal>
  );
}

/// One parsed line. Memoized: a flush of results renders only the lines that changed.
const ImportRow = memo(function ImportRow({ r, i, onToggle, onTest, disabled }: {
  r: BulkRowState;
  disabled: boolean;
  i: number;
  onToggle: (i: number) => void;
  onTest: (i: number) => void;
}) {
  const t = useT();
  return (
    <div
      // A click anywhere on the row toggles its box, once (owner).
      onClick={(e) => { if (!(e.target as HTMLElement).closest("label,input,button,a,select")) onToggle(i); }}
      data-filled={r.status === "ok" || r.status === "fail" ? "" : undefined}
      className={`import-cols row-line row-line-hover relative cursor-pointer px-cell py-1.5 text-secondary${r.status === "ok" ? " bg-success-alpha-16/20" : r.status === "fail" ? " bg-error-alpha-16/20 opacity-85" : ""}`}
    >
      <Checkbox
        checked={r.selected}
        aria-label={t("proxyBulkImporter.selectRow", { host: `${r.entry.host}:${r.entry.port}` })}
        onChange={() => onToggle(i)}
      />
      {/* Owner feedback 5n: the same 12 px, muted look every
          other type+address pairing uses, not a colored badge —
          type and address stay one look everywhere they show. */}
      <ProxyKind kind={r.entry.kind} />
      <Tooltip content={`${r.entry.host}:${r.entry.port}${r.entry.username ? " @" + r.entry.username : ""}`}>
        <span className="min-w-0 overflow-hidden text-ellipsis whitespace-nowrap text-mono text-text-sub-600">
          {r.entry.host}:{r.entry.port}
          {r.entry.username && <span className="text-text-soft-400">, {r.entry.username}</span>}
        </span>
      </Tooltip>
      <div className="inline-flex items-center justify-end gap-1.5">
        {r.status === "idle" && <span className="text-text-soft-400">{t("proxyBulkImporter.notTested")}</span>}
        {r.status === "testing" && <span className="text-text-soft-400">{t("proxyBulkImporter.testingRow")}</span>}
        {r.status === "ok" && (
          <>
            <Tooltip content={t("proxyBulkImporter.tcpTime", { ms: String(r.tcp_ms) })}><Badge color="success" size="small">{t("proxyBulkImporter.active")}</Badge></Tooltip>
            {r.entry.kind === "socks5" && r.udp_ms != null && (
              <Tooltip content={t("proxyBulkImporter.udpRelayWorks", { ms: r.udp_ms })}><Badge color="primary" size="small">UDP</Badge></Tooltip>
            )}
            {r.country && (
              <>
                <CountryFlag cc={r.country} />
                <span className="text-text-sub-600">{r.country}</span>
              </>
            )}
          </>
        )}
        {r.status === "fail" && (
          <Tooltip content={r.error}><Badge color="error" size="small">{t("proxyBulkImporter.failed")}</Badge></Tooltip>
        )}
      </div>
      <IconButton variant="neutral" mode="stroke" size="3xsmall" onClick={() => onTest(i)} disabled={disabled || r.status === "testing"} icon={<RefreshIcon className="size-3.5" />} label={t("proxyBulkImporter.testRow")} />
    </div>
  );
});

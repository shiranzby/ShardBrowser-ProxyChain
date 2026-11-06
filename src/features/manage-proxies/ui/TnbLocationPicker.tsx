import { useMemo, useRef, useState } from "react";
import { Button, Combobox, Popover, type ComboboxOption } from "@proxyshard/shardx-ui-kit";
import { ChevronDownIcon } from "../../../shared/icons";
import { useLang, useT } from "../../../shared/i18n";
import type { TnbLocation } from "../../../entities/proxy";

/** Values come from TNB metadata. Labels are display-only; code is sent unchanged. */
export type TnbLocationState = {
  status: "idle" | "loading" | "ready" | "error" | "unavailable";
  items: TnbLocation[];
  errorCode?: string;
};
export type TnbLocationLevel = "country" | "region" | "city";

const countryNames = {
  en: new Intl.DisplayNames(["en"], { type: "region", fallback: "none" }),
  zh: new Intl.DisplayNames(["zh"], { type: "region", fallback: "none" }),
  ru: new Intl.DisplayNames(["ru"], { type: "region", fallback: "none" }),
};

function names(item: TnbLocation, level: TnbLocationLevel) {
  const code = item.code.toUpperCase();
  if (level !== "country" || !/^[A-Z]{2}$/.test(code)) return {};
  return { en: countryNames.en.of(code), zh: countryNames.zh.of(code), ru: countryNames.ru.of(code) };
}

export function TnbLocationPicker({ level, value, state, disabled, onLoad, onChange }: {
  level: TnbLocationLevel;
  value: TnbLocation | null;
  state: TnbLocationState;
  disabled: boolean;
  onLoad: () => void;
  onChange: (value: TnbLocation | null) => void;
}) {
  const t = useT();
  const lang = useLang((s) => s.lang);
  const [open, setOpen] = useState(false);
  const anchor = useRef<HTMLButtonElement>(null);
  const label = level === "country" ? t("tnbGeo.country") : level === "region" ? t("tnbGeo.region") : t("tnbGeo.city");
  const any = level === "country" ? t("tnbGeo.anyCountry") : level === "region" ? t("tnbGeo.anyRegion") : t("tnbGeo.anyCity");
  const searchLabel = level === "country" ? t("tnbGeo.searchCountry") : level === "region" ? t("tnbGeo.searchRegion") : t("tnbGeo.searchCity");
  const display = (item: TnbLocation) => {
    const translated = names(item, level);
    return translated[lang] || item.label;
  };
  const options = useMemo<ComboboxOption[]>(() => [
    { value: "0", label: any, keywords: ["0", "all", "any", "不限", "global"] },
    ...state.items.filter((item) => item.code !== "0").map((item) => {
      const translated = names(item, level);
      const label = translated[lang] || item.label;
      return { value: item.code, label, hint: item.code === label ? undefined : item.code, keywords: [item.code, item.label, ...Object.values(translated)].filter((s): s is string => !!s) };
    }),
  ], [state.items, level, lang, any]);
  const loading = state.status === "loading";
  const failure = state.errorCode === "tnb_locations_invalid_request" ? t("tnbGeo.invalid")
    : state.errorCode === "tnb_locations_timeout" ? t("tnbGeo.timeout")
      : state.errorCode === "tnb_locations_route_failed" ? t("tnbGeo.routeFailed")
        : state.errorCode === "tnb_locations_auth_required" || state.errorCode === "tnb_locations_http_status" ? t("tnbGeo.refused")
          : state.errorCode === "tnb_locations_too_large" || state.errorCode === "tnb_locations_invalid_response" ? t("tnbGeo.invalidResponse")
            : state.errorCode === "tnb_locations_busy" ? t("tnbGeo.busy") : t("tnbGeo.failed");
  const notice = state.status !== "ready" || state.items.length === 0;
  return (
    <div className="flex min-w-0 flex-col gap-[6px]" data-tnb-location={level}>
      <span className="text-secondary text-text-sub-600">{label}</span>
      <Button ref={anchor} variant="neutral" mode="stroke" size="xsmall" className="w-full" contentAlign="start" rightIcon={<ChevronDownIcon className="size-3.5" />} aria-label={label} aria-haspopup="dialog" aria-expanded={open} disabled={disabled} onClick={() => {
        if (!open && (state.status === "idle" || state.status === "error")) onLoad();
        setOpen(!open);
      }}>
        {value ? display(value) : any}
      </Button>
      <Popover open={open && !disabled} onOpenChange={setOpen} anchorRef={anchor} align="start" aria-label={label} className="w-[300px] p-0">
        <Combobox
          aria-label={searchLabel}
          placeholder={searchLabel}
          options={options}
          value={value?.code ?? "0"}
          maxListHeight={220}
          emptyText={t("tnbGeo.noMatch")}
          onChange={(code) => {
            const item = state.items.find((entry) => entry.code === code);
            if (code !== "0" && (!item || state.status !== "ready")) return;
            onChange(code === "0" ? null : item!);
            setOpen(false);
          }}
        />
        {notice && <div className="px-3 py-2" role={state.status === "error" ? "alert" : "status"}>
          {loading && <p className="m-0 text-secondary text-text-soft-400">{t("tnbGeo.loading")}</p>}
          {state.status === "error" && <div className="flex flex-col gap-2"><p className="m-0 text-secondary text-error-text">{failure}</p><Button size="xsmall" variant="neutral" mode="stroke" disabled={loading} onClick={onLoad}>{t("tnbGeo.retry")}</Button></div>}
          {state.status === "ready" && state.items.length === 0 && <p className="m-0 text-secondary text-text-soft-400">{t("tnbGeo.noLocations")}</p>}
          {state.status === "unavailable" && <p className="m-0 text-secondary text-text-soft-400">{t("tnbGeo.unavailable")}</p>}
        </div>}
      </Popover>
    </div>
  );
}

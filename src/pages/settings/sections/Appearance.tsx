import { Checkbox, SegmentControl, Switch, type Theme } from "@proxyshard/shardx-ui-kit";
import { COLLAPSIBLE_PAGES, type CollapsiblePage } from "../../../shared/model/simpleNavigation";
import { useT } from "../../../shared/i18n";
import { INTERFACE_SIZES, useInterfaceSize, type InterfaceSizeChoice } from "../../../shared/model/interfaceSize";
import { SelectField } from "../../../shared/ui/SelectField";
import { SettingRow, SettingsGroup, type SettingsProps } from "../SettingRow";

// Each shows at once, as a preview, and lives in this window's storage, not in
// settings.json: it is stored on Save, and Don't save puts the old one back.
export function AppearanceSection({ p, setP, saving }: SettingsProps) {
  const t = useT();
  const pageLabels: Record<CollapsiblePage, string> = {
    proxies: t("sidebar.navProxies"), fingerprints: t("sidebar.navFingerprints"),
    extensions: t("sidebar.navExtensions"), bookmarks: t("sidebar.navBookmarks"),
    automation: t("sidebar.navProjects"), proxyshard: t("sidebar.navAccountShop"),
  };
  const auto = useInterfaceSize((s) => s.auto);
  return (
    <SettingsGroup>
      <SettingRow
        id="theme"
        title={t("settings.themeTitle")}
        control={({ labelId }) => (
          // One choice that switches no panel: a radio group (C26).
          <SegmentControl
            fit
            aria-labelledby={labelId}
            size="xsmall"
            value={p.theme}
            onChange={(v) => setP("theme", v as Theme)}
            items={[
              { value: "light", label: t("settings.themeLight") },
              { value: "dark", label: t("settings.themeDark") },
              { value: "system", label: t("settings.themeSystem") },
            ]}
          />
        )}
      />
      <SettingRow
        id="interface-size"
        title={t("settings.interfaceSizeTitle")}
        help={t("settings.interfaceSizeHelp")}
        control={({ labelId, helpId }) => (
          <div className="w-[200px]">
            <SelectField
              aria-labelledby={labelId}
              aria-describedby={helpId}
              value={p.size}
              onChange={(v) => setP("size", v)}
              options={[
                { value: "auto" as InterfaceSizeChoice, label: t("settings.interfaceSizeAuto", { n: auto }) },
                ...INTERFACE_SIZES.map((n) => ({ value: n as InterfaceSizeChoice, label: t("settings.interfaceSizePercent", { n }) })),
              ]}
            />
          </div>
        )}
      />
      <SettingRow
        id="motion"
        title={t("settings.reduceMotion")}
        help={t("settings.motionHelp")}
        control={({ labelId, helpId }) => <Switch aria-labelledby={labelId} aria-describedby={helpId} checked={p.reduceMotion} onChange={(on) => setP("reduceMotion", on)} />}
      />
      <SettingRow
        id="simple-navigation"
        title={t("settings.simpleNavigation")}
        help={t("settings.simpleNavigationHelp")}
        control={({ labelId, helpId }) => <Switch aria-labelledby={labelId} aria-describedby={helpId} disabled={saving} checked={p.simpleNavigation} onChange={(on) => setP("simpleNavigation", on)} />}
        below={p.simpleNavigation ? <fieldset className="m-0 border-0 p-0" disabled={saving}>
          <legend className="mb-2 text-help text-text-sub-600">{t("settings.collapsedPages")}</legend>
          <div className="grid grid-cols-2 gap-2">
            {COLLAPSIBLE_PAGES.map((id) => <Checkbox key={id} label={pageLabels[id]} checked={p.collapsedPages.includes(id)}
              onCheckedChange={(on) => setP("collapsedPages", on ? [...p.collapsedPages, id] : p.collapsedPages.filter((x) => x !== id))} />)}
          </div>
        </fieldset> : undefined}
      />
    </SettingsGroup>
  );
}

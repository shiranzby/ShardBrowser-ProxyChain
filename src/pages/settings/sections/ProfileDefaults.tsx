import { Input, Switch, Textarea } from "@proxyshard/shardx-ui-kit";
import { useEffect } from "react";
import { defaultProfileDefaults, type ProfileDefaults } from "../../../entities/settings";
import { AUTO_LANG, AUTO_TZ, LOCALES, TIMEZONES } from "../../../shared/constants";
import { useT } from "../../../shared/i18n";
import { riskyLaunchArgs, type LaunchArgRisk } from "../../../shared/lib/launchArgs";
import { SelectField } from "../../../shared/ui/SelectField";
import { SettingRow, SettingsGroup, type SettingsProps } from "../SettingRow";
import { holdSave } from "../useSettingsDraft";
import { isStartupUrl } from "../../../shared/lib/startupUrl";

export function ProfileDefaultsSection({ s, setS, p, setP, saving }: SettingsProps) {
  const t = useT();
  const d = { ...defaultProfileDefaults(), ...s.profile_defaults };
  const urlError = isStartupUrl(d.startup_url) ? null : t("profileForm.startupUrlInvalid");
  useEffect(() => {
    holdSave("profile-default-startup-url", urlError);
    return () => holdSave("profile-default-startup-url", null);
  }, [urlError]);
  const setDefault = <K extends keyof ProfileDefaults>(key: K, value: ProfileDefaults[K]) => setS({ ...s, profile_defaults: { ...d, [key]: value } });
  return (
    <>
    <SettingsGroup title={t("settings.newProfileDefaultsTitle")} help={t("settings.newProfileDefaultsHelp")}>
      <SettingRow
        id="default-startup-url"
        title={t("settings.startupUrlTitle")}
        help={t("settings.startupUrlHelp")}
        below={<Input aria-label={t("settings.startupUrlTitle")} value={d.startup_url} error={urlError ?? undefined} disabled={saving} onChange={(e) => setDefault("startup_url", e.target.value)} />}
      />
      <SettingRow
        id="default-locale"
        title={t("settings.defaultLocaleTitle")}
        below={<div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
          <SelectField label={t("inlineEditor.languageLabel")} value={d.language} disabled={saving} isSearchable onChange={(v) => setDefault("language", v)} options={LOCALES.map((l) => ({ value: l.code, label: l.code === AUTO_LANG ? t("inlineEditor.languageAuto") : l.label }))} />
          <SelectField label={t("inlineEditor.timezoneLabel")} value={d.timezone} disabled={saving} isSearchable onChange={(v) => setDefault("timezone", v)} options={TIMEZONES.map((tz) => ({ value: tz, label: tz === AUTO_TZ ? t("inlineEditor.timezoneAuto") : tz }))} />
          <SelectField<ProfileDefaults["ui_language"]> label={t("inlineEditor.uiLanguageLabel")} value={d.ui_language} disabled={saving} onChange={(v) => setDefault("ui_language", v)} options={[
            { value: "system", label: t("inlineEditor.uiLanguageSystem") }, { value: "profile", label: t("inlineEditor.uiLanguageProfile") },
          ]} />
          <SelectField<ProfileDefaults["webrtc"]> label={t("inlineEditor.webrtcLabel")} value={d.webrtc} disabled={saving} onChange={(v) => setDefault("webrtc", v)} options={[
            { value: "auto", label: t("inlineEditor.webrtcAuto") }, { value: "tcp_only", label: t("inlineEditor.webrtcTcpOnly") }, { value: "block", label: t("inlineEditor.webrtcBlock") },
          ]} />
        </div>}
      />
      <SettingRow
        id="default-noise"
        title={t("inlineEditor.noiseHeading")}
        help={t("profileForm.noiseHelp")}
        below={<div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
          {([
            ["noise_canvas", "inlineEditor.noiseCanvas"], ["noise_webgl", "inlineEditor.noiseWebgl"],
            ["noise_audio", "inlineEditor.noiseAudio"], ["noise_client_rects", "inlineEditor.noiseClientRects"],
            ["noise_sensors", "inlineEditor.noiseSensors"], ["noise_fonts", "inlineEditor.noiseFonts"],
          ] as const).map(([key, label]) => <SelectField<"real" | "auto"> key={key} label={t(label)} value={d[key]} disabled={saving} onChange={(v) => setDefault(key, v)} options={[
            { value: "real", label: t("inlineEditor.noiseReal") }, { value: "auto", label: t("inlineEditor.noiseAuto") },
          ]} />)}
        </div>}
      />
    </SettingsGroup>
    <SettingsGroup>
      <SettingRow
        id="geo"
        title={t("settings.geoCheckerTitle")}
        help={t("settings.geoCheckerHelp")}
        control={({ labelId, helpId }) => (
          <div className="w-[240px]">
            <SelectField
              aria-labelledby={labelId}
              aria-describedby={helpId}
              value={s.geo_checker ?? "ip-api.com"}
              onChange={(v) => setS({ ...s, geo_checker: v })}
              options={[
                // Plain HTTP says so in a badge that is never cut, in the
                // list and in the closed field (SEC-12, controller ruling).
                { value: "ip-api.com", label: t("settings.geoIpApiCom"), badge: { label: t("settings.geoNotEncrypted"), color: "warning" } },
                { value: "ipapi.co", label: t("settings.geoIpapiCo") },
                { value: "ipwho.is", label: t("settings.geoIpwhoIs") },
              ]}
            />
          </div>
        )}
      />
      <SettingRow
        id="screen"
        title={t("settings.screenModeLabel")}
        help={t("settings.screenHelp")}
        control={({ labelId, helpId }) => (
          // As wide as its longest choice, so its help keeps one line.
          <div className="w-[180px]">
            <SelectField
              aria-labelledby={labelId}
              aria-describedby={helpId}
              value={s.screen_resolution_mode ?? "fingerprint"}
              onChange={(v) => setS({ ...s, screen_resolution_mode: v })}
              options={[
                { value: "fingerprint", label: t("settings.screenModeFingerprint") },
                { value: "real", label: t("settings.screenModeReal") },
              ]}
            />
          </div>
        )}
      />
      <SettingRow
        id="camera"
        title={t("settings.cameraSwitchLabel")}
        help={t("settings.cameraHelp")}
        control={({ labelId, helpId }) => (
          <Switch aria-labelledby={labelId} aria-describedby={helpId} checked={s.camera_enabled ?? true} onChange={(on) => setS({ ...s, camera_enabled: on })} />
        )}
      />
      <SettingRow
        id="profile-name"
        title={t("settings.profileNameLabel")}
        help={t("settings.profileNameHelp")}
        control={({ labelId, helpId }) => (
          <Switch aria-labelledby={labelId} aria-describedby={helpId} checked={s.show_profile_name ?? true} onChange={(on) => setS({ ...s, show_profile_name: on })} />
        )}
      />
      {/* Off, no password can be set; profiles that have one keep it. */}
      <SettingRow
        id="profile-passwords"
        title={t("settings.profilePasswordsLabel")}
        help={t("settings.profilePasswordsHelp")}
        control={({ labelId, helpId }) => (
          <Switch aria-labelledby={labelId} aria-describedby={helpId} checked={s.profile_passwords ?? false} onChange={(on) => setS({ ...s, profile_passwords: on })} />
        )}
      />
      {/* Lives in this window's storage, not in settings.json; changed on Save. */}
      <SettingRow
        id="direct-start"
        title={t("settings.directAsk")}
        help={t("settings.directHelp")}
        control={({ labelId, helpId }) => (
          <Switch aria-labelledby={labelId} aria-describedby={helpId} checked={p.askDirect} onChange={(on) => setP("askDirect", on)} />
        )}
      />
      <SettingRow
        id="extra-args"
        title={t("settings.extraArgsTitle")}
        help={t("settings.extraArgsHelp")}
        below={
          <>
            <Textarea
              aria-label={t("settings.extraArgsTitle")}
              rows={3}
              className="mono"
              value={s.extra_args ?? ""}
              // Part of the draft like every field; the question about a
              // risky switch comes on Save (SEC-10).
              onChange={(e) => setS({ ...s, extra_args: e.target.value })}
              placeholder={"--disable-background-timer-throttling\n--window-size=1280,800"}
            />
            <RiskyArgsNote raw={s.extra_args ?? ""} />
          </>
        }
      />
    </SettingsGroup>
    </>
  );
}

/// Each risky switch in the field, named with what it does (SEC-10).
function RiskyArgsNote({ raw }: { raw: string }) {
  const t = useT();
  const risky = riskyLaunchArgs(raw);
  const riskText = (flag: string, risk: LaunchArgRisk) =>
    risk === "proxy" ? t("settings.argProxy", { flag })
    : risk === "exec" ? t("settings.argExec", { flag })
    : risk === "debug" ? t("settings.argDebug", { flag })
    : risk === "profile" ? t("settings.argProfile", { flag })
    : risk === "extension" ? t("settings.argExtension", { flag })
    : risk === "cert" ? t("settings.argCert", { flag })
    : t("settings.argUnsafe", { flag });
  if (risky.length === 0) return null;
  return (
    <ul data-args-warnings className="m-0 mt-2 flex list-none flex-col gap-1 p-0">
      {risky.map((r) => (
        <li key={r.flag} className="flex items-start gap-1.5 text-secondary text-text-sub-600">
          <span aria-hidden="true" className="mt-1.5 size-[5px] shrink-0 rounded-full bg-warning-base" />
          {riskText(r.flag, r.risk)}
        </li>
      ))}
    </ul>
  );
}

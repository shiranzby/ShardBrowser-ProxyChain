export type Settings = {
  browser_path: string | null;
  theme: string;
  geo_checker?: string | null;
  screen_resolution_mode?: string | null;
  api_enabled?: boolean;
  api_port?: number;
  /** Shard Helper: offer to fill forms a generated identity fits. */
  helper_enabled?: boolean;
  /** Which field kinds it reacts to. Empty means all of them. */
  helper_triggers?: string[];
  /** Profile's camera is ShardBrowser's rather than the machine's. */
  camera_enabled?: boolean;
  /** Hide to the tray on close instead of quitting. */
  minimize_to_tray?: boolean;
  /** Appended to every launch, one per line. Applied last, so a repeat wins. */
  extra_args?: string;
  /** Ask the Web Store about linked extensions every 5 h (on when absent). */
  ext_update_check?: boolean;
  /** First hop used to connect a profile's existing proxy. Missing means off. */
  proxy_chain?: ProxyChainSettings;
};

export type ProxyChainSettings = {
  /** An unreadable saved chain must be explicitly repaired before use. */
  invalid?: boolean;
  enabled: boolean;
  mode: "auto" | "http" | "socks5";
  host: string;
  port: number;
  username: string;
  /** A saved credential exists; its plaintext is never returned by settings_get. */
  password_saved: boolean;
  /** Write intent: absent keeps it, a nonempty value replaces it, null clears it. */
  password?: string | null;
};

export const defaultProxyChain = (): ProxyChainSettings => ({
  enabled: false, mode: "auto", host: "127.0.0.1", port: 7890, username: "", password_saved: false,
});

export type ProxyChainTestKind = "local" | "system" | "profile";
export type ProxyChainStage = {
  state: "not_tested" | "ok" | "failed";
  latency_ms?: number | null;
  error_code?: string | null;
  error_message?: string | null;
};

export type ProxyChainTestResult = {
  kind: ProxyChainTestKind;
  proxy_kind?: "http" | "socks5" | null;
  proxy_address?: string | null;
  source?: string | null;
  resolved_for?: string | null;
  profile_proxy_address?: string | null;
  profile_name?: string | null;
  local_network?: ProxyChainStage | null;
  system_proxy: ProxyChainStage;
  profile_proxy: ProxyChainStage;
  exit_ip?: string | null;
  /** Live lookup on this test's route, never a cached or first-hop substitute. */
  exit_geo?: {
    ip: string;
    country: string;
    country_code: string;
    region: string;
    city: string;
    isp: string;
    timezone: string;
    latitude: number;
    longitude: number;
    provider: string;
  } | null;
  exit_error?: string | null;
  exit_error_code?: string | null;
  total_ms?: number | null;
  tested_at: string;
};

export type ProxyChainDetection = {
  state: "ok" | "manual_required" | "failed";
  kind?: "http" | "socks5" | null;
  host?: string | null;
  port?: number | null;
  source: string;
  has_bypass?: boolean;
  resolved_for?: string | null;
  error_code?: string | null;
  error_message?: string | null;
};

/** Where profiles, user-data, extensions and the trash live. */
export type DataRootInfo = {
  path: string;
  /** False while the data still sits in the config dir. */
  custom: boolean;
  migrating: boolean;
};

/** Progress of a data-root move, as `data-migration` events carry it. */
export type MigrationProgress = {
  phase: "scan" | "copy" | "verify" | "cleanup" | "done";
  done: number;
  total: number;
  percent: number;
  current: string;
};

/** The kinds the engine publishes. `label` is a translation key, not text —
 *  the settings page renders it through t(). */
export const HELPER_KINDS: { value: string; label: string }[] = [
  { value: "first_name",  label: "helperKinds.firstName" },
  { value: "last_name",   label: "helperKinds.lastName" },
  { value: "full_name",   label: "helperKinds.fullName" },
  { value: "email",       label: "helperKinds.email" },
  { value: "username",    label: "helperKinds.username" },
  { value: "phone",       label: "helperKinds.phone" },
  { value: "country",     label: "helperKinds.country" },
  { value: "city",        label: "helperKinds.city" },
  { value: "postal_code", label: "helperKinds.postalCode" },
  { value: "street",      label: "helperKinds.street" },
  { value: "birth_date",  label: "helperKinds.birthDate" },
  { value: "birth_day",   label: "helperKinds.birthDay" },
  { value: "birth_month", label: "helperKinds.birthMonth" },
  { value: "birth_year",  label: "helperKinds.birthYear" },
  { value: "gender",      label: "helperKinds.gender" },
];

export type ApiInfo = {
  enabled: boolean;
  port: number;
  base_url: string;
  token: string;
};

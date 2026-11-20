import { invoke } from "../../../shared/api/invoke";
import type { ProxyEntry, ProxyTestSnapshot } from "./types";

export const proxyList = () => invoke<ProxyEntry[]>("proxy_list");
export const proxySave = (entry: ProxyEntry) => invoke<ProxyEntry>("proxy_save", { entry });

/** A blank entry for the "new proxy" dialog. */
export const emptyProxy = (): ProxyEntry => ({
  id: "",
  name: "",
  kind: "socks5",
  host: "",
  port: 1080,
  username: "",
  password: "",
  provider_id: "manual",
  credential_ref: "",
  provider_options: {},
  capabilities: {},
  folder: "",
  country: "",
  notes: "",
});
export const proxyDelete = (id: string) => invoke("proxy_delete", { id });
export const proxyFullTest = (entry: ProxyEntry) => invoke<ProxyTestSnapshot>("proxy_full_test", { entry });
export const proxyLastTest = (id: string) => invoke<ProxyTestSnapshot | null>("proxy_last_test", { id });
export const proxyHistory = (id: string) => invoke<ProxyTestSnapshot[]>("proxy_history", { id });
export const proxyBulkParse = (text: string, kind: ProxyEntry["kind"]) => invoke<ProxyEntry[]>("proxy_bulk_parse", { text, kind });
export const proxyBulkSave = (entries: any[]) => invoke<number>("proxy_bulk_save", { entries });
export const proxyBulkImport = (text: string, kind: string) => invoke<number>("proxy_bulk_import", { text, kind });

export type ProxyFolder = { name: string; parent: string };
export type ProxyFolderState = { folders: ProxyFolder[] };
export const proxyFoldersList = () => invoke<ProxyFolderState>("proxy_folder_list");
export const proxyFolderAdd = (name: string, parent = "") => invoke<ProxyFolderState>("proxy_folder_add", { name, parent });
export const proxyFolderRename = (oldName: string, newName: string) =>
  invoke<ProxyFolderState>("proxy_folder_rename", { old: oldName, new: newName });
export const proxyFolderDelete = (name: string) => invoke<ProxyFolderState>("proxy_folder_delete", { name });
export const proxyFolderSetParent = (name: string, parent: string) =>
  invoke<ProxyFolderState>("proxy_folder_set_parent", { name, parent });

export type TnbExtractRequest = {
  server: string; plan_name: string; provider_name?: string; kind: "http" | "socks5"; count: number;
  geo: string; ttl: number; pre: boolean; route: "direct" | "first_hop";
  pool?: string; isp?: string; asn?: string; ip?: string; sid?: string; use_api_credentials?: boolean; use_proxy_credentials?: boolean;
};
export type TnbExtractResult = { entries: ProxyEntry[]; route: TnbExtractRequest["route"]; server: string; requested_count: number };
export const tnbProxyExtract = (request: TnbExtractRequest) => invoke<TnbExtractResult>("tnbproxy_extract", { request });

export type TnbLocation = { code: string; label: string };
export type TnbLocationsRequest = {
  server: string; route: TnbExtractRequest["route"]; level: "country" | "state" | "city";
  country?: string; state?: string;
};
export type TnbLocationsResult = { items: TnbLocation[]; supported: boolean; source: string };
export const tnbProxyLocations = (request: TnbLocationsRequest) => invoke<TnbLocationsResult>("tnbproxy_locations", { request });

export type CustomProviderSummary = { id: string; name: string };
export type ProviderDefaults = { custom_providers: CustomProviderSummary[]; name: string; folder: string; country: string; state: string; city: string; session_type: "sticky" | "rotating" | "per_request"; session_minutes: number; generation_count: number; relay: string; username: string; username_template: string; password_saved: boolean; tnb_server: string; tnb_plan: string; tnb_kind: "http" | "socks5"; tnb_api_username: string; tnb_api_password_saved: boolean; tnb_proxy_username: string; tnb_proxy_password_saved: boolean; custom_name: string; custom_server: string; custom_extract_url: string; custom_plan: string; custom_kind: "http" | "https" | "socks5"; custom_website: string; custom_whitelist: string; custom_api_username: string; custom_api_password_saved: boolean; custom_proxy_username: string; custom_proxy_password_saved: boolean };
export type ProviderUpdate = Omit<ProviderDefaults, "custom_providers" | "password_saved" | "tnb_api_password_saved" | "tnb_proxy_password_saved" | "custom_api_password_saved" | "custom_proxy_password_saved"> & { password?: string; tnb_api_password?: string; tnb_proxy_password?: string; custom_api_password?: string; custom_proxy_password?: string };
type ProviderDefaultsScope = { customProviderId?: string; create?: boolean; tnb?: boolean };
export const providerDefaultsGet = (scope: ProviderDefaultsScope = {}) => invoke<ProviderDefaults>("proxy_provider_defaults_get", scope);
export const providerDefaultsSave = (value: ProviderUpdate, scope: ProviderDefaultsScope = {}) => invoke<ProviderDefaults>("proxy_provider_defaults_save", { value, ...scope });
export const providerDeleteCustom = (customProviderId: string) => invoke<void>("proxy_provider_delete_custom", { customProviderId });
export const providerGenerate = (country: string, state: string, city: string, custom = false, customProviderId?: string, sessionMinutes?: number) =>
  invoke<ProxyEntry>("proxy_provider_generate", { request: { country, state, city, custom, custom_provider_id: customProviderId, session_minutes: sessionMinutes } });
export const providerGenerateMany = (
  country: string,
  state: string,
  city: string,
  count: number,
  sessionMinutes?: number,
  custom = false,
  customProviderId?: string,
) =>
  invoke<ProxyEntry[]>("proxy_provider_generate_many", {
    request: {
      country,
      state,
      city,
      count,
      session_minutes: sessionMinutes,
      custom,
      custom_provider_id: customProviderId,
    },
  });
export const providerExtractCustom = (customProviderId: string | undefined, count: number) =>
  invoke<ProxyEntry[]>("proxy_provider_extract_custom", { customProviderId, count });

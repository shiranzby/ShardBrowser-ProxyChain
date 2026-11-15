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

export type TnbExtractRequest = {
  server: string; plan_name: string; kind: "http" | "socks5"; count: number;
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

export type ProviderDefaults = { relay: string; username: string; username_template: string; password_saved: boolean; tnb_server: string; tnb_plan: string; tnb_kind: "http" | "socks5"; tnb_api_username: string; tnb_api_password_saved: boolean; tnb_proxy_username: string; tnb_proxy_password_saved: boolean };
export type ProviderUpdate = Omit<ProviderDefaults, "password_saved" | "tnb_api_password_saved" | "tnb_proxy_password_saved"> & { password?: string; tnb_api_password?: string; tnb_proxy_password?: string };
export const providerDefaultsGet = () => invoke<ProviderDefaults>("proxy_provider_defaults_get");
export const providerDefaultsSave = (value: ProviderUpdate) => invoke<ProviderDefaults>("proxy_provider_defaults_save", { value });
export const providerGenerate = (country: string, state: string) => invoke<ProxyEntry>("proxy_provider_generate", { request: { country, state } });

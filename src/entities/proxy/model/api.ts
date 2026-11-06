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
  pool?: string; isp?: string; asn?: string; ip?: string; sid?: string;
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

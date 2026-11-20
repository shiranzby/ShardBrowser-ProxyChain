import { invoke } from "@tauri-apps/api/core";
import type { PsMe, PsUsage, PsUsageRange } from "./types";

export const psGetKey = () => invoke<string>("ps_get_key");
export const psSetKey = (key: string) => invoke("ps_set_key", { key });
export const psMe = () => invoke<PsMe>("ps_me");
export const psOrders = (params: { status: string; offset?: number; limit?: number }) => invoke<any>("ps_orders", params);
export const psOrder = (id: number) => invoke<any>("ps_order", { id });
export const psRenew = (id: number) => invoke("ps_renew", { id });
export const psTrialOffer = () => invoke<{ show?: boolean; url?: string; trial_gb?: number }>("ps_trial_offer");
export const psProducts = () => invoke<any>("ps_products");
export const psCalculate = (params: { product: string; location: string | null; cycle: string | null; quantity: number; promoCode: string | null; addonsJson: string | null }) => invoke<any>("ps_calculate", params);
export const psPurchase = (body: any) => invoke<any>("ps_purchase", { body });
export const psAvailableCount = () => invoke<any>("ps_available_count");
export const psActive = (orderId: number) => invoke<any>("ps_active", { orderId });
export const psSignatureSet = (orderId: number, items: { ip: string; signature: string }[]) => invoke("ps_signature_set", { orderId, items });
export const psSetTag = (id: number, tag: string) => invoke("ps_set_tag", { id, tag });
export const psAddBandwidth = (id: number, amount: number, promoCode: string | null) => invoke("ps_add_bandwidth", { id, amount, promoCode });
export const psProfileTraffic = (proxyType: string) => invoke<any>("ps_profile_traffic", { proxyType });
export const psCountries = (proxyType: string) => invoke<any>("ps_countries", { proxyType });
export const psRegions = (proxyType: string, countryCode: string) => invoke<any>("ps_regions", { proxyType, countryCode });
export const psCities = (proxyType: string, countryCode: string, regionCode: string) => invoke<any>("ps_cities", { proxyType, countryCode, regionCode });
export const psResiIsps = (tier: string, country: string, region: string, city: string) =>
  invoke<any>("ps_resi_isps", { tier, country, region, city });
/** Usage history over a range (backend B40). Until the backend has it the
 *  call fails, and every Usage card shows the No data ghost. */
export const psUsage = (range: PsUsageRange) => invoke<PsUsage>("ps_usage", { range });

import { invoke } from "@tauri-apps/api/core";
import type { FingerprintEntry, GpuCompat, HostGlCaps } from "./types";

export const fingerprintList = () => invoke<FingerprintEntry[]>("fingerprint_list");
export const fingerprintDelete = (id: string) => invoke("fingerprint_delete", { id });
export const fingerprintTagsSave = (id: string, tags: string[]) => invoke<FingerprintEntry>("fingerprint_tags_save", { id, tags });
export const fingerprintImport = (jsonText: string, idHint: string | null) => invoke<FingerprintEntry>("fingerprint_import", { jsonText, idHint });
export const fingerprintExport = (id: string) => invoke<string>("fingerprint_export", { id });
export const fingerprintUpdate = (id: string, jsonText: string) => invoke<FingerprintEntry>("fingerprint_update", { id, jsonText });
export const fingerprintRollback = (id: string) => invoke<FingerprintEntry>("fingerprint_rollback", { id });
export const fingerprintDir = () => invoke<string>("fingerprint_dir");

/// Slow on the first call: it starts the engine off-screen and asks it what the
/// GPU supports. Cached in Rust against the engine build afterwards.
export const gpuCaps = (force = false) => invoke<HostGlCaps>("gpu_caps", { force });
/// id -> compatibility. Empty map means "not known" (probe never ran or failed),
/// which must be shown as no verdict rather than as a clean one.
export const gpuCompat = () => invoke<Record<string, GpuCompat>>("gpu_caps_compat");

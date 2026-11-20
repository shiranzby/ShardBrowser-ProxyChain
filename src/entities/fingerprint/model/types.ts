/// Library fingerprint backing the editor GPU select; payload supplies the coherent base.
export type FingerprintEntry = {
  id: string;
  label: string;
  platform: string;
  chrome: string;
  gpu: string;
  tag_color: string;
  builtin: boolean;
  /** Stable payload identity used to explain duplicate imports. */
  payload_sha256?: string;
  /** User-managed library metadata, excluded from exported payload JSON. */
  tags?: string[];
  /** Launcher schema checks, separate from the supplied payload and host probe. */
  field_checks?: Record<string, "missing" | "structure_valid" | "invalid">;
  /** Imported JSON is not runtime hardware evidence. */
  field_sources?: Record<string, "imported" | "unavailable">;
  payload: any;
};

/// What this machine's GPU really supports, as reported by the engine itself.
export type HostGlCaps = {
  renderer: string;
  vendor: string;
  webgl1: string[];
  webgl2: string[];
  engine_version: string;
};

/// What a fingerprint asks of this machine that the machine cannot give. `compatible:
/// false` is catchable: the extension is listed but getExtension() returns null.
export type GpuCompat = {
  compatible: boolean;
  missing_webgl1: string[];
  missing_webgl2: string[];
  /// WebGPU features the profile claims that this machine's adapter lacks.
  missing_webgpu: string[];
  profile_renderer: string;
};

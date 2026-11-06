/** Mirrors Rust profile_defaults::validate_startup_url: Unicode whitespace
 *  at the edges is trimmed, embedded control characters are refused. */
export function isStartupUrl(value: string): boolean {
  const raw = value.replace(/^\p{White_Space}+|\p{White_Space}+$/gu, "");
  if (!raw || raw === "about:blank") return true;
  if (/\p{Cc}/u.test(raw)) return false;
  try {
    const url = new URL(raw);
    return ["http:", "https:"].includes(url.protocol) && !!url.hostname;
  } catch { return false; }
}

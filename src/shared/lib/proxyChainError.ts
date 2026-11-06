type ChainError = { code?: string | null; error_code?: string | null; message?: string | null; error_message?: string | null };

/** Shared by settings, proxy history and launch failures. Unknown safe backend
 * errors keep their own message rather than becoming an unrelated offline error. */
export function proxyChainErrorText(t: (key: string) => string, error?: ChainError | null, scope?: "proxy" | "system"): string | undefined {
  const code = error?.code ?? error?.error_code;
  if (scope === "proxy") switch (code) {
    case "proxy_chain_profile_auth_failed": return t("profileRoute.errorProxyAuth");
    case "proxy_chain_profile_timeout": return t("profileRoute.errorProxyTimeout");
    case "proxy_chain_profile_connect_failed":
    case "proxy_chain_profile_failed": return t("profileRoute.errorProxyConnect");
    case "proxy_chain_profile_tls_failed":
    case "proxy_chain_profile_tls_timeout": return t("profileRoute.errorProxyTls");
    case "proxy_chain_profile_kind_unsupported": return t("profileRoute.errorProxyKind");
    case "proxy_chain_profile_geo_failed": return t("profileRoute.errorProxyGeo");
  }
  if (scope === "system") switch (code) {
    case "system_proxy_connect_failed":
    case "system_proxy_connect_refused": return t("profileRoute.errorSystemConnect");
    case "system_proxy_auth_failed": return t("profileRoute.errorSystemAuth");
    case "system_proxy_timeout": return t("profileRoute.errorSystemTimeout");
    case "system_proxy_io_failed":
    case "system_proxy_protocol_error": return t("profileRoute.errorSystemProtocol");
    case "system_proxy_tls_failed":
    case "system_proxy_tls_timeout": return t("profileRoute.errorSystemTls");
    case "proxy_chain_profile_geo_failed": return t("proxyChain.systemGeoFailed");
  }
  switch (code) {
    case "proxy_chain_routes_failed": return t("proxyChain.errorRoutesFailed");
    case "proxy_chain_same_proxy_failed": return t("proxyChain.errorSameProxyFailed");
    case "proxy_chain_policy_invalid": return t("proxyChain.errorPolicy");
    case "proxy_chain_profile_tls_failed":
    case "proxy_chain_profile_tls_timeout": return t("proxyChain.errorSecondTls");
    case "proxy_chain_manual_required": return t("proxyChain.errorManualRequired");
    case "proxy_chain_system_not_configured": return t("proxyChain.errorSystemNotConfigured");
    case "proxy_chain_system_detect_failed": return t("proxyChain.errorSystemDetectFailed");
    case "proxy_chain_windows_required": return t("proxyChain.errorWindowsRequired");
    case "proxy_chain_auto_resolve_failed": return t("proxyChain.errorAutoResolve");
    case "proxy_chain_auto_resolve_timeout": return t("proxyChain.errorAutoTimeout");
    case "proxy_chain_auto_direct_refused": return t("proxyChain.errorAutoDirect");
    case "proxy_chain_auto_ambiguous": return t("proxyChain.errorAutoAmbiguous");
    case "proxy_chain_auto_protocol_unsupported": return t("proxyChain.errorAutoProtocol");
    case "proxy_chain_auto_target_required": return t("proxyChain.errorAutoTarget");
    case "proxy_chain_invalid_config":
    case "proxy_chain_settings_unreadable": return t("proxyChain.errorSettings");
    case "proxy_chain_credentials_unavailable": return t("proxyChain.errorCredentials");
    case "system_proxy_connect_failed": return t("proxyChain.errorFirstConnect");
    case "system_proxy_tls_failed":
    case "system_proxy_tls_timeout": return t("proxyChain.errorFirstTls");
    case "system_proxy_connect_refused": return t("proxyChain.errorFirstRefused");
    case "system_proxy_auth_failed": return t("proxyChain.errorFirstAuth");
    case "system_proxy_timeout": return t("proxyChain.errorFirstTimeout");
    case "system_proxy_io_failed":
    case "system_proxy_protocol_error": return t("proxyChain.errorFirstProtocol");
    case "proxy_chain_profile_auth_failed": return t("proxyChain.errorSecondAuth");
    case "proxy_chain_profile_timeout": return t("proxyChain.errorSecondTimeout");
    case "proxy_chain_profile_connect_failed":
    case "proxy_chain_profile_failed": return t("proxyChain.errorSecondConnect");
    case "proxy_chain_profile_kind_unsupported": return t("proxyChain.errorSecondKind");
    case "proxy_chain_profile_required": return t("proxyChain.errorProfileRequired");
    case "proxy_chain_stream_closed":
    case "proxy_chain_idle_timeout": return t("proxyChain.errorTunnelClosed");
    case "proxy_chain_local_network_failed":
    case "proxy_chain_local_geo_failed": return t("proxyChain.localGeoFailed");
    case "proxy_chain_system_geo_failed": return t("proxyChain.systemGeoFailed");
    case "proxy_chain_profile_geo_failed": return t("proxyChain.profileGeoFailed");
    default: return error?.message ?? error?.error_message ?? undefined;
  }
}

/** Older launch IPC returns fixed English messages rather than a structured
 * code. Recognize only our chain-owned phrases before generic network regexes. */
export function proxyChainErrorFromText(raw: string): ChainError | undefined {
  const tagged = /^\[((?:proxy_chain_|system_proxy_)[a-z0-9_]+)\]\s*(.*)$/s.exec(raw);
  if (tagged) return { code: tagged[1], message: tagged[2] };
  const rules: ReadonlyArray<readonly [RegExp, string]> = [
    [/^The system proxy refused (?:CONNECT|SOCKS5 CONNECT)/i, "system_proxy_connect_refused"],
    [/^Could not connect to the system proxy\./i, "system_proxy_connect_failed"],
    [/^The system proxy (?:rejected|requires valid)/i, "system_proxy_auth_failed"],
    [/^The system proxy did not finish connecting in time\./i, "system_proxy_timeout"],
    [/^The system proxy closed or interrupted the connection\./i, "system_proxy_io_failed"],
    [/^The profile SOCKS5 proxy rejected (?:its username|authentication)/i, "proxy_chain_profile_auth_failed"],
    [/^The profile SOCKS5 proxy did not finish its handshake or CONNECT in time\./i, "proxy_chain_profile_timeout"],
    [/^The profile SOCKS5 proxy refused or could not complete the destination CONNECT request\./i, "proxy_chain_profile_connect_failed"],
    [/^The proxy chain connected, but its live exit IP could not be verified\./i, "proxy_chain_profile_geo_failed"],
    [/^The proxy-chain connection (?:was interrupted|closed after being idle)\./i, "proxy_chain_stream_closed"],
  ];
  const code = rules.find(([pattern]) => pattern.test(raw))?.[1];
  return code ? { code, message: raw } : undefined;
}

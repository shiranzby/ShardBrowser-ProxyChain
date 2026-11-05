# Per-profile route choice and HTTP/HTTPS second hops (pc5)

Each saved profile `_meta.proxy_chain_policy` and saved proxy
`proxy_chain_policy` accepts `inherit`, `enabled`, or `disabled`. Missing fields
mean `inherit`. Unknown values are errors, never an unchained fallback.

Precedence is **profile > assigned proxy > global `settings.proxy_chain.enabled`**.
`disabled` skips only the first hop: the original proxy binding, credentials,
fingerprint and data directory stay unchanged. It is not a command to clear the
proxy. If the profile originally has no proxy, the original unchained behavior
still applies. Saved edits affect the next start; active tunnels remain owned by
the running Session, including runtime proxy-swap restrictions.

UI: edit a profile, open Proxy, choose Proxy chain choice; or open a proxy's Peek,
press Edit and choose its default. A profile's explicit choice wins when multiple
profiles share one proxy. Global first-hop host/port/credentials remain shared;
this is not a separate first-hop server per profile.

Local API `POST /profiles` and `PATCH /profiles/{id}` accept
`proxy_chain_policy`. `GET /profiles/{id}` keeps it inside `_meta`; the list also
returns the policy. `POST /proxies` accepts the same field. Profile JSON
export/import, clone and the opaque team/sync payload retain the choice. Older
unpatched clients may discard fields they do not understand: use the patched
version on every machine that edits these records. No claim is made that an old
launcher understands the new policy.

Second-hop SOCKS5 and HTTP remain transparent byte tunnels through the first
proxy. A true HTTPS proxy requires TLS **inside the first-hop tunnel**, validated
against the original proxy host/IP with WebPKI roots. Only the in-memory loopback
view changes from HTTPS to HTTP; the saved entry stays HTTPS. The local listener
forwards HTTP requests/CONNECT through verified TLS, leaving second-hop
authentication with the engine/detection client. TLS failures do not retry HTTP,
disable certificate checks, or connect directly. Private/self-signed certificates
are not accepted by an untrusted override. First-hop credentials remain separate.

A supplier saying “HTTPS proxy” may mean a plain HTTP CONNECT proxy that carries
HTTPS websites. Select `http` only after verifying the actual endpoint; never
silently downgrade a saved `https` entry. An IP HTTPS endpoint must have a matching
IP certificate. HTTP Basic credentials are not protected by proxy TLS in HTTP mode.

pc5 still has TCP-only chains, no QUIC/UDP and no raw-CDP/automation launch in a
chained Session. First-hop HTTP/SOCKS5 routes may be automatic Windows settings or
manual remote endpoints; encrypted subscription protocols are not implemented.

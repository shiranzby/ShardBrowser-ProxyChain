# Per-profile route choice and HTTP/HTTPS second hops (pc8)

Each saved profile `_meta.proxy_chain_policy` and saved proxy
`proxy_chain_policy` accepts `inherit`, `enabled`, `disabled`, or `prefer_first_hop`. Missing fields
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
returns the policy. `POST /proxies` accepts the same field; its response and
`GET /proxies` also return it without credentials. Proxy history used as an
unchained launch fallback is filtered by the resolved profile policy, not only
the global switch. Team and local proxy deduplication both include the policy.
Profile JSON
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

pc8 still has TCP-only protected routes, no QUIC/UDP and no raw-CDP/automation launch in a
chained Session. First-hop HTTP/SOCKS5 routes may be automatic Windows settings or
manual remote endpoints; encrypted subscription protocols are not implemented.

On upstream v2.1.3, password-protected profiles keep the route choice in their
sealed content, not in the readable stub. Reading a locked stub does not disclose
its route; editing the choice (even to inherit) through the API requires the
profile password. Changing that choice updates the profile revision. The saved
profile-chain test does not unlock a protected profile and reports that a password
is required; a normal password-authorized start uses the decrypted policy.

The proxy Peek shows its saved default, not a claim about a running profile's
route. Simple navigation under Settings / Appearance is a separate local UI
preference: saved manually, complete navigation by default, and no proxy or profile
data changes. More keeps all secondary destinations available; the current one
remains visible when the group is folded, and extension update notices remain visible.

## Preferred first hop, always keeping the assigned proxy

`prefer_first_hop` is an explicit per-profile or per-proxy choice. It first
verifies the configured first-hop route to the assigned proxy. If that network
route fails, it tries a connection from this computer to the same assigned proxy,
with the same protocol and credentials, and requires both proxy handshake and
live Geo verification before starting. It never tries a direct website connection.
The existing `enabled` policy remains strict and has no alternative route.

When the resolved first-hop endpoint and assigned proxy match in normalized
host, port, protocol and actual credentials, this explicit policy connects to
that endpoint once. A matching public exit IP does not prove a matching endpoint
and is never used for this decision. A local proxy such as `127.0.0.1:7890` does
not expose its internal selected node through HTTP/SOCKS; no private account or
subscription state is inspected to guess it.

Both alternatives retain the protected fixed-target loopback tunnel, TCP-only
behavior, strict HTTPS certificate verification and runtime proxy-control
restrictions. The selected route stays fixed for that Session; the next start
may choose again. Changing the upstream node inside a separate local proxy app
can still affect that app's own connections; the Launcher cannot freeze its
private node selection.

The `route` field in test results/history reports `first_hop`,
`assigned_proxy_fallback` or `same_proxy_endpoint`; an alternative route keeps
`first_hop_error_code` when relevant. The interface shows that first attempt
separately and reports the route after a successful native start. No IP equality
is labeled as proof of identical proxy nodes. Unknown policy values, corrupt
settings/credentials, missing proxy bindings and locked-profile authorization
errors are not eligible for route recovery. Existing saved choices are not
automatically changed.

Older pc7 clients treat the new policy as invalid and refuse it. Use pc8 on
clients editing or starting profiles with this choice; switch back to a supported
explicit policy in pc8 before returning to pc7. Proxy identity and data stay intact.

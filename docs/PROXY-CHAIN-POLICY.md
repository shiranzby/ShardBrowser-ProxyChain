# Per-profile route choice and HTTP/HTTPS second hops (pc8)

## Live verification (pc10)

The relay is bound before testing. There is no discarded first-hop CONNECT
before the actual profile handshake. Preparation, profile tests, proxy-list
tests and Start use the same relay verification. Each test closes its own
listener and flows; it cannot close a running Session's relay.

The dedicated `example.com:443` probe is a reachability measurement, not an
exclusive browsing requirement. If that target fails, a valid fresh Geo
response through the same relay and credentials proves proxy transport and
the live exit. The measured Geo round trip supplies the reachability latency
in this case. It is not a retry of the probe or an alternative route. Explicit
authentication refusals still stop immediately. When both the probe and Geo
fail, the original probe error is retained; when only Geo fails, measured
connectivity remains visible but Start still refuses the unverified exit.

Typed tunnel errors are published before the local socket closes, so the
caller can identify a failed first hop instead of guessing from loopback EOF.
The settings-only system test likewise accepts live Geo evidence on the
resolved system proxy when its separate probe target is unavailable.

PC12 retries only TCP establishment once, before any proxy protocol bytes are
sent. Both attempts share the original connection deadline; the first uses at
most half the remaining time. This applies to detection, preparation and every
new browser flow, for both the first hop and a selected proxy used alone.
Hostnames are resolved once; the second attempt tries the resolved addresses in
reverse order so an unavailable first address cannot starve the other IP family.
Authentication, CONNECT refusals, malformed replies and TLS failures are not
replayed. An established connection is never switched to another route.

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
The existing `enabled` policy does not allow unconditional assigned-proxy
fallback. PC11 additionally permits the verified equivalence described below.

When the resolved first-hop endpoint and assigned proxy match in normalized
host, port, protocol and actual credentials, strict and preferred routes connect
to that endpoint once. A matching public exit IP does not prove a matching endpoint
and is never used as endpoint identity. A local proxy such as `127.0.0.1:7890` does
not expose its internal selected node through HTTP/SOCKS; no private account or
subscription state is inspected to guess it.

Both alternatives retain the protected fixed-target loopback tunnel, TCP-only
behavior, strict HTTPS certificate verification and runtime proxy-control
restrictions. The selected route stays fixed for that Session; the next start
may choose again. Changing the upstream node inside a separate local proxy app
can still affect that app's own connections; the Launcher cannot freeze its
private node selection.

The `route` field in test results/history reports `first_hop`,
`assigned_proxy_fallback`, `same_proxy_endpoint` or `same_exit_verified`; an alternative route keeps
`first_hop_error_code` when relevant. The interface shows that first attempt
separately and reports the route after a successful native start. No IP equality
is labeled as proof of identical proxy nodes. Unknown policy values, corrupt
settings/credentials, missing proxy bindings and locked-profile authorization
errors are not eligible for route recovery. Existing saved choices are not
automatically changed.

For `same_exit_verified`, PC11 compares fresh Geo evidence from the resolved,
frozen first-hop endpoint with the verified chain exit. Only when they agree
does it try the original assigned proxy alone. If that single route also
authenticates and returns the same fresh exit, its already-verified tunnel is
retained, and the chain is closed. Comparison has a six-second total budget,
separate from product connection deadlines. An unsuccessful optimisation keeps
the verified chain. A transport-failed chain can recover only when both fresh
single-route exits agree; explicit authentication refusal is never hidden.
Unknown exits, historical IPs and country labels cannot establish equivalence.

This means equal observed exits, not equal node identity. Credentials, proxy
binding and profile data remain unchanged. The retained route uses the assigned
proxy, not a mutable system-only connection. Each new test/start resolves and
checks again; it does not reuse an earlier equivalence decision.

Older pc7 clients treat the new policy as invalid and refuse it. Use pc8 on
clients editing or starting profiles with this choice; switch back to a supported
explicit policy in pc8 before returning to pc7. Proxy identity and data stay intact.

## Explicit per-profile connections (pc8)

An optional `_meta.proxy_route` selects a single `system`, `proxy` or `direct`
connection, or a `chain` whose first hop is `system`, a selected library `proxy`,
or `direct`, and whose second hop is the assigned `proxy` or `direct`. PC11 also
allows second-hop `system` only after a `direct` first hop; runtime normalises
that selection to the single system route while the editor keeps the selection.
Second-hop `system` after `system` or a selected first proxy is rejected. The
original `proxy_id` remains the selected final proxy and can be kept while an
inactive route does not use it. The first selected proxy has its own library ID.
Missing route data preserves legacy policy inheritance; malformed data refuses.

These explicit choices override the global chain switch. A direct connection
disables Chromium's explicit proxy but cannot bypass OS VPN/TUN routing. With a
direct second hop, the first proxy remains the website exit. System-only routes
resolve the Windows proxy/PAC for every CONNECT target and reject PAC DIRECT;
changing Windows settings affects later connections. Selected proxy routes keep
their startup endpoint. Both proxy routes use tracked TCP-only tunnels, validated
TLS and runtime proxy-control restrictions. No failure chooses website direct.

The profile editor tests its unsaved route without saving a proxy or profile.
Its result belongs to that draft, not a library proxy's historical test. Routes
are sealed with protected profile settings and omitted from public team cards.
Using a selected first hop in a team requires access to that proxy; unavailable
or unauthorized entries refuse the route.

## Startup tab reuse

Profiles with a startup URL load a local tabs-only helper. Its temporary page
checks restored/open tabs, including pending navigation URLs, and reuses an exact
normalized URL match. If absent, it navigates its own temporary tab to the saved
URL. It does not close other websites or remove pre-existing duplicate user tabs.
Query strings and fragments remain part of the match. Multiple restored helper
pages serialize their decisions with an origin-scoped Web Lock.

The helper has no host permissions, content scripts, background worker or network
requests. It uses no product DevTools port and does not change proxy protections.
Its files live in each profile's user-data directory, separate from user-installed
extensions. Clearing the startup URL retires its saved target. Browser session
restore preferences stay unchanged. If the helper cannot query tabs, its page
offers Retry instead of blindly opening another target.

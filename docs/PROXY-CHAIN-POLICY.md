# Profile proxy routes and HTTP/HTTPS transport

## Five configuration choices, four execution plans

The profile editor offers these explicit choices:

| Proxy chain | Connection | Execution plan |
|---|---|---|
| Off | Real IP direct | `Direct` |
| Off | System proxy | `System` |
| Off | Selected proxy | `Single` |
| On | System proxy, then selected proxy | `Chain` |
| On | Selected first proxy, then selected second proxy | `Chain` |

In chain mode, the first layer is either the system proxy or a selected library
proxy. The second layer is a selected proxy. Empty layers are not additional
editor choices. The final selected proxy remains the profile's `proxy_id`; a
selected first proxy has its own library ID.

Launch and profile connection tests compile saved or draft intent into the same
`ProfilePlan` and four `RoutePlan` kinds. Explicit routes override the global
chain switch. A direct connection disables Chromium's explicit proxy but cannot
bypass OS VPN/TUN routing. No proxy-route failure chooses website direct.

The profile editor tests its unsaved route without saving a proxy or profile.
Its result belongs to that draft, not a library proxy's historical test. The
settings test likewise uses its settings draft; saved credentials can complete
that draft without replacing its route choice.

## Compatibility with saved routes and policies

Older `_meta.proxy_route` values remain readable. Planning normalizes empty
layers without rewriting their saved representation or inactive proxy bindings:

| Saved legacy chain | Effective route |
|---|---|
| Direct, then direct | Direct |
| Direct, then system | System |
| System, then direct | System |
| Direct, then assigned proxy B | Single proxy B |
| Selected proxy A, then direct | Single proxy A |

For the last case, editing the active single proxy updates A's stored selection;
it does not replace the retained, inactive B binding. A system second hop after
system or selected A is invalid. Missing route data preserves legacy policy
inheritance; malformed or contradictory route data refuses testing and Start.

Each saved profile `_meta.proxy_chain_policy` and saved proxy
`proxy_chain_policy` accepts `inherit`, `enabled`, `disabled`, or
`prefer_first_hop`. Missing fields mean `inherit`. Unknown values are errors.
For profiles without an explicit route, precedence is **profile > assigned proxy >
global `settings.proxy_chain.enabled`**. `disabled` skips the configured first
hop while keeping the assigned proxy, credentials, fingerprint and data directory.
If no proxy is assigned, the existing unchained behavior applies.

Saved edits affect the next start. Active tunnels remain owned by the running
Session, including runtime proxy-swap restrictions. Global legacy first-hop
settings are shared; an explicit route can select its own library first proxy.

Local API `POST /profiles` and `PATCH /profiles/{id}` accept the policy and optional
route. `GET /profiles/{id}` keeps the policy inside `_meta`; profile lists also
return the choice. `POST /proxies` accepts a default policy, and proxy responses
return it without credentials. Proxy history used as an unchained launch fallback
is filtered by the resolved profile policy, not only the global switch. Team and
local proxy deduplication include the policy. Profile JSON export/import, clone
and the opaque team/sync payload retain these choices. Older unpatched clients
may discard or reject unknown fields: use compatible patched clients on every
computer that edits these records.

## Verification and preparation lifetime

The relay is bound before testing. Preparation, profile tests and Start share the
prepared-route verification path. It first requests fresh Geo through the actual
relay and original credentials. Geo providers are tried serially under one
six-second Geo deadline. Success already proves transport, so there is no preceding
discarded CONNECT or separate successful-path TCP probe.

If Geo fails, the same prepared route may prove readiness with a bounded HTTPS
content check. Certificate and hostname verification, HTTP 200 and a nonempty body
are required. The content stage has its own six-second budget, with up to three
seconds per target; the Geo deadline is not a six-second total startup guarantee.
An authentication refusal is retained as an authentication error, not a location
provider outage. A TCP/protocol probe is only a diagnostic after both Geo and
HTTPS content verification fail.

A ready route without Geo keeps the current Geo failure and unknown exit
IP/location in detection and history. It does not borrow a historical IP, the
first hop's location, a country tag or this computer's location. Existing manual
fingerprint fields remain; unresolved automatic fields use the existing
proxy-unknown defaults (UTC, en-US, no automatic coordinates). No historical IP
is used for ICE.

The optional `route_ready` evidence is true only after this attempt proves live
Geo or verified HTTPS content. The interface shows a warning-colored
“Connected, location unknown” result when appropriate. Old history without this
field remains unknown. TCP-only partial results do not authorize Start. Each new
start verifies the current route rather than trusting history.

Before switching from Geo to HTTPS content, between failed content targets, and
before handing the prepared relay to the Engine, preparation cancels its owned
flows and waits for them to finish. A verification client's timeout must not leave
an upstream dial running after preparation. The listener stays available for the
next stage. This cleanup belongs only to a relay still in preparation; it does not
cancel business traffic in an existing Session. Completed tests close their own
relay and flows, never a running Session's relay.

Typed tunnel errors are published before the local socket closes, so the caller
can identify the failing hop instead of guessing from loopback EOF. Settings
system checks also retain live Geo evidence when a separate probe is unavailable.

TCP establishment may retry once, before any proxy protocol bytes are sent. Both
attempts share the original connection deadline. The first is limited to the
smaller of two seconds and half the remaining budget; the second uses the remaining
time. Hostnames are resolved once and the second attempt reverses address order.
Authentication, CONNECT refusals, malformed replies and TLS failures are not
replayed. An established connection is never switched to another route.

## Identical endpoints and explicit first-hop preference

When the resolved first hop and assigned proxy match in normalized host, port,
protocol and actual credentials, the route connects to that endpoint once. The
result reports `same_proxy_endpoint`, while saved binding and chain intent remain
unchanged. This avoids an unnecessary proxy self-connection.

Equal public exit IPs do not establish endpoint identity. A verified chain is kept
even if both proxies share an exit. A failed chain is no longer recovered by
comparing first-hop and assigned-proxy exits. `same_exit_verified` remains a
readable historical result value; new preparation does not emit it. Historical
IPs, countries and unavailable Geo cannot authorize an alternate route.

`prefer_first_hop` is a separate, explicit policy. It first tries the configured
route to the assigned proxy. If that route fails and the policy permits recovery,
it tries the same assigned proxy from this computer, with the same protocol and
credentials, and requires current route verification. This is not a direct website
connection. The normal `enabled` policy and ordinary explicit chains do not grant
this assigned-proxy fallback.

Results identify `first_hop`, `assigned_proxy_fallback`, `same_proxy_endpoint`,
`explicit_direct`, `explicit_system`, `explicit_proxy` or `explicit_chain` as
applicable. A fallback retains `first_hop_error_code` when relevant, and the
interface reports the chosen route. Invalid settings, unreadable credentials,
missing required bindings and locked-profile authorization are not repaired by
silently choosing another connection.

System-only routes resolve the Windows proxy/PAC for each CONNECT target and
reject PAC DIRECT; changing Windows settings affects later connections. In a
system-first chain, preparation resolves the first hop for the assigned proxy's
endpoint. Selected proxy routes keep their startup endpoint. A local proxy such
as `127.0.0.1:7890` does not expose its internal node through HTTP/SOCKS. The Launcher
does not inspect private account/subscription state or infer node identity from
matching exits. It cannot freeze another proxy app's internal node selection.

## HTTP, HTTPS and protected-route boundaries

Second-hop SOCKS5 and HTTP use transparent byte tunnels through the first proxy.
A true HTTPS proxy adds TLS **inside the first-hop tunnel**, validated against the
original proxy host/IP with WebPKI roots. Only the in-memory loopback view changes
from HTTPS to HTTP; the saved entry stays HTTPS. The local listener forwards HTTP
requests/CONNECT through verified TLS, leaving second-hop authentication with the
Engine/detection client. TLS failures never retry HTTP, disable certificate checks
or connect directly. Private/self-signed certificates are not accepted through an
untrusted override. First-hop credentials are separate. Selected HTTPS single
proxies use the same verified TLS boundary.

A supplier saying “HTTPS proxy” may mean a plain HTTP CONNECT proxy that carries
HTTPS websites. Select `http` only after verifying the actual endpoint; never
silently downgrade a saved `https` entry. An IP HTTPS endpoint needs a matching IP
certificate. HTTP Basic credentials are not protected by proxy TLS in HTTP mode.

Protected proxy routes use tracked TCP-only tunnels, no QUIC/direct UDP and no
raw-CDP/automation launch. First hops support HTTP, HTTPS or SOCKS5, with automatic
Windows resolution or explicit library/manual endpoints as applicable. Encrypted
subscription protocols such as HY2/VLESS are not implemented by this backend.

Password-protected profiles keep route and policy in sealed content, not the
readable stub. Reading a locked stub does not disclose its route. Editing these
choices through the API requires the profile password and updates the profile
revision. A test does not silently unlock a protected profile; password-authorized
testing or Start uses decrypted settings. A selected first proxy in a team requires
access to that proxy; unavailable or unauthorized entries refuse the route. Routes
are omitted from public team cards.

The proxy Peek shows its saved default, not a running profile's actual route.
Simple navigation under Settings / Appearance is an independent local UI
preference: saved manually, complete navigation by default, no proxy/profile data
changes. More keeps secondary destinations available; the current one and extension
update notices remain visible when the group is folded.

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

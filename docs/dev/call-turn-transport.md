# Private call transport

`CallTurn` uses the existing device-authenticated WebSocket. The relay admits a
current two-person, two-writer EndToEnd DM through one bounded FDB transaction
and verifies both Person profiles. It never reads encrypted call consent. This
credential authorizes a TURN allocation, not microphone/camera access or a call.
The Core and native RTC still require the confirmed invitation, selected devices
and DTLS fingerprint delivered through MLS. ICE remains relay-only.

The server reads `ZOEN_TURN_URLS` and `ZOEN_TURN_REST_SECRET` together. Missing
configuration explicitly disables allocation; partial/invalid configuration
fails startup. URLs are one to four `turns:` TLS endpoints, separated by commas.
Do not add STUN, plaintext TURN or direct fallback. Generate a random secret of
at least32 characters, keep it in protected service configuration, and configure
the same value as coturn's REST secret. Never return that secret to clients.

The issuer uses the standard coturn REST password MAC. The opaque per-device
username stays stable within a minute; credentials have at most120 seconds left.
The node's additional issuance buckets are6/minute burst4 per device and12/minute
burst8 per account. These limits do not replace coturn allocation/bandwidth quotas.
Debug formatting redacts credentials. No TURN password belongs in analytics,
logs, canonical messages, user-visible errors or screenshot evidence.

The [operator template](../../infra/local/coturn.conf.example) requires real TLS
files, host addresses, realm and secret before use. It bounds allocations and
bandwidth and denies private, loopback, link-local, multicast and translation
ranges. Pin and qualify the actual coturn release and enforce equivalent egress
rules at the network boundary. Do not add allow-list overrides that defeat those
denials. The template is not an installed service or measured capacity.

Short credential lifetime does not immediately terminate an existing allocation.
Call end, account/device retirement and membership removal must stop native RTC;
refresh cannot bypass Core authority. Test active media, reconnect and network
changes separately, including rejected private-peer permissions. A working MAC
or a compiled module does not prove a successful private call.

Source tests cover protocol round trips/URI bounds, an independent MAC vector,
expiry windows, issuer quotas, owner/context checks and real-FDB pair admission.
The latter runs with a disposable test cell in the existing relay contract suite.
Actual service, restricted-network RTC and physical iPhone evidence remain #284
and #266 acceptance items.

References: [coturn's maintained server documentation](https://github.com/coturn/coturn/blob/master/README.turnserver)
and [TURN URI standard](https://www.rfc-editor.org/rfc/rfc7065.html).

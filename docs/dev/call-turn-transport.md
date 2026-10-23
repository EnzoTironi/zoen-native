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
at least 32 characters, keep it in protected service configuration, and configure
the same value as coturn's REST secret. Never return that secret to clients.

The issuer uses the standard coturn REST password MAC. The opaque per-device
username stays stable within a minute; credentials have at most 120 seconds left.
The node's additional issuance buckets are 6/minute burst 4 per device and 12/minute
burst 8 per account. These limits do not replace coturn allocation/bandwidth quotas.
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

## Disposable real-server proof

The explicitly ignored Rust test
`production_credentials_relay_over_real_tls_coturn_and_invalid_auth_is_refused`
uses the production issuer with fresh synthetic credentials. It invokes
`scripts/call-turn-proof.py`, which owns an internal Docker network, TLS server,
upstream UDP echo peer and upstream clients. It publishes no host ports and never
changes the existing local relay stack. Documentation-only addresses preserve the
template's private-peer deny rules. The official coturn 4.18.0-r0 image is pinned
by manifest digest; pull it explicitly before running the test:

```sh
docker pull coturn/coturn@sha256:bbefd3e1fdfdc0d58770fe01b581fd8b00d9f3a5580d00acb77cf719a6bc78e3
```

Set `ZOEN_TURN_DOCKER_PROOF=1`, `ZOEN_TURN_PROOF_PYTHON` to an absolute Python 3
executable and `ZOEN_TURN_PROOF_DIR` to a new, empty absolute evidence directory.
Use the normal Cargo source guard, qualifying those three input variables, to run:

```sh
cargo test -p zoen-relay --lib turn::tests::docker::production_credentials_relay_over_real_tls_coturn_and_invalid_auth_is_refused -- --ignored --exact --nocapture
```

The proof requires verified TLS/hostname, echoed packets through an actual
allocation, and explicit TURN authentication rejection for wrong MAC and expired
production credentials. Timeout or unrelated failure cannot count as rejection.
Private issuer input, secret and certificate key are removed; logs redact the
ephemeral usernames/passwords. `report.json` records the pinned image, template,
case results, owned resource IDs and cleanup. This remains a transport proof;
it cannot certify real iPhone calls, carrier NAT, active-allocation revocation,
load capacity or the authenticated relay admission journey by itself.

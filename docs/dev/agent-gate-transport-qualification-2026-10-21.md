# Agent Gate transport qualification — 21 October 2026

The production CLI Gate previously used a default reqwest client, then awaited
the complete response without an explicit timeout or size ceiling. A stalled
peer could hold a turn indefinitely; a chunked body could grow before parsing.

The existing synchronous Gate transport now uses the same request envelope as
ModelBrain: 10 seconds for connection establishment, 60 seconds for the whole
request and reads, and 1 MiB for accumulated response bytes. Declared length and
each chunk are checked before extending the buffer. Network/configuration errors
retain fixed strings. Under-limit HTTP error bodies still reach the existing
Clef parser; endpoint, bearer, proxy, redirects and Ask/Block policy are unchanged.

This uses the existing reqwest path and bounded-reading pattern. It introduces
no executor, provider, production dependency, retry or environment setting.

## Executed evidence

Code head: `7051c905291b40acde1ba1037260577af7bd5b0a`.

```sh
cargo test --locked -p zoen-cli --bin zoen -- --test-threads=1
cargo clippy --locked -p zoen-cli --all-targets -- -D warnings
```

All six test functions pass; Clippy passes for all CLI targets. Owned ephemeral
loopback peers exercise the actual synchronous transport and reqwest client:

| Scenario | Required result |
| --- | --- |
| Exact byte cap; Content-Length/chunked; HTTP 200/500 | Exact body, POST path, authorization and JSON header retained |
| HTTP request received but headers withheld | Return before the peer's fallback close |
| Actual TLS ClientHello received; handshake withheld | Connect deadline expires before the longer total deadline |
| Body progresses every 75 ms, within the per-read limit | Total deadline still ends the request before body completion |
| Oversized declared body; bytes/EOF withheld; HTTP 200/500 | Reject without waiting for body or EOF |
| Oversized chunked body; terminal chunk/EOF withheld; HTTP 200/500 | Reject the stream before EOF |

The fixture uses short budgets with separation from its three-second fallback.
Assertions require the transport's bound to act before that fallback. Tests
disable proxies only for their own loopback clients. TLS certificate validation
is not bypassed.

Three isolated source controls also failed their intended assertions:

- Removing only the total deadline allowed the continuously progressing body to
  complete rather than time out.
- Widening only the connection budget missed the earlier handshake deadline.
- Widening only the response cap missed both early oversized-body rejections.

Each control restored the exact positive source and clean Git state. The final
unmodified six-test run passed again. Private receipts/logs and control hashes:
`/private/tmp/zoen-gate-runtime-v238/`.

The first run failed all six cases at client construction because isolated unit
tests bypass normal Net startup's crypto-provider initialization. It exercised
no HTTP/TLS assertions. The fixture now explicitly installs the same existing
Rustls ring provider, independent of test order. The only dependency addition is
a test-only reference to the already resolved Rustls package; the lockfile adds
one existing dependency edge. Production TLS configuration is unchanged.

## Limits

This proves bounded local transport behavior, not live-provider availability,
model correctness, charge cancellation, a durable cost ledger, immediate turn
revocation or the iPhone DeckCraft agent journey. A synchronous Gate can still
occupy its turn until this bounded request completes. No provider credentials,
external model requests or native device were used in this qualification.

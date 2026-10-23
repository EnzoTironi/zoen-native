# Retained Image output delivery

Issue #355 follows #325 and #349. Implementation is under qualification; written
tests are not evidence of execution. Production output and continuity gates stay
closed. No configured route or successful model result opens them.

An operator supplies a relay URL, expected Challenge name and the already-bound
relay source cell. The route is inert configuration. The native vault supplies
the certified device key; Apps, prompts and document payloads supply none of it.
Deployment qualification must independently establish that the configured URL
serves that actual source cell before the production output gate can open.

Delivery reads the original authenticated task, retained output Publication and
current native capsule. It never creates another item, FileDoc, encryption key,
model attempt or financial claim. Bounded catch-up ingests the actual configured
relay history before each effect. Incomplete catch-up returns its applied prefix;
changed membership, trust, source, expiry, device directory or fences refuse new
effects. This preserves the existing cross-store admission semantics: a request
admitted before revocation may complete, while later requests recheck authority.

The normal Net media worker and retained output transport share one signed blob
PUT implementation. Before sending, ciphertext must match its original hash and
size; retained Image chunks additionally verify decryption and plaintext hashes.
Only an actual HTTP 200/201 constructs an opaque upload receipt. Redirects,
refusals and missing responses cannot clear the pending-upload marker. The exact
ACK and native capsule progress activate through the existing directory/device
fences and the same Publication transaction. Ciphertext retention is never an ACK.

After every chunk has a real ACK, the normal upload-dependent outbox must offer
the original retained sealed envelope. The bounded Hello/Auth/Publish transport
uses the existing relay protocol and requires the configured Challenge name.
Accepted identifies a position; it is not an acceptance proof by itself. The
runtime reads that position from the actual configured source, checks the exact
envelope, ingests its ordered native history and commits the receipt with the
capsule. A lost ACK retries/observes that original, never signs a replacement.
Historical receipt replay checks the actual retained source position and creates
no new upload, publication, native generation or budget operation.

Written integration coverage uses the actual relay HTTP/blob/WebSocket handlers,
the same FdbLog instance, SQL directory and a local filesystem object store. A
bounded proxy discards real CREATED and real Accepted responses after upstream
commits, then cold runtime reopening must recover the same original. It also
checks source/Challenge mismatch, revoked trust before transfer, exact ciphertext
storage, unchanged one claim/settlement and inert receipt replay after device
revocation. This does not certify a public deployment, paid provider, iPhone UI,
device memory, restore continuity, orphan collection or production readiness.

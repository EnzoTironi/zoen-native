# Admitted image provider attempts

Issue #307 is the provider boundary for #179. It does not wire generation into
conversation tools, approvals, the image library or persistent File Items. It
makes no paid-provider, account-access, cost-settlement or iPhone claim.

`ModelGateway::preflight_image` and `generate_image` use OpenAI's configured
`/v1/images/generations` wire in Rig0.44, the existing `BoundedHttp` transport and
`DispatchAuthority`. The host explicitly configures a GPT Image model, credential
reference and HTTPS endpoint; only test fixtures permit numeric loopback HTTP.
No account credentials are copied from Codex or Cloudflare.

The request produces one PNG, with an explicit square/portrait/landscape size,
low/medium/high quality and opaque/transparent background. The shared authority
receives `IMAGE_PROFILE`, operation `ImageGeneration` and an `ImageDispatch`
specification containing those choices and the decoded-byte cap. The digest
binds the exact wire, context, model, endpoint, credential reference, output
specification and transport limits. A hashed owner identifier supplies the
provider safety identifier without sending the owner's identity directly.

The wire has no output-token cap: `requested_output_tokens=0` represents that
absence, not zero cost. Existing chat pricing rejects this profile/operation and
any image specification mislabeled as chat. Cheap chat quota rejects it before
consuming quota. A host must implement an explicit image pricing/admission rule
before enabling calls. This increment deliberately adds no parallel ledger,
approval store, retries or permissive financial authority.

The result retains exact provider PNG bytes and provenance metadata, MIME,
dimensions and SHA256 after signature, checksum, frame, decoded-size and dimension
validation. It accepts exactly one inline base64 image; remote URLs, animation,
substituted metadata and malformed/truncated media are rejected. The existing
request/response bounds remain in force; the decoded image is additionally capped
at 12MiB and PNG decoder resources at 16MiB. These are logical per-attempt bounds,
not a process RSS or total concurrency guarantee.

The same `ProviderReceipt` keeps actually reported token totals; separate typed
image details retain input/output image/text counters. Missing and invalid usage
remain distinct from reported zero. Unknown fields remain in bounded sensitive
provider evidence. Configured model names are never fabricated as reported
model names. No currency amount is calculated here. Rejected, incomplete,
over-limit or cancelled attempts can have unknown provider cost; only the
existing durable authority may reconcile them. No automatic retry or URL fetch
is performed.

Hermetic tests exercise real Rig encoding/decoding and the bounded HTTP seam,
including replay, cancellation, limits, policy denial and privacy canaries.
These tests cannot prove durable financial admission or paid provider delivery.
The product acceptance remains: actual authorized generation, canonical encrypted
File publication, selected reference edits/variants, recovery and exact export.

Official contracts consulted on 2026-10-23:

- [Images generation request and response](https://developers.openai.com/api/reference/typescript/resources/images/methods/generate).
- [Output formats, dimensions and quality](https://developers.openai.com/api/docs/guides/image-generation).


The supported response domain reserves exactly 65,536 bytes for normalized JSON
fields other than the canonical base64 string. The PNG budget must satisfy
`4 * ceil(png_bytes / 3) + 65,536 <= max_response_bytes`; both gateway preflight
and the bound image price profile reject incompatible limits before admission.
At the existing 16,777,216-byte response cap the maximum PNG is 12,533,760 bytes.
The 12 MiB raw PNG ceiling is a separate hard bound, not a claim that its full
range fits a 16 MiB JSON response. Normalized envelope fields above 65,536 bytes
are rejected while retaining provider usage/evidence. Transport still bounds the
actual raw JSON, including whitespace/escapes. This defines supported outputs;
it does not force the provider to respect a cost or output ceiling.

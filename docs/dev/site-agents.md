# Reviewed Site creation and editing

Sites use closed ToolProposal family 5, within the existing signed Requests,
Gate decisions and continuation lifecycle. Deck creation remains family 3 and
Images family 4. There is no Site publication tool or model-selected destination.

The three operations create a private HTML FileDoc, read a selected saved version,
or propose replacement HTML against that exact version/hash. The current certified
agent, human owner and triggering message are rechecked at every boundary. Writes
require an exact owner approval even when ordinary Gate policy allows them.
Current encrypted-space membership, writer access, source hash and base version
remain necessary after approval. A pending proposal never creates a file.

Model-facing HTML is initially limited to 16 KiB and the complete arguments to
32 KiB, within the signed ToolProposal capacity. This is separate from the 2 MiB
human Site-file limit. Discovery does not advertise larger files as usable agent
tools. Complete, verified UTF-8 bytes are read through the existing file APIs.

Creation reuses atomic file-send and attachment receipts. A bounded effect record
containing only identity/intent/Item/version/hash commits in the same transaction
as the file change. It contains no HTML or second document. Recovery validates the
exact saved bytes and attributed version before returning an existing effect;
absence never grants permission to replay an unknown write. Checked edits retain
the previous FileDoc version and cannot overwrite a changed base.

The human review is the existing request card with the disposable static Site
preview. Replacements offer Before and Proposed views. No HTML source is required
from the person. Approval stays unavailable until the exact signed proposal has
been read under the current human/device and that proposed version finishes loading
in the native preview. Module availability alone is insufficient. Each view has a
retirable lease; switching Before/Proposed, failure, process termination or a late
callback cannot approve an old view. Old native modules cannot unlock approval.
Losing focus, foreground, account, permissions or the canonical request retires
the preview. Reading it does not sign an approval or publication.

This is part of #336, #319 and #175. Compile/runtime/journey evidence is recorded
in the issue rather than inferred from source. The complete acceptance remains
create → approve → canonical v1 → cold read → reviewed edit → v2 → human-selected
publication. Mixed-family continuation recovery and large-source patch tools need
their own deliberate contracts; this family currently requires one held call per
batch. Interactive private Sites and a reusable visual editor remain separate
work. Physical rendering, keyboard, rotation and accessibility QA are pending.

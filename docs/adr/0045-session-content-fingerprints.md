# ADR 0045: incremental session dirty comparison

Status: accepted for S4-D1 maintenance, 2026-10-01.

Whole-document JSON serialization and SHA-256 dominate small edits on large
projects. Session dirty comparison may use ordered 512-object canonical JSON
SHA-256 chunks plus a signature covering document identity/format/source,
apertures and Block definitions. Layer IDs, layer order, object order, counts,
exposure, origin and every serialized geometry field remain represented.
This does not change disk digests or public schemas, and does not use GPU data.

Only successful core transactions publish a monotonic content generation and
a layer/index hint for modifications, plus a tail offset for object splices.
Object insertion/deletion/replacement invalidates the affected layer tail;
layer transactions and unknown/nonconsecutive generations rebuild signatures.
Header fields are checked at each new manufacturing revision. Board-only
transactions retain the document generation; Board dirty state continues via
the existing project state hash. Baselines advance only on successful open/save.

No revision-only dirty flag, floating tolerance, inverse-transform restore,
probabilistic XOR of unordered object hashes or hash eviction is used. Cache
clones preserve their baseline and generations. Signed zero remains distinct,
matching the old canonical JSON oracle. The bounded cache stores digest strings,
not a second copy of the large manufacturing document. Existing history limits
and atomic preflight remain unchanged. Test against an independent full JSON
oracle across edits, Undo/Redo, branches, failures and baseline changes.

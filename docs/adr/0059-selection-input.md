# ADR 0059: complete selection and press-time set operations

Status: source candidate; S5 selection input follow-up.

The user requested physical Ctrl+A and Ctrl-add/Shift-remove on both click and box. Click already captured modifiers; box discarded them, and no all-selection command existed.

Add `edit.select_all` to the shared command catalogue and the actual application's platform-specific default keymap. macOS physical Control is logical Secondary, distinct from Command/Primary. Keep schema v1 missing-command migration and user-binding conflict precedence, without altering historical fixtures. Cross-platform import transfers only this command's exact physical-Control default; custom logical bindings are preserved and explicit collisions reject import. Serialized source metadata and original keys stay unchanged; compilation and UI hints map only that default. Check original and effective explicit collisions, preserving default and custom logical-key roundtrips.

Keep the existing explicit Replace rectangle action for regression/native callers. Route a new modifier-aware canvas rectangle action and All through the existing serial read lane. Use complete manufacturing snapshot objects and existing visibility/selectability policy. A modified press on an object arms a rectangle, never Move/Grip; ordinary press behavior stays unchanged.

Merge borrowed objects by stable layer/object identity; build a new immutable selection only if its contents change. Keep geometry copies to one pass, insert cooperative checkpoints, and install only after a final checkpoint. Bind read replies to project identity and a strong prior selection Arc in addition to existing task/version/epoch fences. Unknown terminal state continues to isolate editing.

Alternatives rejected: substituting Command for requested Control, selecting a renderer/viewport subset, converting selection to an edit transaction, or mutating a published selection during a cancellable build. Real Mac input verification remains external.

### S5-I1 near-contour source amendment (2026-10-08)

`S5_NEAR_CONTOUR_SELECTION.md` freezes the authorized near-only analytic distance ranking while direct hits keep zero-tolerance layer/exposure priority, one-pass score/error equivalence and anchor-stable logical cycling. Default six physical pixels, geometry/Clear/Block meanings, ProbeDrag and PointMove ownership remain. Incoming Ctrl Add/box Add/All now excludes locks; Replace/Remove supports inspection and mixed edits still reject atomically. This supersedes the earlier All/Add inspection-lock behavior. Ordinary source/CPU checks are distinct from pending native mouse/build validation; existing acceptance IDs and thresholds are unchanged.

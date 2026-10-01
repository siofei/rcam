# ADR 0046: exact marquee selection without a fixed work admission cap

Accepted 2026-10-01 by user direction to remove resource admission limits and
accept slower very large resources. S4-D1 maintenance, not D2.

The fixed 2000000 cumulative rectangle-query work budget rejects legitimate
large layers even when the requested rectangle is small. Rectangle queries
now use an unlimited work counter; point queries and manufacturing boundary
helpers keep their existing bounded policy. Numerical ambiguity, invalid
coordinates, missing IDs and malformed geometry remain errors. Never return
partial IDs or approximate material from display data.

Expose additive nullable resource_limits.max_select_rect_work (null means no
fixed work cap). Existing max_hit_test_work continues to describe point query
work. No persistent schema, dependency, API version or frozen V1 threshold
changes. Unlimited processing still uses finite loops and available memory;
allocation failures are not made impossible.

GUI rectangle work remains on the existing versioned worker. Resolve selected
IDs from one immutable revision-bound snapshot and a per-layer map, preserving
layer/exposure order. Publish a replacement selection only after every query
and lookup succeeds. A full query may be slower on extremely large geometry;
no new hard latency promise is made.

Retain the old expensive macro fixture and change its former performance-cap
expectation to exact successful selection plus zero manufacturing mutation.
Retain all invalid/numerical rejection assertions. Add an independent 70000
capsule-line truth case exceeding old work admission, both modes/order, and
read-only regression on the same real twelve-layer and 527227-object project.
Native final-binary marquee evidence is separate from automated geometry truth.

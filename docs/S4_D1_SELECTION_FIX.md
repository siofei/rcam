# S4-D1 large-workspace multi-selection follow-up

Scope: S4-D1 maintenance, R09/R17/R19/R21/R22; regression references
AT-029/030/074/089/091. Allowed modules: editor-core hit-test work accounting
and exact rectangle query/tests; editor-service resource capabilities/tests;
editor-app rectangle selection snapshot lookup/tests; design/API/ADR/evidence.
No D2, schema migration, dependency or approximation.

User screenshot and native log reproduce RESOURCE_LIMIT select_rect_work at
2000000 while the project still has 527227 objects. Remove that fixed work cap
under the user's earlier explicit direction; preserve all geometry safety and
atomic all-or-error selection. Avoid quadratic object lookup for large result
sets. Preserve the user's active dirty native window and use a separate app
for acceptance. Inputs remain read-only. Re-run locked gates and package exact
clean source, public binary, raw evidence and per-file hashes together with
S4D1 and earlier performance fixes. Windows/full V1/CORE10/P100K not claimed.

Development read-only release observations: both modes return all 169482
set objects and 527227 project objects; local selection 120/233 and
1230/1394 objects. Local worker action time 125–281 ms, full selection
1265–4281 ms (not GPU-present latency). These are development results;
final clean identity and measurements belong to the versioned review.
Runtime selection logging retains total count and first 64 IDs rather than
printing half a million IDs on every subsequent action.

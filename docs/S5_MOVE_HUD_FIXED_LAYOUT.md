# Move canvas instruction layout

The Move phase instruction and the navigation instruction both used the canvas
top left plus `(12, 12)`. During click placement they painted on top of each other.
The canvas now has one bounded instruction slot. A valid Move instruction owns
it while placement is active; otherwise the navigation instruction appears.
All existing Move phase wording, including Esc, right click and Alt guidance,
is retained.

The slot uses a 12 point inset, 20 point rows and at most six rows, clipped to the
canvas and the existing painter clip. Long text wraps and is elided at the row
limit. The slot depends only on the viewport, so text and phase changes cannot
resize the canvas or neighbouring controls. Extremely small canvases necessarily
clip instructions; they do not enlarge panels or capture input.

This is painter-only presentation. It adds no Area, Response, interaction or
canvas allocation, and does not change input routing, Snap/Alt, preview/commit,
manufacturing coordinates, history or performance gates. Ordinary point picking
and other tools retain their existing paths.

CPU regressions inspect actual App output in preparing/following and after
cancellation, and actual phase paint output for all six Move phases. They cover
viewport size, DPI, themes, bounded long text, invalid context and unchanged
manufacturing state. Layout relates to R08/R18 and AT-025; compatibility also
relates to R09/R10/R11/R16 and AT-032/043/065/078. These tests do not complete
those acceptance cases and do not
claim native input, font, Metal or performance acceptance. Native checks must
use the frozen source and binary identity, especially CJK text in a small canvas.

The external CPU harness retains the product source with a documented auxiliary
platform adapter. A Linux dependency MSRV metadata override, when necessary for
that harness, is diagnostic evidence only and does not pass the product's locked
MSRV gate. Original failures must remain in private evidence.

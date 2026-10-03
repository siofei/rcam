# ADR 0056 — Transient world point workflows

Accepted for B implementation, validation pending, 2026-10-03. See S5_I2_B_PLAN for complete scope, fixture matrix and frozen gates. A final ca4f7ed is approved and its composite algorithm unchanged.

Use one transient f64 world-point/session abstraction and common input widget, four sources (numeric, contour pick, world envelope center, selected composite area centroid). Bind full task/selection context, restore prior point on child pick cancellation, and suppress selection/drag/Grip in child mode. Point values are not document fields. Every edit is an existing service SelectionEdit with a final worker context fence and one atomic transaction. A read-only preview shares existing core transform math; display outlines never become edit inputs.

Dedicated contour policy uses analytic standard aperture holes as well as existing boundary providers, cloned global preferences, 8/11 physical-pixel radius and selected-B/self-excluded-T. Navigation preserves confirmed world coordinates and resets screen hysteresis. Text/IME and cancellation precede manufacturing commit.

Adapt applicable existing tools without changing their distinct meanings: Block definition-local reference, text layout anchor, Array pitch and fixed Grip pivot remain separate from selection world pivot. C preferences/cursor/status/flicker remain a later independent stage. No new dependencies or project schema.

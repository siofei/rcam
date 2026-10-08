# ADR0060: click-placement Move using authoritative point tasks

Status: source candidate, S5 follow-up; native validation pending.

The user requested a Move flow that follows the pointer after starting and commits on one target click. Keep the existing numeric Move and drag paths, and add a shared command without changing their shortcuts.

Use the existing PointPreview/PointApply service path and selected manufacturing bounding-center. One admitted zero-delta preview supplies an immutable display template; following only paints a translation. A frozen actual f64 moved request must pass its own final preview before one service transaction. Display paths cannot become edit geometry or bypass validation.

Preserve a point-task owner after the visible session is retired. It fences task/source/project/selection identity, consumes retired previews and waits for authoritative late commit outcomes. Hold the first project transition until a valid terminal reply; unknown mutation outcomes fail closed. Strengthen point Context with strong selection and project identity using explicit O(1) comparisons.

Consequences: pointer following avoids repeated geometry copies and worker requests; admission/final preview can still fail under existing budgets, and an already Committing edit can finish after cancellation. The base is explicitly the complete manufacturing bounding-center; this command does not add another base-pick modal. See S5_MOVE_CLICK_PLACE.md for scope and ordinary verification, with actual platform input checks external.

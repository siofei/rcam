# ADR0057 — App interaction preferences and stable material status

2026-10-04. Accepted for implementation only; S5-I2-C not accepted. See S5_I2_C_PLAN.

Mouse movement and Grip editing are separate persisted application preferences. Legacy/default enabled preserves existing behavior; disabling suppresses mouse manufacturing arming and cancels its active preview before release, retaining selection/navigation/explicit commands. Canvas cursor preference is display-only and obeys egui layer ownership.

Status reads A SelectionCenters selected-only ordered layer composition. Area/perimeter are the same material result; holes count, overlaps deduplicate, cross-layer boundaries sum. Zero-area has no usable centroid. Pending/stale/error are explicit; object metric sums never masquerade as composition. Existing TaskVersion and selection identity fence/cache stay authoritative. Presentation units never round manufacturing inputs.

One nonwrapping fixed status line reserves right coordinate space before optional metrics. Dynamic messages/errors are bounded. This addresses proven layout instability; it does not establish or close the reported menu flicker, which requires lossless candidate-native noncanvas ROI investigation. No speculative GPU behavior change. No model/project/dependency/AT-schema changes.

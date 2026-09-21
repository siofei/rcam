//! Object Snap architecture (S4-B1 interfaces + resolver; providers land in S4-C).
//!
//! Snap points are never pre-generated for a whole document. The flow is
//! `screen radius -> world radius -> nearby objects (spatial query) -> lazy
//! features -> SnapResolver`. Features come from the *manufacturing boundary*,
//! never from GPU meshes or tessellation. Object Snap is distinct from Grip
//! Editing, but `SnapFeatureId` is stable so grips can reuse it later.

use crate::MmPoint;
use crate::workspace::{DisplayClass, LayerWorkspaceState};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapKind {
    Endpoint,
    Vertex,
    Midpoint,
    Center,
    Quadrant,
    ArcCenter,
    Intersection,
    Nearest,
    Perpendicular,
    Tangent,
}

impl SnapKind {
    /// Lower value wins ties: precise points beat derived ones.
    pub fn priority(self) -> u8 {
        match self {
            Self::Endpoint | Self::Vertex => 0,
            Self::Center | Self::ArcCenter => 1,
            Self::Midpoint => 2,
            Self::Quadrant => 3,
            Self::Intersection => 4,
            Self::Perpendicular | Self::Tangent => 5,
            Self::Nearest => 6,
        }
    }
}

/// Stable identity of a feature inside one object; reusable as a grip id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "index", rename_all = "snake_case")]
pub enum SnapFeatureId {
    Vertex(u32),
    Endpoint(u32),
    EdgeMidpoint(u32),
    ArcMidpoint(u32),
    ArcCenter(u32),
    Quadrant(u8),
    Center,
}

/// A stroke offers two families; strategy: manufacturing boundary by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapSource {
    ManufacturingBoundary,
    OriginalPath,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SnapFeature {
    pub id: SnapFeatureId,
    pub kind: SnapKind,
    pub point: MmPoint,
    pub source: SnapSource,
}

/// Fixed *screen* radius converted to world millimetres by the caller.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapQuery {
    pub center: MmPoint,
    pub radius_mm: f64,
    pub kinds: Vec<SnapKind>,
}

impl SnapQuery {
    pub fn from_screen(
        center: MmPoint,
        radius_px: f64,
        pixels_per_mm: f64,
        kinds: Vec<SnapKind>,
    ) -> Option<Self> {
        (radius_px > 0. && pixels_per_mm.is_finite() && pixels_per_mm > 0.).then(|| Self {
            center,
            radius_mm: radius_px / pixels_per_mm,
            kinds,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapCandidate {
    pub layer_id: String,
    pub object_id: String,
    pub feature: SnapFeature,
    pub distance_mm: f64,
}

/// Anything that can lazily produce snap features for a query.
pub trait SnapFeatureProvider {
    fn snap_features(&self, query: &SnapQuery) -> Vec<SnapFeature>;
}

/// A locked layer/class still snaps; a non-selectable or hidden one does not.
pub fn snap_allowed(state: &LayerWorkspaceState, class: DisplayClass) -> bool {
    state.effective_selectable(class)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SnapResolution {
    pub point: MmPoint,
    pub kind: Option<SnapKind>,
    pub feature: Option<SnapFeatureId>,
    pub from_grid: bool,
}

/// Object Snap beats Grid Snap; ties use screen distance, kind priority and a
/// hysteresis bonus for the feature that was snapped on the previous frame.
#[derive(Debug, Clone, Copy)]
pub struct SnapResolver {
    pub hysteresis_px: f64,
    pub kind_penalty_px: f64,
}

impl Default for SnapResolver {
    fn default() -> Self {
        Self {
            hysteresis_px: 3.,
            kind_penalty_px: 0.75,
        }
    }
}

impl SnapResolver {
    pub fn resolve(
        &self,
        raw: MmPoint,
        pixels_per_mm: f64,
        radius_px: f64,
        object_candidates: &[SnapCandidate],
        grid: Option<MmPoint>,
        previous: Option<(&str, SnapFeatureId)>,
    ) -> SnapResolution {
        let score = |candidate: &SnapCandidate| {
            let mut px = candidate.distance_mm * pixels_per_mm;
            px += f64::from(candidate.feature.kind.priority()) * self.kind_penalty_px;
            if previous == Some((candidate.object_id.as_str(), candidate.feature.id)) {
                px -= self.hysteresis_px;
            }
            px
        };
        let best = object_candidates
            .iter()
            .filter(|c| c.distance_mm * pixels_per_mm <= radius_px + self.hysteresis_px)
            .min_by(|a, b| score(a).total_cmp(&score(b)));
        if let Some(candidate) = best {
            return SnapResolution {
                point: candidate.feature.point,
                kind: Some(candidate.feature.kind),
                feature: Some(candidate.feature.id),
                from_grid: false,
            };
        }
        match grid {
            Some(point) => SnapResolution {
                point,
                kind: None,
                feature: None,
                from_grid: true,
            },
            None => SnapResolution {
                point: raw,
                kind: None,
                feature: None,
                from_grid: false,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::{Color, LayerKind};

    fn candidate(
        object: &str,
        id: SnapFeatureId,
        kind: SnapKind,
        x: f64,
        distance_mm: f64,
    ) -> SnapCandidate {
        SnapCandidate {
            layer_id: "layer-1".into(),
            object_id: object.into(),
            feature: SnapFeature {
                id,
                kind,
                point: MmPoint::new(x, 0.),
                source: SnapSource::ManufacturingBoundary,
            },
            distance_mm,
        }
    }

    #[test]
    fn screen_radius_becomes_world_radius() {
        let q = SnapQuery::from_screen(MmPoint::new(1., 2.), 8., 40., vec![SnapKind::Endpoint])
            .unwrap();
        assert!((q.radius_mm - 0.2).abs() < 1e-12);
        assert!(SnapQuery::from_screen(MmPoint::new(0., 0.), 8., 0., vec![]).is_none());
    }

    #[test]
    fn object_snap_beats_grid_and_grid_is_fallback() {
        let resolver = SnapResolver::default();
        let cands = [candidate(
            "o1",
            SnapFeatureId::Center,
            SnapKind::Center,
            5.,
            0.05,
        )];
        let hit = resolver.resolve(
            MmPoint::new(4.95, 0.),
            100.,
            8.,
            &cands,
            Some(MmPoint::new(5., 5.)),
            None,
        );
        assert_eq!(hit.kind, Some(SnapKind::Center));
        assert!(!hit.from_grid);
        let far = resolver.resolve(
            MmPoint::new(9., 0.),
            100.,
            8.,
            &[],
            Some(MmPoint::new(9., 1.)),
            None,
        );
        assert!(far.from_grid && far.point == MmPoint::new(9., 1.));
        let none = resolver.resolve(MmPoint::new(9., 0.), 100., 8., &[], None, None);
        assert_eq!(none.point, MmPoint::new(9., 0.));
    }

    #[test]
    fn kind_priority_and_hysteresis_prevent_jitter() {
        let resolver = SnapResolver::default();
        let corner = candidate("o1", SnapFeatureId::Vertex(0), SnapKind::Vertex, 0., 0.030);
        let mid = candidate(
            "o1",
            SnapFeatureId::EdgeMidpoint(0),
            SnapKind::Midpoint,
            1.,
            0.030,
        );
        let equal = resolver.resolve(
            MmPoint::new(0., 0.),
            100.,
            8.,
            &[mid.clone(), corner.clone()],
            None,
            None,
        );
        assert_eq!(
            equal.feature,
            Some(SnapFeatureId::Vertex(0)),
            "vertex outranks midpoint at equal distance"
        );
        // The mouse drifts so the midpoint is slightly closer, but the previous snap sticks.
        let mid_closer = candidate(
            "o1",
            SnapFeatureId::EdgeMidpoint(0),
            SnapKind::Midpoint,
            1.,
            0.028,
        );
        let sticky = resolver.resolve(
            MmPoint::new(0., 0.),
            100.,
            8.,
            &[mid_closer, corner],
            None,
            Some(("o1", SnapFeatureId::Vertex(0))),
        );
        assert_eq!(sticky.feature, Some(SnapFeatureId::Vertex(0)));
    }

    #[test]
    fn locked_layers_snap_but_non_selectable_ones_do_not() {
        let mut state = LayerWorkspaceState::new(LayerKind::Gerber, "ref", Color::rgb(1, 2, 3));
        state.locked = true;
        assert!(snap_allowed(&state, DisplayClass::FlashCircle));
        state.selectable = false;
        assert!(!snap_allowed(&state, DisplayClass::FlashCircle));
        state.selectable = true;
        state.visible = false;
        assert!(!snap_allowed(&state, DisplayClass::FlashCircle));
    }
}

//! Presentation cache only. Service and gesture commit validation remain authoritative.
use crate::{shared_snapshot::SnapshotVec, state::View, tools::DisplayUnit};
use std::{cell::RefCell, sync::Arc};

#[derive(Clone, PartialEq, Eq)]
struct Version {
    document: Option<(String, String, String, String)>,
    generation: u64,
    rule: u64,
    selection: u64,
}
impl Version {
    fn matches(&self, view: &View) -> bool {
        self.generation == view.task_generation
            && self.rule == view.rule_revision
            && self.selection == view.selection_epoch
            && match (&self.document, &view.info) {
                (Some((p, d, r, w)), Some(info)) => {
                    p == &info.project_id
                        && d == &info.document_id
                        && r == &info.revision
                        && w == &info.workspace_revision
                }
                (None, None) => true,
                _ => false,
            }
    }
    fn capture(view: &View) -> Self {
        Self {
            document: view.info.as_ref().map(|d| {
                (
                    d.project_id.clone(),
                    d.document_id.clone(),
                    d.revision.clone(),
                    d.workspace_revision.clone(),
                )
            }),
            generation: view.task_generation,
            rule: view.rule_revision,
            selection: view.selection_epoch,
        }
    }
}
struct Policy {
    version: Version,
    selected: SnapshotVec<editor_service::ObjectInfo>,
    layers: SnapshotVec<editor_service::LayerInfo>,
    apertures: SnapshotVec<editor_core::ApertureDefinition>,
    snapshot: Option<Arc<editor_service::RenderSnapshot>>,
    editable: Option<bool>,
    arrangement: Option<crate::state::ArrangementEligibility>,
    array: Option<bool>,
    block: Option<bool>,
}
impl Policy {
    fn matches(&self, view: &View) -> bool {
        self.version.matches(view)
            && self.selected.shares_storage(&view.selected.ordered)
            && self.layers.shares_storage(&view.layers)
            && self.apertures.shares_storage(&view.apertures)
            && match (&self.snapshot, &view.snap_snapshot) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
    }
}
struct Metrics {
    version: Version,
    selected: SnapshotVec<editor_service::ObjectInfo>,
    metrics: SnapshotVec<editor_service::MetricsItem>,
    error: Option<String>,
    unit: DisplayUnit,
    resolution: u64,
    lines: Vec<String>,
}
#[derive(Default)]
pub struct Cache {
    policy: RefCell<Option<Policy>>,
    metrics: RefCell<Option<Metrics>>,
    #[cfg(test)]
    pub calculations: std::cell::Cell<usize>,
}
impl Cache {
    /// Retire stale owners even when UI guards prevent all lazy queries (e.g. Close).
    pub fn synchronize(&self, view: &View) {
        if view.info.is_none() || view.scene.is_none() || view.blocked.is_some() {
            self.policy.borrow_mut().take();
            self.metrics.borrow_mut().take();
            return;
        }
        if self
            .policy
            .borrow()
            .as_ref()
            .is_some_and(|p| !p.matches(view))
        {
            self.policy.borrow_mut().take();
        }
        if self.metrics.borrow().as_ref().is_some_and(|m| {
            !m.version.matches(view)
                || !m.selected.shares_storage(&view.selected.ordered)
                || !m.metrics.shares_storage(&view.metrics)
                || m.error != view.metrics_error
        }) {
            self.metrics.borrow_mut().take();
        }
    }
    fn policy<R: Default>(&self, view: &View, query: impl FnOnce(&mut Policy) -> R) -> R {
        let mut cached = self.policy.borrow_mut();
        if view.blocked.is_some() || view.scene.is_none() {
            *cached = None;
            return R::default();
        }
        if cached.as_ref().is_none_or(|p| !p.matches(view)) {
            *cached = Some(Policy {
                version: Version::capture(view),
                selected: view.selected.ordered.clone(),
                layers: view.layers.clone(),
                apertures: view.apertures.clone(),
                snapshot: view.snap_snapshot.clone(),
                editable: None,
                arrangement: None,
                array: None,
                block: None,
            });
        }
        query(cached.as_mut().unwrap())
    }
    fn calculate<T>(&self, compute: impl FnOnce() -> T) -> T {
        #[cfg(test)]
        self.calculations.set(self.calculations.get() + 1);
        compute()
    }
    pub fn editable(&self, view: &View) -> bool {
        self.policy(view, |p| {
            *p.editable
                .get_or_insert_with(|| self.calculate(|| crate::drag::editable_selection(view)))
        })
    }
    pub fn arrangement(&self, view: &View) -> crate::state::ArrangementEligibility {
        self.policy(view, |p| {
            *p.arrangement.get_or_insert_with(|| {
                self.calculate(|| crate::state::arrangement_eligibility(view))
            })
        })
    }
    pub fn array(&self, view: &View) -> bool {
        self.policy(view, |p| {
            *p.array
                .get_or_insert_with(|| self.calculate(|| crate::array_ui::eligible(view)))
        })
    }
    pub fn block(&self, view: &View) -> bool {
        self.policy(view, |p| {
            *p.block.get_or_insert_with(|| {
                self.calculate(|| crate::block_ui::create_targets(view).is_ok())
            })
        })
    }
    pub fn metric_lines(&self, view: &View, unit: DisplayUnit, resolution: f64) -> Vec<String> {
        let mut cached = self.metrics.borrow_mut();
        let matches = cached.as_ref().is_some_and(|p| {
            p.version.matches(view)
                && p.selected.shares_storage(&view.selected.ordered)
                && p.metrics.shares_storage(&view.metrics)
                && p.error == view.metrics_error
                && p.unit == unit
                && p.resolution == resolution.to_bits()
        });
        if !matches {
            *cached = Some(Metrics {
                version: Version::capture(view),
                selected: view.selected.ordered.clone(),
                metrics: view.metrics.clone(),
                error: view.metrics_error.clone(),
                unit,
                resolution: resolution.to_bits(),
                lines: self.calculate(|| crate::metrics_panel::lines(view, unit, resolution)),
            });
        }
        cached.as_ref().unwrap().lines.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{selection::SelectionSet, state::Model};
    use editor_service::{MetricValue, MetricsItem, ObjectInfo};
    fn view(count: usize) -> View {
        let path = std::env::temp_dir().join(format!(
            "rcam-presentation-{}-{:?}.gbr",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::write(
            &path,
            b"%FSLAX46Y46*%\n%MOMM*%\n%ADD10C,1*%\nD10*\nX0Y0D03*\nM02*\n",
        )
        .unwrap();
        let mut model = Model::default();
        model.open(&path).unwrap();
        std::fs::remove_file(path).unwrap();
        let mut view = model.view;
        let mut snapshot = view.snap_snapshot.as_ref().unwrap().as_ref().clone();
        let template = snapshot.layers[0].objects[0].clone();
        snapshot.layers[0].objects = (0..count)
            .map(|i| {
                let mut o = template.clone();
                o.object_id = format!("synthetic-{i}");
                o
            })
            .collect();
        view.selected = SelectionSet {
            ordered: snapshot.layers[0]
                .objects
                .iter()
                .map(|o| ObjectInfo {
                    layer_id: snapshot.layers[0].id.clone(),
                    object: o.clone(),
                })
                .collect(),
        };
        view.snap_snapshot = Some(Arc::new(snapshot));
        view.metrics = view
            .selected
            .ordered
            .iter()
            .map(|o| MetricsItem {
                object_id: o.object.object_id.clone(),
                value: MetricValue::Exact {
                    area_mm2: 1.,
                    perimeter_mm: 2.,
                },
            })
            .collect();
        view.metrics_error = None;
        view
    }
    #[test]
    fn eighty_thousand_real_command_queries_and_repeated_frames_compute_once_per_query() {
        let view = view(80_000);
        let mut app = crate::modal::tests::app();
        app.view = view.clone();
        let expected = crate::metrics_panel::lines(&view, DisplayUnit::Millimeter, 0.0001);
        let selected_before = view.selected.clone();
        let array_expected = crate::array_ui::eligible(&view);
        for _frame in 0..240 {
            let commands = [
                editor_core::command::ids::OBJECT_MOVE,
                editor_core::command::ids::OBJECT_ROTATE,
                editor_core::command::ids::OBJECT_MIRROR,
                editor_core::command::ids::EDIT_DUPLICATE,
                editor_core::command::ids::EDIT_DELETE,
            ];
            for button in 0..41 {
                assert!(app.command_enabled(commands[button % commands.len()]));
            }
            assert!(app.selection_presentation.arrangement(&app.view).align);
            assert!(app.selection_presentation.arrangement(&app.view).distribute);
            assert_eq!(app.selection_presentation.array(&app.view), array_expected);
            assert!(app.selection_presentation.block(&app.view));
            assert_eq!(
                app.selection_presentation
                    .metric_lines(&app.view, DisplayUnit::Millimeter, 0.0001),
                expected
            );
        }
        assert_eq!(
            app.selection_presentation.calculations.get(),
            5,
            "five distinct computations, never per button/frame"
        );
        assert_eq!(app.view.selected, selected_before);
        assert_eq!(app.view.selected.ordered.len(), 80_000);
        assert!(
            app.view
                .selected
                .ordered
                .shares_storage(&view.selected.ordered)
        );
        assert!(app.view.metrics.shares_storage(&view.metrics));
    }
    #[test]
    fn policy_writes_detach_even_without_epoch_changes_and_rollback_revalidates() {
        let mut view = view(2);
        let original = view.clone();
        let cache = Cache::default();
        assert!(cache.editable(&view));
        view.layers[0].locked = true;
        assert!(!cache.editable(&view));
        view = original.clone();
        assert!(cache.editable(&view));
        view.layers[0].effective_visible = false;
        assert!(!cache.editable(&view));
        view = original.clone();
        view.layers[0].selectable = false;
        assert!(!cache.editable(&view));
        view = original.clone();
        let class = crate::state::Classifier::new(&view.layers, &view.apertures)
            .class(&view.selected.ordered[0]);
        view.layers[0]
            .classes
            .iter_mut()
            .find(|s| s.class == class)
            .unwrap()
            .locked = true;
        assert!(!cache.editable(&view));
        view = original.clone();
        view.selected.ordered[0].layer_id = "missing".into();
        assert!(!cache.editable(&view));
        view = original.clone();
        assert!(cache.editable(&view));
        view.blocked = Some("unconfirmed reply".into());
        assert!(!cache.editable(&view));
        view = original.clone();
        view.scene = None;
        assert!(!cache.editable(&view));
        assert!(cache.policy.borrow().is_none());
        view = original;
        assert!(cache.editable(&view));
    }
    #[test]
    fn metrics_pending_errors_values_units_and_same_count_replacement_invalidate() {
        let mut view = view(3);
        let cache = Cache::default();
        let check = |v: &View, unit, resolution| {
            assert_eq!(
                cache.metric_lines(v, unit, resolution),
                crate::metrics_panel::lines(v, unit, resolution)
            )
        };
        check(&view, DisplayUnit::Millimeter, 0.0001);
        view.metrics[1].value = MetricValue::Unsupported {
            reason: "unsupported".into(),
        };
        check(&view, DisplayUnit::Millimeter, 0.0001);
        view.metrics[0].value = MetricValue::Exact {
            area_mm2: 5.,
            perimeter_mm: 7.,
        };
        check(&view, DisplayUnit::Millimeter, 0.0001);
        check(&view, DisplayUnit::Mil, 0.0001);
        check(&view, DisplayUnit::Mil, 0.001);
        let complete = view.metrics.clone();
        view.metrics.clear();
        check(&view, DisplayUnit::Mil, 0.001);
        view.metrics_error = Some("explicit failure".into());
        check(&view, DisplayUnit::Mil, 0.001);
        view.metrics = complete;
        view.metrics_error = None;
        check(&view, DisplayUnit::Mil, 0.001);
        view.selected.ordered.reverse();
        check(&view, DisplayUnit::Mil, 0.001);
        assert_eq!(cache.calculations.get(), 9);
    }
    #[test]
    fn document_workspace_epoch_rules_and_snapshot_changes_invalidate() {
        let mut view = view(2);
        let cache = Cache::default();
        assert!(cache.editable(&view));
        view.info.as_mut().unwrap().document_id.push('x');
        assert!(cache.editable(&view));
        view.info.as_mut().unwrap().workspace_revision.push('x');
        assert!(cache.editable(&view));
        view.info.as_mut().unwrap().revision.push('x');
        assert!(cache.editable(&view));
        view.task_generation += 1;
        assert!(cache.editable(&view));
        view.rule_revision += 1;
        assert!(cache.editable(&view));
        view.selection_epoch += 1;
        assert!(cache.editable(&view));
        view.apertures[0].source_dcode += 1;
        assert!(cache.editable(&view));
        view.snap_snapshot = view
            .snap_snapshot
            .as_ref()
            .map(|s| Arc::new(s.as_ref().clone()));
        assert!(cache.editable(&view));
        assert_eq!(cache.calculations.get(), 9);
    }
    #[test]
    fn aggregation_keeps_original_order_and_unsupported_error_precedence() {
        let mut view = view(3);
        let cache = Cache::default();
        for (metric, area) in view.metrics.iter_mut().zip([1e16, 1., -1e16]) {
            metric.value = MetricValue::Exact {
                area_mm2: area,
                perimeter_mm: 2.,
            };
        }
        let first = cache.metric_lines(&view, DisplayUnit::Millimeter, 0.0001);
        view.metrics.swap(1, 2);
        let second = cache.metric_lines(&view, DisplayUnit::Millimeter, 0.0001);
        assert_ne!(first, second);
        assert_eq!(
            second,
            crate::metrics_panel::lines(&view, DisplayUnit::Millimeter, 0.0001)
        );
        view.metrics_error = Some("explicit error beats full values".into());
        assert_eq!(
            cache.metric_lines(&view, DisplayUnit::Millimeter, 0.0001),
            crate::metrics_panel::lines(&view, DisplayUnit::Millimeter, 0.0001)
        );
    }
    #[test]
    fn guards_release_old_owners_without_any_further_lazy_query() {
        for clear_scene in [false, true] {
            let mut view = view(2);
            let cache = Cache::default();
            let weak = Arc::downgrade(view.snap_snapshot.as_ref().unwrap());
            assert!(cache.editable(&view));
            let _ = cache.metric_lines(&view, DisplayUnit::Millimeter, 0.0001);
            if clear_scene {
                view.scene = None;
            } else {
                view = View::default();
            }
            cache.synchronize(&view);
            assert!(cache.policy.borrow().is_none());
            assert!(cache.metrics.borrow().is_none());
            drop(view);
            assert!(weak.upgrade().is_none());
        }
        let mut view = view(2);
        let cache = Cache::default();
        assert!(cache.editable(&view));
        view.selected.ordered.reverse();
        cache.synchronize(&view);
        assert!(cache.policy.borrow().is_none());
    }
}

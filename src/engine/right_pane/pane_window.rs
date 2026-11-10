//! Port of `src/views/pane_window.rs`: one owner for desired pane space,
//! observed native width, and resize requests, so opening the right pane grows
//! a narrow window and closing it gives the space back.

/// `views::WINDOW_MIN_WIDTH_PX`.
const WINDOW_MIN_WIDTH_PX: f32 = 800.0;

#[derive(Clone, Copy)]
enum PaneSpace {
    Existing,
    Added(f32),
}

#[derive(Clone, Copy)]
struct ResizeRequest {
    from: f32,
    target: f32,
    generation: u64,
}

#[derive(Clone, Copy)]
enum Transition {
    Queued(ResizeRequest),
    InFlight(ResizeRequest),
}

impl Transition {
    fn request(self) -> ResizeRequest {
        match self {
            Self::Queued(request) | Self::InFlight(request) => request,
        }
    }
}

#[derive(Default)]
pub struct PaneWindowSizing {
    base: Option<f32>,
    observed: Option<f32>,
    desired_pane: Option<f32>,
    space: Option<PaneSpace>,
    transition: Option<Transition>,
    generation: u64,
    native_acknowledged: bool,
}

impl PaneWindowSizing {
    pub fn sync(
        &mut self,
        width: f32,
        pane: Option<f32>,
        required: f32,
        maximum: f32,
        resizable: bool,
    ) -> Option<f32> {
        self.native_acknowledged = false;
        if !resizable {
            // Do not learn fullscreen/maximized bounds as the user's base size.
            return None;
        }
        let base = *self.base.get_or_insert(width);
        if let Some(transition) = self.transition {
            if matches!(transition, Transition::Queued(_))
                && (width - transition.request().from).abs() > 1.
                && (width - transition.request().target).abs() > 1.
            {
                self.transition = None;
            }
            if (width - transition.request().target).abs() <= 1. {
                self.transition = None;
                self.native_acknowledged = true;
            }
        }
        if self.transition.is_none()
            && !self.native_acknowledged
            && self.observed.is_some_and(|old| (old - width).abs() > 1.)
        {
            let added = match self.space {
                Some(PaneSpace::Added(delta)) => delta,
                _ => 0.,
            };
            let base = (width - added).max(WINDOW_MIN_WIDTH_PX);
            self.base = Some(base);
            if matches!(self.space, Some(PaneSpace::Added(_))) {
                self.space = Some(PaneSpace::Added((width - base).max(0.)));
            }
        }
        self.observed = Some(width);
        let changed = pane != self.desired_pane;
        self.desired_pane = pane;
        let base = self.base.unwrap_or(base);
        if pane.is_some() && self.space.is_none() {
            self.space = Some(if width >= required {
                PaneSpace::Existing
            } else {
                PaneSpace::Added(0.)
            });
        }
        let target = match (pane, self.space) {
            (Some(pane), Some(PaneSpace::Added(delta))) => {
                let delta = if changed || delta == 0. {
                    let target = (base + pane).max(required).min(maximum.max(base));
                    (target - base).max(0.)
                } else {
                    delta
                };
                self.space = Some(PaneSpace::Added(delta));
                base + delta
            }
            (None, Some(PaneSpace::Added(_))) => base,
            _ => width,
        };
        // A submitted native resize cannot be cancelled. Reconcile the latest
        // desired state after its acknowledgement instead of losing a reopen.
        if matches!(self.transition, Some(Transition::InFlight(_))) {
            return None;
        }
        if (target - width).abs() <= 1. {
            self.transition = None;
            if pane.is_none() {
                self.space = None;
            }
            return None;
        }
        let reuse = self
            .transition
            .is_some_and(|transition| (transition.request().target - target).abs() <= 1.);
        if !reuse {
            self.generation += 1;
            self.transition = Some(Transition::Queued(ResizeRequest {
                from: width,
                target,
                generation: self.generation,
            }));
        }
        Some(target)
    }

    pub fn queued_generation(&self) -> Option<u64> {
        match self.transition {
            Some(Transition::Queued(request)) => Some(request.generation),
            _ => None,
        }
    }

    /// Called by the deferred native effect. Obsolete callbacks do nothing.
    pub fn submit(&mut self, generation: u64) -> bool {
        match self.transition {
            Some(Transition::Queued(request)) if request.generation == generation => {
                self.transition = Some(Transition::InFlight(request));
                true
            }
            _ => false,
        }
    }

    /// No pane space held and no resize pending: nothing to reconcile.
    pub fn idle(&self) -> bool {
        self.desired_pane.is_none() && self.space.is_none() && self.transition.is_none()
    }

    pub fn automatic_resize(&self) -> bool {
        self.transition.is_some() || self.native_acknowledged
    }
    pub fn base_width(&self, current: f32) -> f32 {
        self.base.unwrap_or(current)
    }
    /// The width before the app widened the window for a pane, if it did.
    pub fn base(&self) -> Option<f32> {
        self.base
    }
    pub fn expanding(&self) -> bool {
        self.desired_pane.is_some()
            && self.transition.is_some_and(|transition| {
                let request = transition.request();
                request.target > request.from
            })
    }
    pub fn closing_width(&self) -> Option<f32> {
        self.transition
            .map(Transition::request)
            .filter(|request| request.target < request.from)
            .map(|request| request.target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_resize_transition_retains_chat_layout_until_size_is_observed() {
        let mut sizing = PaneWindowSizing::default();
        sizing.sync(780., Some(360.), 1124., 1600., true);
        assert!(sizing.expanding());
        sizing.sync(780., Some(360.), 1124., 1600., true);
        assert!(
            sizing.expanding(),
            "extra renders must not reveal an overlay"
        );
        sizing.sync(1140., Some(360.), 1124., 1600., true);
        assert!(!sizing.expanding());
        sizing.sync(1140., None, 764., 1600., true);
        assert_eq!(sizing.closing_width(), Some(780.));
        sizing.sync(1140., None, 764., 1600., true);
        assert_eq!(sizing.closing_width(), Some(780.));
        sizing.sync(780., None, 764., 1600., true);
        assert_eq!(sizing.closing_width(), None);
    }

    #[test]
    fn narrow_window_expands_once_and_restores_after_switching_pane_content() {
        let mut sizing = PaneWindowSizing::default();
        assert_eq!(
            sizing.sync(780., Some(360.), 1124., 1600., true),
            Some(1140.)
        );
        assert_eq!(sizing.sync(1140., Some(360.), 1124., 1600., true), None);
        assert_eq!(sizing.sync(1140., Some(360.), 1124., 1600., true), None);
        assert_eq!(sizing.sync(1140., None, 764., 1600., true), Some(780.));
        assert_eq!(sizing.sync(780., None, 764., 1600., true), None);
    }

    #[test]
    fn wide_windows_use_existing_space_and_manual_resize_updates_the_base() {
        let mut sizing = PaneWindowSizing::default();
        assert_eq!(sizing.sync(1400., Some(360.), 1124., 1600., true), None);
        assert_eq!(sizing.sync(1400., None, 764., 1600., true), None);
        assert_eq!(
            sizing.sync(800., Some(360.), 1124., 1600., true),
            Some(1160.)
        );
        assert_eq!(sizing.sync(1300., Some(360.), 1124., 1600., true), None);
        assert_eq!(sizing.base_width(1300.), 940.);
        assert_eq!(sizing.sync(1300., None, 764., 1600., true), Some(940.));
    }

    #[test]
    fn pane_resize_uses_the_original_chat_width_and_expansion_is_screen_bounded() {
        let mut sizing = PaneWindowSizing::default();
        sizing.sync(780., Some(360.), 1124., 1600., true);
        assert_eq!(
            sizing.sync(1140., Some(520.), 1284., 1600., true),
            Some(1300.)
        );
        assert_eq!(sizing.sync(1300., None, 764., 1600., true), Some(780.));
        assert_eq!(
            sizing.sync(780., Some(360.), 1124., 1000., true),
            Some(1000.)
        );
        assert_eq!(sizing.sync(1000., None, 764., 1000., true), Some(780.));
    }

    #[test]
    fn saved_width_excludes_only_app_added_space_even_with_a_screen_cap() {
        let mut sizing = PaneWindowSizing::default();
        sizing.sync(780., Some(360.), 1124., 1000., true);
        assert_eq!(sizing.base_width(780.), 780.);
        sizing.sync(1000., Some(360.), 1124., 1000., true);
        assert_eq!(sizing.base_width(1000.), 780.);
        sizing.sync(1200., Some(360.), 1124., 1600., true);
        assert_eq!(
            sizing.base_width(1200.),
            980.,
            "exclude the actual 220-point expansion"
        );
        assert_eq!(sizing.sync(1200., None, 764., 1600., true), Some(980.));
        assert_eq!(
            sizing.base_width(1200.),
            980.,
            "save the base even before collapse finishes"
        );
        sizing.sync(980., None, 764., 1600., true);
        assert_eq!(sizing.base_width(980.), 980.);
        sizing.sync(1400., Some(360.), 1124., 1600., true);
        assert_eq!(
            sizing.base_width(1400.),
            1400.,
            "a pane borrowing existing space adds nothing"
        );
    }

    #[test]
    fn reopen_after_submitted_collapse_reconciles_when_the_old_resize_arrives() {
        let mut controller = PaneWindowSizing::default();
        controller.sync(780., Some(360.), 1124., 1600., true);
        assert!(controller.submit(controller.queued_generation().unwrap()));
        controller.sync(1140., Some(360.), 1124., 1600., true);
        assert_eq!(controller.sync(1140., None, 764., 1600., true), Some(780.));
        assert!(controller.submit(controller.queued_generation().unwrap()));
        assert_eq!(controller.sync(1140., Some(360.), 1124., 1600., true), None);
        assert_eq!(
            controller.sync(780., Some(360.), 1124., 1600., true),
            Some(1140.)
        );
        controller.submit(controller.queued_generation().unwrap());
        controller.sync(1140., Some(360.), 1124., 1600., true);
        assert_eq!(controller.base_width(1140.), 780.);
    }

    #[test]
    fn close_before_expansion_ack_and_obsolete_queued_callbacks_are_safe() {
        let mut controller = PaneWindowSizing::default();
        controller.sync(780., Some(360.), 1124., 1600., true);
        let obsolete = controller.queued_generation().unwrap();
        controller.sync(780., None, 764., 1600., true);
        assert!(!controller.submit(obsolete));
        controller.sync(780., Some(360.), 1124., 1600., true);
        assert!(controller.submit(controller.queued_generation().unwrap()));
        controller.sync(780., None, 764., 1600., true);
        assert_eq!(controller.sync(1140., None, 764., 1600., true), Some(780.));
        controller.submit(controller.queued_generation().unwrap());
        assert_eq!(controller.sync(780., None, 764., 1600., true), None);
        assert_eq!(controller.base_width(780.), 780.);
    }

    #[test]
    fn fullscreen_defers_native_changes() {
        let mut sizing = PaneWindowSizing::default();
        assert_eq!(sizing.sync(1400., Some(360.), 1124., 1600., false), None);
        assert_eq!(
            sizing.sync(780., Some(360.), 1124., 1600., true),
            Some(1140.)
        );
        assert_eq!(sizing.sync(1600., None, 764., 1600., false), None);
        assert_eq!(sizing.sync(1140., None, 764., 1600., true), Some(780.));
    }
}

use std::time::{Duration, Instant};
use ahash::HashMap;
use winit::{window::WindowId,event_loop::ControlFlow};
const STALLED_WAYLAND_REDRAW_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Default)]
struct RedrawPollGuard {
    pending: HashMap<WindowId, PendingRedraw>,
}

struct PendingRedraw {
    first_request: Instant,
    is_wayland: bool,
}

impl RedrawPollGuard {
    fn requested(&mut self, window_id: WindowId, is_wayland: bool, now: Instant) {
        // Repeated repaint requests cannot extend a stalled window's deadline.
        self.pending.entry(window_id).or_insert(PendingRedraw {
            first_request: now,
            is_wayland,
        });
    }

    fn completed(&mut self, window_id: WindowId) {
        self.pending.remove(&window_id);
    }

    fn control_flow(&self, current: ControlFlow, now: Instant) -> ControlFlow {
        if current == ControlFlow::Poll
            && !self.pending.is_empty()
            && self.pending.values().all(|request| {
                request.is_wayland
                    && now.saturating_duration_since(request.first_request)
                        >= STALLED_WAYLAND_REDRAW_INTERVAL
            })
        {
            ControlFlow::Wait
        } else {
            current
        }
    }
}

#[cfg(test)]
mod redraw_poll_guard_tests {
    use super::*;

    #[test]
    fn repeated_requests_do_not_extend_the_stall_deadline() {
        let start = Instant::now();
        let window = WindowId::from(1);
        let mut guard = RedrawPollGuard::default();
        guard.requested(window, true, start);
        for millis in [1, 50, 99, 100, 500] {
            let now = start + Duration::from_millis(millis);
            guard.requested(window, true, now);
            assert_eq!(guard.pending[&window].first_request, start);
            assert_eq!(
                guard.control_flow(ControlFlow::Poll, now),
                if millis < 100 {
                    ControlFlow::Poll
                } else {
                    ControlFlow::Wait
                },
            );
        }
    }

    #[test]
    fn delivered_or_removed_windows_do_not_poison_later_requests() {
        let start = Instant::now();
        let window = WindowId::from(1);
        let mut guard = RedrawPollGuard::default();
        guard.requested(window, true, start);
        let resumed = start + Duration::from_secs(5);
        assert_eq!(
            guard.control_flow(ControlFlow::Poll, resumed),
            ControlFlow::Wait
        );
        guard.completed(window);
        // A long render completes before a new request starts its own budget.
        let after_render = resumed + Duration::from_millis(200);
        guard.requested(window, true, after_render);
        assert_eq!(
            guard.control_flow(ControlFlow::Poll, after_render),
            ControlFlow::Poll
        );
        guard.completed(window);
        guard.completed(window); // Destroyed/missing-window cleanup is idempotent.
        assert!(guard.pending.is_empty());
        assert_eq!(
            guard.control_flow(ControlFlow::Poll, after_render),
            ControlFlow::Poll
        );
    }

    #[test]
    fn a_progressing_window_keeps_polling_beside_an_occluded_window() {
        let start = Instant::now();
        let hidden = WindowId::from(1);
        let visible = WindowId::from(2);
        let mut guard = RedrawPollGuard::default();
        guard.requested(hidden, true, start);
        for frame in 1..20 {
            let now = start + Duration::from_millis(frame * 16);
            guard.completed(visible);
            guard.requested(visible, true, now);
            assert_eq!(
                guard.control_flow(ControlFlow::Poll, now),
                ControlFlow::Poll
            );
            assert_eq!(guard.pending[&hidden].first_request, start);
        }
        let now = start + Duration::from_millis(400);
        // The latest visible request is 96ms old; only the other is stalled.
        assert_eq!(
            guard.control_flow(ControlFlow::Poll, now),
            ControlFlow::Poll
        );
        assert_eq!(
            guard.control_flow(ControlFlow::Poll, now + Duration::from_millis(4)),
            ControlFlow::Wait
        );
        guard.completed(visible);
        assert_eq!(
            guard.control_flow(ControlFlow::Poll, now),
            ControlFlow::Wait
        );
        guard.requested(visible, true, now);
        assert_eq!(
            guard.control_flow(ControlFlow::Poll, now),
            ControlFlow::Poll
        );
    }

    #[test]
    fn explicit_waits_timers_and_other_backends_keep_their_control_flow() {
        let start = Instant::now();
        let mut guard = RedrawPollGuard::default();
        guard.requested(WindowId::from(1), true, start);
        for now in [start, start + Duration::from_secs(5)] {
            for flow in [
                ControlFlow::Wait,
                ControlFlow::WaitUntil(start + Duration::from_millis(50)),
                ControlFlow::WaitUntil(start + Duration::from_secs(10)),
            ] {
                assert_eq!(guard.control_flow(flow, now), flow);
            }
        }
        guard.requested(WindowId::from(2), false, start);
        assert_eq!(
            guard.control_flow(ControlFlow::Poll, start + Duration::from_secs(60)),
            ControlFlow::Poll
        );
    }
}


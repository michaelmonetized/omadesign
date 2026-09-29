use std::time::{Duration, Instant};

use raw_window_handle::{HasDisplayHandle as _, RawDisplayHandle};
use winit::{
    application::ApplicationHandler,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::WindowId,
};

use ahash::HashMap;

use super::winit_integration::{UserEvent, WinitApp};
use crate::{
    Result, epi,
    native::{
        event_loop_context,
        winit_integration::{EventResult, is_invisible_or_minimized},
    },
};

/// Minimum interval between repaints for invisible windows.
///
/// On Windows, invisible windows don't receive `RedrawRequested` events,
/// so we throttle their repaints to avoid busy-looping while still
/// processing viewport commands like `Visible(true)`.
/// See <https://github.com/emilk/egui/issues/7776>.
const INVISIBLE_WINDOW_REPAINT_INTERVAL: Duration = Duration::from_millis(100);

// Preserve ordinary polling while redraws make progress. A Wayland compositor
// can withhold a hidden surface's frame callback indefinitely, so an outstanding
// request must eventually yield to input, worker events and the callback itself.
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

// ----------------------------------------------------------------------------
fn create_event_loop(native_options: &mut epi::NativeOptions) -> Result<EventLoop<UserEvent>> {
    #[cfg(target_os = "android")]
    use winit::platform::android::EventLoopBuilderExtAndroid as _;

    profiling::function_scope!();
    let mut builder = winit::event_loop::EventLoop::with_user_event();

    #[cfg(target_os = "android")]
    let mut builder =
        builder.with_android_app(native_options.android_app.take().ok_or_else(|| {
            crate::Error::AppCreation(Box::from(
                "`NativeOptions` is missing required `android_app`",
            ))
        })?);

    if let Some(hook) = std::mem::take(&mut native_options.event_loop_builder) {
        hook(&mut builder);
    }

    profiling::scope!("EventLoopBuilder::build");
    Ok(builder.build()?)
}

/// Access a thread-local event loop.
///
/// We reuse the event-loop so we can support closing and opening an eframe window
/// multiple times. This is just a limitation of winit.
#[cfg(not(target_os = "ios"))]
fn with_event_loop<R>(
    mut native_options: epi::NativeOptions,
    f: impl FnOnce(&mut EventLoop<UserEvent>, epi::NativeOptions) -> R,
) -> Result<R> {
    thread_local!(static EVENT_LOOP: std::cell::RefCell<Option<EventLoop<UserEvent>>> = const { std::cell::RefCell::new(None) });

    EVENT_LOOP.with(|event_loop| {
        // Since we want to reference NativeOptions when creating the EventLoop we can't
        // do that as part of the lazy thread local storage initialization and so we instead
        // create the event loop lazily here
        let mut event_loop_lock = event_loop.borrow_mut();
        let event_loop = if let Some(event_loop) = &mut *event_loop_lock {
            event_loop
        } else {
            event_loop_lock.insert(create_event_loop(&mut native_options)?)
        };
        Ok(f(event_loop, native_options))
    })
}

/// Wraps a [`WinitApp`] to implement [`ApplicationHandler`]. This handles redrawing, exit states, and
/// some events, but otherwise forwards events to the [`WinitApp`].
struct WinitAppWrapper<T: WinitApp> {
    windows_next_repaint_times: HashMap<WindowId, Instant>,
    redraw_poll_guard: RedrawPollGuard,
    winit_app: T,
    return_result: Result<(), crate::Error>,
    run_and_return: bool,
}

impl<T: WinitApp> WinitAppWrapper<T> {
    fn new(winit_app: T, run_and_return: bool) -> Self {
        Self {
            windows_next_repaint_times: HashMap::default(),
            redraw_poll_guard: RedrawPollGuard::default(),
            winit_app,
            return_result: Ok(()),
            run_and_return,
        }
    }

    fn handle_event_result(
        &mut self,
        event_loop: &ActiveEventLoop,
        event_result: Result<EventResult>,
    ) {
        let mut exit = false;
        let mut save = false;

        log::trace!("event_result: {event_result:?}");

        let mut event_result = event_result;

        if cfg!(target_os = "windows")
            && let Ok(EventResult::RepaintNow(window_id)) = event_result
        {
            log::trace!("RepaintNow of {window_id:?}");
            self.windows_next_repaint_times
                .insert(window_id, Instant::now());

            // Fix flickering on Windows, see https://github.com/emilk/egui/pull/2280
            event_result = self.winit_app.run_ui_and_paint(event_loop, window_id);
        }

        let combined_result = event_result.map(|event_result| match event_result {
            EventResult::Wait => {
                event_loop.set_control_flow(ControlFlow::Wait);
                event_result
            }
            EventResult::RepaintNow(window_id) => {
                log::trace!("RepaintNow of {window_id:?}");
                self.windows_next_repaint_times
                    .insert(window_id, Instant::now());
                event_result
            }
            EventResult::RepaintNext(window_id) => {
                log::trace!("RepaintNext of {window_id:?}");
                self.windows_next_repaint_times
                    .insert(window_id, Instant::now());
                event_result
            }
            EventResult::RepaintAt(window_id, repaint_time) => {
                self.windows_next_repaint_times.insert(
                    window_id,
                    self.windows_next_repaint_times
                        .get(&window_id)
                        .map_or(repaint_time, |last| (*last).min(repaint_time)),
                );
                event_result
            }
            EventResult::Save => {
                save = true;
                event_result
            }
            EventResult::Exit => {
                exit = true;
                event_result
            }
            EventResult::CloseRequested => {
                // The windows need to be dropped whilst the event loop is running to allow for proper cleanup.
                self.winit_app.save_and_destroy();
                self.redraw_poll_guard.pending.clear();
                event_result
            }
        });

        if let Err(err) = combined_result {
            log::error!("Exiting because of error: {err}");
            exit = true;
            self.return_result = Err(err);
        }

        if save {
            log::debug!("Received an EventResult::Save - saving app state");
            self.winit_app.save();
        }

        if exit {
            if self.run_and_return {
                log::debug!("Asking to exit event loop…");
                event_loop.exit();
            } else {
                log::debug!("Quitting - saving app state…");
                self.winit_app.save_and_destroy();

                log::debug!("Exiting with return code 0");

                std::process::exit(0);
            }
        }

        self.check_redraw_requests(event_loop);
    }

    fn check_redraw_requests(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();

        let mut invisible_window_ids = Vec::new();

        self.windows_next_repaint_times
            .retain(|window_id, repaint_time| {
                if now < *repaint_time {
                    return true; // not yet ready
                }

                if let Some(window) = self.winit_app.window(*window_id) {
                    // On Windows, invisible windows don't receive RedrawRequested
                    // events, so pending viewport commands (e.g. Visible(true)) would
                    // never be processed. We collect these windows to paint them
                    // directly below.
                    // See: https://github.com/emilk/egui/issues/5229
                    if is_invisible_or_minimized(&window) {
                        self.redraw_poll_guard.completed(*window_id);
                        invisible_window_ids.push(*window_id);
                    } else {
                        log::trace!("request_redraw for {window_id:?}");
                        let is_wayland = window.display_handle().is_ok_and(|handle| {
                            matches!(handle.as_raw(), RawDisplayHandle::Wayland(_))
                        });
                        self.redraw_poll_guard
                            .requested(*window_id, is_wayland, Instant::now());
                        event_loop.set_control_flow(ControlFlow::Poll);
                        window.request_redraw();
                    }
                } else {
                    self.redraw_poll_guard.completed(*window_id);
                    log::trace!("No window found for {window_id:?}");
                }
                false
            });

        // Paint invisible windows directly, since they won't receive
        // RedrawRequested events on Windows. This ensures that viewport
        // commands like Visible(true) are still processed.
        for window_id in &invisible_window_ids {
            let event_result = self.winit_app.run_ui_and_paint(event_loop, *window_id);
            self.handle_event_result(event_loop, event_result);
        }

        // Throttle any already-scheduled repaints for invisible windows
        // to avoid busy-looping. If no repaint was requested by the app,
        // the window will simply sleep.
        // See: https://github.com/emilk/egui/issues/7776
        if !invisible_window_ids.is_empty() {
            let next_paint = Instant::now() + INVISIBLE_WINDOW_REPAINT_INTERVAL;
            for window_id in &invisible_window_ids {
                self.windows_next_repaint_times
                    .entry(*window_id)
                    .and_modify(|t| *t = (*t).min(next_paint));
            }
        }

        let next_repaint_time = self.windows_next_repaint_times.values().min().copied();
        if let Some(next_repaint_time) = next_repaint_time {
            event_loop.set_control_flow(ControlFlow::WaitUntil(next_repaint_time));
        }
        // Do not replace explicit Wait or scheduled WaitUntil decisions. A
        // single progressing window keeps Poll even if another is occluded.
        event_loop.set_control_flow(
            self.redraw_poll_guard
                .control_flow(event_loop.control_flow(), Instant::now()),
        );
    }
}

impl<T: WinitApp> ApplicationHandler<UserEvent> for WinitAppWrapper<T> {
    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        profiling::scope!("Event::Suspended");

        event_loop_context::with_event_loop_context(event_loop, move || {
            let event_result = self.winit_app.suspended(event_loop);
            self.handle_event_result(event_loop, event_result);
        });
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        profiling::scope!("Event::Resumed");

        // Nb: Make sure this guard is dropped after this function returns.
        event_loop_context::with_event_loop_context(event_loop, move || {
            let event_result = self.winit_app.resumed(event_loop);
            self.handle_event_result(event_loop, event_result);
        });
    }

    fn exiting(&mut self, event_loop: &ActiveEventLoop) {
        // On Mac, Cmd-Q we get here and then `run_app_on_demand` doesn't return (despite its name),
        // so we need to save state now:
        log::debug!("Received Event::LoopExiting - saving app state…");
        event_loop_context::with_event_loop_context(event_loop, move || {
            self.winit_app.save_and_destroy();
        });
    }

    fn device_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        device_id: winit::event::DeviceId,
        event: winit::event::DeviceEvent,
    ) {
        profiling::function_scope!(egui_winit::short_device_event_description(&event));

        // Nb: Make sure this guard is dropped after this function returns.
        event_loop_context::with_event_loop_context(event_loop, move || {
            let event_result = self.winit_app.device_event(event_loop, device_id, event);
            self.handle_event_result(event_loop, event_result);
        });
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        profiling::function_scope!(match &event {
            UserEvent::RequestRepaint { .. } => "UserEvent::RequestRepaint",
            #[cfg(feature = "accesskit")]
            UserEvent::AccessKitActionRequest(_) => "UserEvent::AccessKitActionRequest",
        });

        event_loop_context::with_event_loop_context(event_loop, move || {
            let event_result = match event {
                UserEvent::RequestRepaint {
                    when,
                    cumulative_pass_nr,
                    viewport_id,
                } => {
                    let current_pass_nr = self
                        .winit_app
                        .egui_ctx()
                        .map_or(0, |ctx| ctx.cumulative_pass_nr_for(viewport_id));
                    if current_pass_nr == cumulative_pass_nr
                        || current_pass_nr == cumulative_pass_nr + 1
                    {
                        log::trace!("UserEvent::RequestRepaint scheduling repaint at {when:?}");
                        if let Some(window_id) =
                            self.winit_app.window_id_from_viewport_id(viewport_id)
                        {
                            // Throttle repaints for invisible windows to prevent
                            // high CPU usage on Windows.
                            // See: https://github.com/emilk/egui/issues/7776
                            let when = if let Some(window) = self.winit_app.window(window_id)
                                && is_invisible_or_minimized(&window)
                            {
                                when.max(Instant::now() + INVISIBLE_WINDOW_REPAINT_INTERVAL)
                            } else {
                                when
                            };
                            Ok(EventResult::RepaintAt(window_id, when))
                        } else {
                            Ok(EventResult::Wait)
                        }
                    } else {
                        log::trace!("Got outdated UserEvent::RequestRepaint");
                        Ok(EventResult::Wait) // old request - we've already repainted
                    }
                }
                #[cfg(feature = "accesskit")]
                UserEvent::AccessKitActionRequest(request) => {
                    self.winit_app.on_accesskit_event(request)
                }
            };
            self.handle_event_result(event_loop, event_result);
        });
    }

    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: winit::event::StartCause) {
        if let winit::event::StartCause::ResumeTimeReached { .. } = cause {
            log::trace!("Woke up to check next_repaint_time");
        }

        self.check_redraw_requests(event_loop);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: winit::event::WindowEvent,
    ) {
        profiling::function_scope!(egui_winit::short_window_event_description(&event));

        if matches!(
            event,
            winit::event::WindowEvent::RedrawRequested | winit::event::WindowEvent::Destroyed
        ) {
            // Clear before running UI: rendering time is not time spent waiting
            // for the next compositor callback, even for very expensive frames.
            self.redraw_poll_guard.completed(window_id);
        }

        // Nb: Make sure this guard is dropped after this function returns.
        event_loop_context::with_event_loop_context(event_loop, move || {
            let event_result = match event {
                winit::event::WindowEvent::RedrawRequested => {
                    self.winit_app.run_ui_and_paint(event_loop, window_id)
                }
                _ => self.winit_app.window_event(event_loop, window_id, event),
            };

            self.handle_event_result(event_loop, event_result);
        });
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

#[cfg(not(target_os = "ios"))]
fn run_and_return(event_loop: &mut EventLoop<UserEvent>, winit_app: impl WinitApp) -> Result {
    use winit::platform::run_on_demand::EventLoopExtRunOnDemand as _;

    log::trace!("Entering the winit event loop (run_app_on_demand)…");

    let mut app = WinitAppWrapper::new(winit_app, true);
    event_loop.run_app_on_demand(&mut app)?;
    log::debug!("eframe window closed");
    app.return_result
}

fn run_and_exit(event_loop: EventLoop<UserEvent>, winit_app: impl WinitApp) -> Result {
    log::trace!("Entering the winit event loop (run_app)…");

    // When to repaint what window
    let mut app = WinitAppWrapper::new(winit_app, false);
    event_loop.run_app(&mut app)?;

    log::debug!("winit event loop unexpectedly returned");
    Ok(())
}

// ----------------------------------------------------------------------------

#[cfg(feature = "glow")]
pub fn run_glow(
    app_name: &str,
    mut native_options: epi::NativeOptions,
    egui_ctx: Option<egui::Context>,
    app_creator: epi::AppCreator<'_>,
) -> Result {
    use super::glow_integration::GlowWinitApp;

    #[cfg(not(target_os = "ios"))]
    if native_options.run_and_return {
        return with_event_loop(native_options, |event_loop, native_options| {
            let glow_eframe =
                GlowWinitApp::new(event_loop, app_name, native_options, egui_ctx, app_creator);
            run_and_return(event_loop, glow_eframe)
        })?;
    }

    let event_loop = create_event_loop(&mut native_options)?;
    let glow_eframe =
        GlowWinitApp::new(&event_loop, app_name, native_options, egui_ctx, app_creator);
    run_and_exit(event_loop, glow_eframe)
}

#[cfg(feature = "glow")]
pub fn create_glow<'a>(
    app_name: &str,
    native_options: epi::NativeOptions,
    app_creator: epi::AppCreator<'a>,
    event_loop: &EventLoop<UserEvent>,
) -> impl ApplicationHandler<UserEvent> + 'a {
    use super::glow_integration::GlowWinitApp;

    let glow_eframe = GlowWinitApp::new(event_loop, app_name, native_options, None, app_creator);
    WinitAppWrapper::new(glow_eframe, true)
}

// ----------------------------------------------------------------------------

#[cfg(feature = "wgpu_no_default_features")]
pub fn run_wgpu(
    app_name: &str,
    mut native_options: epi::NativeOptions,
    egui_ctx: Option<egui::Context>,
    app_creator: epi::AppCreator<'_>,
) -> Result {
    use super::wgpu_integration::WgpuWinitApp;

    #[cfg(not(target_os = "ios"))]
    if native_options.run_and_return {
        return with_event_loop(native_options, |event_loop, native_options| {
            let wgpu_eframe =
                WgpuWinitApp::new(event_loop, app_name, native_options, egui_ctx, app_creator);
            run_and_return(event_loop, wgpu_eframe)
        })?;
    }

    let event_loop = create_event_loop(&mut native_options)?;
    let wgpu_eframe =
        WgpuWinitApp::new(&event_loop, app_name, native_options, egui_ctx, app_creator);
    run_and_exit(event_loop, wgpu_eframe)
}

#[cfg(feature = "wgpu_no_default_features")]
pub fn create_wgpu<'a>(
    app_name: &str,
    native_options: epi::NativeOptions,
    app_creator: epi::AppCreator<'a>,
    event_loop: &EventLoop<UserEvent>,
) -> impl ApplicationHandler<UserEvent> + 'a {
    use super::wgpu_integration::WgpuWinitApp;

    let wgpu_eframe = WgpuWinitApp::new(event_loop, app_name, native_options, None, app_creator);
    WinitAppWrapper::new(wgpu_eframe, true)
}

// ----------------------------------------------------------------------------

/// A proxy to the eframe application that implements [`ApplicationHandler`].
///
/// This can be run directly on your own [`EventLoop`] by itself or with other
/// windows you manage outside of eframe.
pub struct EframeWinitApplication<'a> {
    wrapper: Box<dyn ApplicationHandler<UserEvent> + 'a>,
    control_flow: ControlFlow,
}

impl ApplicationHandler<UserEvent> for EframeWinitApplication<'_> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.wrapper.resumed(event_loop);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        self.wrapper.window_event(event_loop, window_id, event);
    }

    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: winit::event::StartCause) {
        self.wrapper.new_events(event_loop, cause);
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        self.wrapper.user_event(event_loop, event);
    }

    fn device_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        device_id: winit::event::DeviceId,
        event: winit::event::DeviceEvent,
    ) {
        self.wrapper.device_event(event_loop, device_id, event);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.wrapper.about_to_wait(event_loop);
        self.control_flow = event_loop.control_flow();
    }

    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        self.wrapper.suspended(event_loop);
    }

    fn exiting(&mut self, event_loop: &ActiveEventLoop) {
        self.wrapper.exiting(event_loop);
    }

    fn memory_warning(&mut self, event_loop: &ActiveEventLoop) {
        self.wrapper.memory_warning(event_loop);
    }
}

impl<'a> EframeWinitApplication<'a> {
    pub(crate) fn new<T: ApplicationHandler<UserEvent> + 'a>(app: T) -> Self {
        Self {
            wrapper: Box::new(app),
            control_flow: ControlFlow::default(),
        }
    }

    /// Pump the `EventLoop` to check for and dispatch pending events to this application.
    ///
    /// Returns either the exit code for the application or the final state of the [`ControlFlow`]
    /// after all events have been dispatched in this iteration.
    ///
    /// This is useful when your [`EventLoop`] is not the main event loop for your application.
    /// See the `external_eventloop_async` example.
    #[cfg(not(target_os = "ios"))]
    pub fn pump_eframe_app(
        &mut self,
        event_loop: &mut EventLoop<UserEvent>,
        timeout: Option<std::time::Duration>,
    ) -> EframePumpStatus {
        use winit::platform::pump_events::{EventLoopExtPumpEvents as _, PumpStatus};

        match event_loop.pump_app_events(timeout, self) {
            PumpStatus::Continue => EframePumpStatus::Continue(self.control_flow),
            PumpStatus::Exit(code) => EframePumpStatus::Exit(code),
        }
    }
}

/// Either an exit code or a [`ControlFlow`] from the [`ActiveEventLoop`].
///
/// The result of [`EframeWinitApplication::pump_eframe_app`].
#[cfg(not(target_os = "ios"))]
pub enum EframePumpStatus {
    /// The final state of the [`ControlFlow`] after all events have been dispatched
    ///
    /// Callers should perform the action that is appropriate for the [`ControlFlow`] value.
    Continue(ControlFlow),

    /// The exit code for the application
    Exit(i32),
}

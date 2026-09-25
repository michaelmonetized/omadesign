use eframe::egui;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, mpsc};

/// Frame counter shared with a background export. Other jobs leave it at zero.
pub(super) struct Progress {
    done: AtomicU32,
    total: AtomicU32,
    cancel: AtomicBool,
}

impl Progress {
    fn new(total: u32) -> Arc<Self> {
        Arc::new(Self {
            done: AtomicU32::new(0),
            total: AtomicU32::new(total),
            cancel: AtomicBool::new(false),
        })
    }

    pub(super) fn report(&self, done: u32) {
        self.done.store(done, Ordering::Relaxed);
    }

    pub(super) fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

// A window owns its request. Removing it drops the receiver, so a late network
// response cannot repopulate a cleared search or insert a cancelled download.
struct Job<T> {
    rx: Arc<Mutex<mpsc::Receiver<Result<T, String>>>>,
    progress: Arc<Progress>,
}

impl<T> Clone for Job<T> {
    fn clone(&self) -> Self {
        Self {
            rx: Arc::clone(&self.rx),
            progress: Arc::clone(&self.progress),
        }
    }
}

pub(super) fn start<T: Send + 'static>(
    ctx: &egui::Context,
    id: &'static str,
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) {
    launch(ctx, id, Progress::new(0), move |_| work());
}

pub(super) fn start_with_progress<T: Send + 'static>(
    ctx: &egui::Context,
    id: &'static str,
    total: u32,
    work: impl FnOnce(Arc<Progress>) -> Result<T, String> + Send + 'static,
) {
    launch(ctx, id, Progress::new(total), work);
}

fn launch<T: Send + 'static>(
    ctx: &egui::Context,
    id: &'static str,
    progress: Arc<Progress>,
    work: impl FnOnce(Arc<Progress>) -> Result<T, String> + Send + 'static,
) {
    ctx.data_mut(|d| {
        let mut ids = d
            .get_temp::<std::collections::HashSet<String>>(egui::Id::new("background-jobs"))
            .unwrap_or_default();
        ids.insert(id.into());
        d.insert_temp(egui::Id::new("background-jobs"), ids);
    });
    let (tx, rx) = mpsc::channel();
    ctx.data_mut(|d| {
        d.insert_temp(
            egui::Id::new(id),
            Job {
                rx: Arc::new(Mutex::new(rx)),
                progress: Arc::clone(&progress),
            },
        );
    });
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let _ = tx.send(work(progress));
        ctx.request_repaint();
    });
}

pub(super) fn poll<T: Send + 'static>(
    ctx: &egui::Context,
    id: &'static str,
) -> Option<Result<T, String>> {
    let job = ctx.data(|d| d.get_temp::<Job<T>>(egui::Id::new(id)))?;
    let result = match job.rx.lock().ok()?.try_recv() {
        Ok(result) => result,
        Err(mpsc::TryRecvError::Empty) => return None,
        Err(mpsc::TryRecvError::Disconnected) => {
            Err("The request stopped. Please try again.".into())
        }
    };
    cancel::<T>(ctx, id);
    Some(result)
}

pub(super) fn frame_progress<T: Send + 'static>(
    ctx: &egui::Context,
    id: &'static str,
) -> Option<(u32, u32)> {
    ctx.data(|d| {
        d.get_temp::<Job<T>>(egui::Id::new(id)).map(|job| {
            (
                job.progress.done.load(Ordering::Relaxed),
                job.progress.total.load(Ordering::Relaxed),
            )
        })
    })
}

pub(super) fn cancel<T: Send + 'static>(ctx: &egui::Context, id: &'static str) {
    ctx.data_mut(|d| {
        if let Some(job) = d.get_temp::<Job<T>>(egui::Id::new(id)) {
            job.progress.cancel.store(true, Ordering::Relaxed);
        }
        d.remove::<Job<T>>(egui::Id::new(id));
        let mut ids = d
            .get_temp::<std::collections::HashSet<String>>(egui::Id::new("background-jobs"))
            .unwrap_or_default();
        ids.remove(id);
        d.insert_temp(egui::Id::new("background-jobs"), ids);
    });
}

pub(super) fn is_running<T: Send + 'static>(ctx: &egui::Context, id: &'static str) -> bool {
    ctx.data(|d| d.get_temp::<Job<T>>(egui::Id::new(id)).is_some())
}

pub(super) fn any_running(ctx: &egui::Context) -> bool {
    ctx.data(|d| {
        d.get_temp::<std::collections::HashSet<String>>(egui::Id::new("background-jobs"))
            .is_some_and(|ids| !ids.is_empty())
    })
}

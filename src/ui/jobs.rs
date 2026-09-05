use eframe::egui;
use std::sync::{Arc, Mutex, mpsc};

// A window owns its request. Removing it drops the receiver, so a late network
// response cannot repopulate a cleared search or insert a cancelled download.
struct Job<T>(Arc<Mutex<mpsc::Receiver<Result<T, String>>>>);

impl<T> Clone for Job<T> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

pub(super) fn start<T: Send + 'static>(
    ctx: &egui::Context,
    id: &'static str,
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) {
    let (tx, rx) = mpsc::channel();
    ctx.data_mut(|d| d.insert_temp(egui::Id::new(id), Job(Arc::new(Mutex::new(rx)))));
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let _ = tx.send(work());
        ctx.request_repaint();
    });
}

pub(super) fn poll<T: Send + 'static>(
    ctx: &egui::Context,
    id: &'static str,
) -> Option<Result<T, String>> {
    let job = ctx.data(|d| d.get_temp::<Job<T>>(egui::Id::new(id)))?;
    let result = match job.0.lock().ok()?.try_recv() {
        Ok(result) => result,
        Err(mpsc::TryRecvError::Empty) => return None,
        Err(mpsc::TryRecvError::Disconnected) => {
            Err("The request stopped. Please try again.".into())
        }
    };
    cancel::<T>(ctx, id);
    Some(result)
}

pub(super) fn cancel<T: Send + 'static>(ctx: &egui::Context, id: &'static str) {
    ctx.data_mut(|d| d.remove::<Job<T>>(egui::Id::new(id)));
}

pub(super) fn is_running<T: Send + 'static>(ctx: &egui::Context, id: &'static str) -> bool {
    ctx.data(|d| d.get_temp::<Job<T>>(egui::Id::new(id)).is_some())
}

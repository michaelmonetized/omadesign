use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU32, Ordering},
};

/// Share a cancellation request with active photo loading.
/// Clone the token for a worker; cancellation remains set for that job's lifetime.
#[derive(Clone, Debug, Default)]
pub struct Cancellation {
    flag: Arc<AtomicBool>,
    progress: Arc<AtomicU32>,
}

impl Cancellation {
    pub(crate) fn from_flag(flag: Arc<AtomicBool>) -> Self {
        Self {
            flag,
            progress: Arc::default(),
        }
    }
    /// Stop this job at its next decoder or buffer checkpoint.
    /// Args: none. Returns after publishing the request to every token clone.
    pub fn cancel(&self) {
        self.flag.store(true, Ordering::Release);
    }

    /// Inspect the shared cancellation state.
    /// Args: none. Returns true once cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::Acquire)
    }

    /// Inspect the last native stage actually entered by this job.
    /// Args: none. Returns LibRaw's progress bit, or zero before a stage is reported.
    pub fn native_progress(&self) -> u32 {
        self.progress.load(Ordering::Acquire)
    }

    /// Reject further work after cancellation.
    /// Args: none. Returns a stable cancellation error or success.
    pub fn check(&self) -> Result<(), String> {
        if self.is_cancelled() {
            Err("Photo opening cancelled.".into())
        } else {
            Ok(())
        }
    }
}

pub(super) unsafe extern "C" fn progress(context: *const std::ffi::c_void, stage: u32) {
    // SAFETY: The same synchronous token lifetime applies as for cancelled().
    let token = unsafe { &*context.cast::<Cancellation>() };
    token.progress.store(stage, Ordering::Release);
}

pub(super) unsafe extern "C" fn cancelled(context: *const std::ffi::c_void) -> i32 {
    // SAFETY: decode_cancellable keeps the referenced token alive until the
    // synchronous native call and its joined monitor have both returned.
    let token = unsafe { &*context.cast::<Cancellation>() };
    i32::from(token.is_cancelled())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn raw_cancellation_reaches_shared_owner_and_does_not_cancel_next_job() {
        let first = Cancellation::default();
        let worker = first.clone();
        assert!(worker.check().is_ok());
        first.cancel();
        assert!(worker.check().unwrap_err().contains("cancelled"));
        let next = Cancellation::default();
        assert!(next.check().is_ok());
        assert!(
            super::super::decode_cancellable(include_bytes!("synthetic.dng"), &worker)
                .unwrap_err()
                .contains("cancelled")
        );
    }
}

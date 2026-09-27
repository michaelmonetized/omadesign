//! Shared offline inference and verified model storage. No network on model load.
pub mod models;
pub mod runtime;

pub trait Progress: Sync {
    fn report(&self, stage: &str, done: usize, total: usize);
    fn cancelled(&self) -> bool;
    fn check(&self) -> Result<(), String> {
        if self.cancelled() {
            Err("Cancelled".into())
        } else {
            Ok(())
        }
    }
}

pub struct NoProgress;
impl Progress for NoProgress {
    fn report(&self, _: &str, _: usize, _: usize) {}
    fn cancelled(&self) -> bool {
        false
    }
}

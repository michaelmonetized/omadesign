//! Shared admission for encoded inputs, native decoding and retained RAW samples.
use super::Cancellation;
use std::sync::{
    Arc, Condvar, Mutex, OnceLock, Weak,
    atomic::{AtomicU64, Ordering},
};
use std::time::{Duration, Instant};

const MIB: u64 = 1024 * 1024;
const HEADROOM: u64 = 256 * MIB;

#[derive(Debug, Default)]
struct State {
    used: u64,
    sources: Vec<(Weak<Vec<u16>>, Arc<Lease>)>,
}

#[derive(Debug)]
pub struct Budget {
    limit: u64,
    state: Mutex<State>,
    wake: Condvar,
    host_pressure: bool,
}

#[derive(Debug)]
pub struct Lease {
    budget: Arc<Budget>,
    bytes: AtomicU64,
}

impl Budget {
    /// Create a bounded resource pool.
    /// Args: aggregate byte limit and whether to inspect live host memory. Returns an empty shared pool.
    pub fn new(limit: u64, host_pressure: bool) -> Arc<Self> {
        Arc::new(Self {
            limit,
            state: Mutex::new(State::default()),
            wake: Condvar::new(),
            host_pressure,
        })
    }

    /// Reap only sources whose final sample-buffer clone has been dropped.
    /// Args: none. Returns with expired leases released outside the pool lock.
    fn reap(&self) {
        let expired = {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            let mut expired = vec![];
            let mut index = 0;
            while index < state.sources.len() {
                if state.sources[index].0.strong_count() == 0 {
                    expired.push(state.sources.swap_remove(index).1);
                } else {
                    index += 1;
                }
            }
            expired
        };
        drop(expired);
    }

    /// Reserve one operation's buffers without exceeding aggregate admission.
    /// Args: required bytes and cancellation token. Returns an owned lease, waiting at most thirty seconds for other work.
    pub fn reserve(
        self: &Arc<Self>,
        bytes: u64,
        cancel: &Cancellation,
    ) -> Result<Arc<Lease>, String> {
        let lease = Arc::new(Lease {
            budget: self.clone(),
            bytes: AtomicU64::new(0),
        });
        lease.resize(bytes, cancel)?;
        Ok(lease)
    }

    /// Bind retained memory to the actual sample allocation, including external Arc clones.
    /// Args: shared samples and their lease. Returns after recording weak lifetime ownership.
    pub fn retain(&self, pixels: &Arc<Vec<u16>>, lease: Arc<Lease>) {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .sources
            .push((Arc::downgrade(pixels), lease));
    }

    /// Find admission already owned by a restored sample allocation.
    /// Args: shared samples. Returns their existing lease without counting them twice.
    pub(crate) fn retained(&self, pixels: &Arc<Vec<u16>>) -> Option<Arc<Lease>> {
        let weak = Arc::downgrade(pixels);
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .sources
            .iter()
            .find(|(source, _)| source.ptr_eq(&weak))
            .map(|(_, lease)| lease.clone())
    }

    /// Inspect current admitted storage after releasing expired source leases.
    /// Args: none. Returns used bytes and total byte limit.
    pub fn usage(&self) -> (u64, u64) {
        self.reap();
        (
            self.state.lock().unwrap_or_else(|e| e.into_inner()).used,
            self.limit,
        )
    }
}

impl Lease {
    /// Resize an admitted operation as its actual sensor/output geometry becomes known.
    /// Args: total required bytes and cancellation token. Returns an error before allocation if pressure or bounds cannot admit it.
    pub fn resize(&self, bytes: u64, cancel: &Cancellation) -> Result<(), String> {
        if bytes > self.budget.limit {
            return Err(format!(
                "RAW needs an estimated {} MiB; the shared photo budget is {} MiB. Close other photos or increase available memory.",
                bytes.div_ceil(MIB),
                self.budget.limit / MIB
            ));
        }
        let started = Instant::now();
        loop {
            cancel.check()?;
            self.budget.reap();
            let mut state = self.budget.state.lock().unwrap_or_else(|e| e.into_inner());
            let old = self.bytes.load(Ordering::Acquire);
            let extra = bytes.saturating_sub(old);
            let host_ok = !self.budget.host_pressure
                || extra == 0
                || available_memory()
                    .is_none_or(|available| extra.saturating_add(HEADROOM) <= available);
            if state.used.saturating_sub(old).saturating_add(bytes) <= self.budget.limit && host_ok
            {
                state.used = state.used - old + bytes;
                self.bytes.store(bytes, Ordering::Release);
                self.budget.wake.notify_all();
                return Ok(());
            }
            if started.elapsed() > Duration::from_secs(30) {
                return Err(format!(
                    "Not enough free memory for this RAW operation ({} MiB requested). Current photos and edits were kept.",
                    bytes.div_ceil(MIB)
                ));
            }
            let _ = self
                .budget
                .wake
                .wait_timeout(state, Duration::from_millis(5))
                .unwrap_or_else(|e| e.into_inner());
        }
    }

    pub fn bytes(&self) -> u64 {
        self.bytes.load(Ordering::Acquire)
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        let mut state = self.budget.state.lock().unwrap_or_else(|e| e.into_inner());
        state.used = state
            .used
            .saturating_sub(self.bytes.load(Ordering::Acquire));
        self.budget.wake.notify_all();
    }
}

/// Read Linux memory counters without assuming reclaimable cache is unavailable.
/// Args: counter name. Returns bytes when procfs supplies that counter.
fn memory_counter(name: &str) -> Option<u64> {
    std::fs::read_to_string("/proc/meminfo")
        .ok()?
        .lines()
        .find_map(|line| {
            let mut words = line.split_whitespace();
            (words.next()? == name)
                .then(|| words.next()?.parse::<u64>().ok()?.checked_mul(1024))
                .flatten()
        })
}
pub fn available_memory() -> Option<u64> {
    memory_counter("MemAvailable:")
}

/// Use one process-wide pool for native, source and development allocations.
/// Args: none. Returns a pool capped at half physical memory and eight GiB, with live pressure checks.
pub fn global() -> &'static Arc<Budget> {
    static POOL: OnceLock<Arc<Budget>> = OnceLock::new();
    POOL.get_or_init(|| {
        Budget::new(
            (memory_counter("MemTotal:").unwrap_or(2 * 1024 * MIB) / 2)
                .clamp(256 * MIB, 8192 * MIB),
            true,
        )
    })
}

/// Compute conservative native peak demand from stored sensor and expanded output sizes.
/// Args: sensor pixels and output pixels. Returns bytes including temporary decode buffers, or overflow rejection.
pub fn native_demand(sensor: u64, output: u64) -> Option<u64> {
    sensor
        .checked_mul(8)?
        .checked_add(output.checked_mul(32)?)?
        .checked_add(64 * MIB)
}

/// Retain linear samples plus the bounded Photo preview levels.
/// Args: output pixel count. Returns source/preview bytes without a megapixel cutoff.
pub fn retained_demand(pixels: u64) -> Option<u64> {
    pixels.checked_mul(6)?.checked_add(40 * MIB)
}

/// Serialize expensive native decodes while allowing cancellation of waiting jobs.
/// Args: job cancellation token. Returns an exclusive guard held through native decode and copying.
pub fn decoder(cancel: &Cancellation) -> Result<std::sync::MutexGuard<'static, ()>, String> {
    static DECODER: Mutex<()> = Mutex::new(());
    loop {
        cancel.check()?;
        match DECODER.try_lock() {
            Ok(guard) => return Ok(guard),
            Err(std::sync::TryLockError::Poisoned(error)) => return Ok(error.into_inner()),
            Err(std::sync::TryLockError::WouldBlock) => {
                std::thread::sleep(Duration::from_millis(2))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn raw_resources_follow_last_sample_clone_and_enforce_aggregate_budget() {
        let pool = Budget::new(100, false);
        let cancel = Cancellation::default();
        let lease = pool.reserve(60, &cancel).unwrap();
        let pixels = Arc::new(vec![0u16; 30]);
        pool.retain(&pixels, lease.clone());
        let external = pixels.clone();
        drop(lease);
        drop(pixels);
        assert_eq!(pool.usage(), (60, 100));
        let cancelled = Cancellation::default();
        cancelled.cancel();
        assert!(pool.reserve(41, &cancelled).is_err());
        drop(external);
        assert_eq!(pool.usage(), (0, 100));
        assert!(pool.reserve(100, &cancel).is_ok());
        assert!(pool.reserve(101, &cancel).is_err());
    }
    #[test]
    fn raw_resources_admit_high_resolution_by_bytes_and_reject_overflow() {
        let demand = native_demand(105_000_000, 102_000_000).unwrap();
        assert!(demand > 4_000_000_000);
        let pool = Budget::new(8 * 1024 * MIB, false);
        let lease = pool.reserve(demand, &Cancellation::default()).unwrap();
        assert_eq!(pool.usage().0, demand);
        drop(lease);
        assert_eq!(pool.usage().0, 0);
        assert!(native_demand(u64::MAX, u64::MAX).is_none());
    }
}

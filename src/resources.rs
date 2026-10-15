//! Shared admission and lifetime ownership for photo and native document buffers.
use std::sync::{
    Arc, Condvar, Mutex, OnceLock, Weak,
    atomic::{AtomicU64, Ordering},
};
use std::time::{Duration, Instant};

const MIB: u64 = 1024 * 1024;
const HEADROOM: u64 = 256 * MIB;

pub trait CancellationCheck {
    fn check(&self) -> Result<(), String>;
}
impl<F: Fn() -> Result<(), String>> CancellationCheck for F {
    fn check(&self) -> Result<(), String> {
        self()
    }
}

#[derive(Debug, Default)]
struct State {
    used: u64,
    sources: Vec<(Weak<dyn Send + Sync>, Arc<Lease>)>,
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

#[derive(Debug)]
pub struct Allocation<T: Copy> {
    values: Vec<T>,
    lease: Arc<Lease>,
}
impl<T: Copy> std::ops::Deref for Allocation<T> {
    type Target = [T];
    fn deref(&self) -> &Self::Target {
        &self.values
    }
}
impl<T: Copy + PartialEq> PartialEq for Allocation<T> {
    fn eq(&self, other: &Self) -> bool {
        self.values == other.values
    }
}
impl<T: Copy> Allocation<T> {
    /// Borrow immutable samples. Args: none. Returns the admitted slice.
    pub fn as_slice(&self) -> &[T] {
        &self.values
    }
    /// Edit uniquely owned working samples. Args: none. Returns the existing admitted slice without allocation.
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        &mut self.values
    }
    /// Inspect allocation ownership. Args: none. Returns its retained admission lease.
    pub fn lease(&self) -> &Arc<Lease> {
        &self.lease
    }
    /// Inspect actual allocation capacity. Args: none. Returns admitted storage bytes.
    pub fn bytes(&self) -> u64 {
        self.values.capacity() as u64 * std::mem::size_of::<T>() as u64
    }
    /// Copy an allocation only after admitting its independent storage.
    /// Args: cancellation owner. Returns a fallible COW working allocation in the same pool.
    pub fn try_clone<C: CancellationCheck + ?Sized>(&self, cancel: &C) -> Result<Self, String> {
        self.lease.budget.copy(&self.values, cancel)
    }
    /// Transfer storage with its admission intact. Args: none. Returns both owned samples and their lease.
    pub fn into_parts(self) -> (Vec<T>, Arc<Lease>) {
        (self.values, self.lease)
    }
}

fn allocation_bytes<T>(capacity: usize) -> Result<u64, String> {
    if std::mem::size_of::<T>() == 0 {
        return Err("Zero-sized buffers do not need memory admission".into());
    }
    capacity
        .checked_mul(std::mem::size_of::<T>())
        .and_then(|n| u64::try_from(n).ok())
        .ok_or_else(|| "Buffer allocation size overflow".into())
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
    pub fn reserve<C: CancellationCheck + ?Sized>(
        self: &Arc<Self>,
        bytes: u64,
        cancel: &C,
    ) -> Result<Arc<Lease>, String> {
        let lease = Arc::new(Lease {
            budget: self.clone(),
            bytes: AtomicU64::new(0),
        });
        lease.resize(bytes, cancel)?;
        Ok(lease)
    }

    /// Adopt already admitted storage without reserving the same bytes again.
    /// Args: owned plain samples and an exclusive same-pool lease covering their capacity. Returns immutable allocation ownership.
    pub fn adopt<T: Copy>(
        self: &Arc<Self>,
        values: Vec<T>,
        lease: Arc<Lease>,
    ) -> Result<Arc<Allocation<T>>, String> {
        if !Arc::ptr_eq(self, &lease.budget)
            || Arc::strong_count(&lease) != 1
            || lease.bytes() < allocation_bytes::<T>(values.capacity())?
        {
            return Err("Buffer adoption needs one exclusive same-pool lease covering its allocation capacity".into());
        }
        Ok(Arc::new(Allocation { values, lease }))
    }

    /// Allocate a mutable working buffer inside aggregate admission.
    /// Args: plain sample count and cancellation owner. Returns initialized owned storage or an error before growth.
    pub fn allocate<T: Copy + Default, C: CancellationCheck + ?Sized>(
        self: &Arc<Self>,
        count: usize,
        cancel: &C,
    ) -> Result<Allocation<T>, String> {
        let lease = self.reserve(allocation_bytes::<T>(count)?, cancel)?;
        let mut values = Vec::new();
        values
            .try_reserve_exact(count)
            .map_err(|_| "Not enough memory to allocate pixel samples")?;
        lease.resize(allocation_bytes::<T>(values.capacity())?, cancel)?;
        values.resize(count, T::default());
        Ok(Allocation { values, lease })
    }

    /// Allocate an independent copy for editing without hidden Clone allocations.
    /// Args: plain source samples and cancellation owner. Returns owned mutable storage from this pool.
    pub fn copy<T: Copy, C: CancellationCheck + ?Sized>(
        self: &Arc<Self>,
        source: &[T],
        cancel: &C,
    ) -> Result<Allocation<T>, String> {
        let lease = self.reserve(allocation_bytes::<T>(source.len())?, cancel)?;
        let mut values = Vec::new();
        values
            .try_reserve_exact(source.len())
            .map_err(|_| "Not enough memory to copy pixel samples")?;
        lease.resize(allocation_bytes::<T>(values.capacity())?, cancel)?;
        values.extend_from_slice(source);
        Ok(Allocation { values, lease })
    }

    /// Bind retained memory to the actual sample allocation, including external Arc clones.
    /// Args: shared samples and their lease. Returns after recording weak lifetime ownership.
    pub fn retain<T: Send + Sync + 'static>(&self, pixels: &Arc<T>, lease: Arc<Lease>) {
        let allocation: Arc<dyn Send + Sync> = pixels.clone();
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .sources
            .push((Arc::downgrade(&allocation), lease));
    }

    /// Find admission already owned by a restored sample allocation.
    /// Args: shared samples. Returns their existing lease without counting them twice.
    pub fn retained<T: Send + Sync + 'static>(&self, pixels: &Arc<T>) -> Option<Arc<Lease>> {
        let allocation: Arc<dyn Send + Sync> = pixels.clone();
        let weak = Arc::downgrade(&allocation);
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
    pub fn resize<C: CancellationCheck + ?Sized>(
        &self,
        bytes: u64,
        cancel: &C,
    ) -> Result<(), String> {
        if bytes > self.budget.limit {
            return Err(format!(
                "This operation needs an estimated {} MiB; the shared pixel budget is {} MiB. Close other documents or increase available memory.",
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
            let required = state.used.saturating_sub(old).saturating_add(bytes);
            let host_ok = !self.budget.host_pressure
                || extra == 0
                || available_memory()
                    .is_none_or(|available| required.saturating_add(HEADROOM) <= available);
            if required <= self.budget.limit && host_ok {
                state.used = state.used - old + bytes;
                self.bytes.store(bytes, Ordering::Release);
                self.budget.wake.notify_all();
                return Ok(());
            }
            if started.elapsed() > Duration::from_secs(30) {
                return Err(format!(
                    "Not enough free memory for this operation ({} MiB requested). Current documents and edits were kept.",
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
pub fn decoder<C: CancellationCheck + ?Sized>(
    cancel: &C,
) -> Result<std::sync::MutexGuard<'static, ()>, String> {
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
    fn adopted_native_allocations_share_one_budget_with_raw_and_cow_copies() {
        let pool = Budget::new(160, false);
        let check = || Ok(());
        let raw = pool.reserve(80, &check).unwrap();
        let values = vec![0.25f32; 10];
        let memory = pool.reserve(40, &check).unwrap();
        let owner = pool.adopt(values, memory).unwrap();
        let clone = owner.clone();
        assert_eq!(pool.usage(), (120, 160));
        let mut working = owner.try_clone(&check).unwrap();
        assert_eq!(pool.usage(), (160, 160));
        working.as_mut_slice()[0] = 0.75;
        assert_eq!(owner[0], 0.25);
        assert_eq!(working[0], 0.75);
        drop(working);
        drop(owner);
        assert_eq!(pool.usage(), (120, 160));
        drop(clone);
        assert_eq!(pool.usage(), (80, 160));
        drop(raw);
        assert_eq!(pool.usage(), (0, 160));
    }

    #[test]
    fn adoption_rejects_wrong_pool_short_and_shared_leases() {
        let pool = Budget::new(100, false);
        let other = Budget::new(100, false);
        let check = || Ok(());
        assert!(
            pool.adopt(vec![0u32; 10], other.reserve(40, &check).unwrap())
                .is_err()
        );
        assert!(
            pool.adopt(vec![0u32; 10], pool.reserve(39, &check).unwrap())
                .is_err()
        );
        let lease = pool.reserve(40, &check).unwrap();
        assert!(pool.adopt(vec![0u32; 10], lease.clone()).is_err());
        assert_eq!(pool.usage(), (40, 100));
        assert!(pool.adopt(vec![0u32; 10], lease).is_ok());
        assert_eq!(pool.usage(), (0, 100));
        let cancelled = || Err("Cancelled".into());
        assert!(pool.allocate::<u8, _>(10, &cancelled).is_err());
    }
    #[test]
    fn raw_resources_follow_last_sample_clone_and_enforce_aggregate_budget() {
        let pool = Budget::new(100, false);
        let cancel = || Ok(());
        let lease = pool.reserve(60, &cancel).unwrap();
        let pixels = Arc::new(vec![0u16; 30]);
        pool.retain(&pixels, lease.clone());
        let external = pixels.clone();
        drop(lease);
        drop(pixels);
        assert_eq!(pool.usage(), (60, 100));
        let cancelled = || Err("Cancelled".into());
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
        let lease = pool.reserve(demand, &|| Ok(())).unwrap();
        assert_eq!(pool.usage().0, demand);
        drop(lease);
        assert_eq!(pool.usage().0, 0);
        assert!(native_demand(u64::MAX, u64::MAX).is_none());
    }
}

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
    materialized: u64,
    sources: Vec<(Weak<dyn Send + Sync>, Arc<Lease>)>,
}

#[derive(Debug)]
pub struct Budget {
    limit: u64,
    state: Mutex<State>,
    wake: Condvar,
    host_pressure: bool,
    #[cfg(test)]
    available_override: Option<fn() -> Option<u64>>,
}

#[derive(Debug)]
pub struct Lease {
    budget: Arc<Budget>,
    bytes: AtomicU64,
    materialized: AtomicU64,
    backing: AtomicU64,
    owner: Mutex<Weak<()>>,
}

#[derive(Debug)]
pub struct Allocation<T: Copy> {
    values: Vec<T>,
    owner: Arc<()>,
    lease: Arc<Lease>,
    transferred: bool,
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

    /// Reserve synchronous typed storage without initializing spare capacity.
    /// Args: minimum total capacity and cancellation owner. Returns immediately on pressure, covering old and new buffers during growth.
    pub fn reserve_now<C: CancellationCheck + ?Sized>(
        &mut self,
        capacity: usize,
        cancel: &C,
    ) -> Result<(), String> {
        self.grow(capacity, cancel, false)
    }

    /// Append initialized samples without requiring a default sample value.
    /// Args: source samples and cancellation owner. Returns after crediting only the written length or before growth on pressure.
    pub fn extend_from_slice_now<C: CancellationCheck + ?Sized>(
        &mut self,
        values: &[T],
        cancel: &C,
    ) -> Result<(), String> {
        let end = self
            .values
            .len()
            .checked_add(values.len())
            .ok_or("Buffer length overflows")?;
        self.reserve_now(end, cancel)?;
        self.values.extend_from_slice(values);
        self.lease
            .budget
            .bind_storage(&self.values, &self.lease, &self.owner)
    }

    fn grow<C: CancellationCheck + ?Sized>(
        &mut self,
        capacity: usize,
        cancel: &C,
        wait: bool,
    ) -> Result<(), String> {
        if capacity <= self.values.capacity() {
            return cancel.check();
        }
        let bytes = allocation_bytes::<T>(capacity)?;
        let old = self.bytes();
        self.lease.resize_mode(
            old.checked_add(bytes).ok_or("Output growth overflows")?,
            cancel,
            wait,
        )?;
        if let Err(error) = cancel.check() {
            self.lease.resize_now(old, &|| Ok(()))?;
            return Err(error);
        }
        if self
            .values
            .try_reserve_exact(capacity - self.values.len())
            .is_err()
        {
            self.lease.resize_now(old, &|| Ok(()))?;
            return Err("Not enough memory to grow output storage".into());
        }
        self.lease
            .budget
            .bind_storage(&self.values, &self.lease, &self.owner)?;
        self.lease.resize_now(self.bytes(), &|| Ok(()))?;
        cancel.check()
    }
    /// Copy an allocation only after admitting its independent storage.
    /// Args: cancellation owner. Returns a fallible COW working allocation in the same pool.
    pub fn try_clone<C: CancellationCheck + ?Sized>(&self, cancel: &C) -> Result<Self, String> {
        self.lease.budget.copy(&self.values, cancel)
    }
    /// Copy for a synchronous edit without waiting under pressure.
    /// Args: cancellation owner. Returns admitted working storage or an immediate error.
    pub fn try_clone_now<C: CancellationCheck + ?Sized>(&self, cancel: &C) -> Result<Self, String> {
        self.lease.budget.copy_now(&self.values, cancel)
    }
    /// Transfer storage with its admission intact. Args: none. Returns both owned samples and their lease.
    pub fn into_parts(mut self) -> (Vec<T>, Arc<Lease>) {
        let mut state = self
            .lease
            .budget
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let values = std::mem::take(&mut self.values);
        self.lease.release_credit(&mut state, &self.owner, false);
        self.transferred = true;
        (values, self.lease.clone())
    }
}

impl Allocation<u8> {
    /// Write a checked output chunk and credit only its initialized bytes.
    /// Args: byte offset, data and cancellation owner. Returns after bounded zero-filling/copying or before growth on pressure.
    pub fn write_at<C: CancellationCheck + ?Sized>(
        &mut self,
        offset: usize,
        bytes: &[u8],
        cancel: &C,
    ) -> Result<(), String> {
        let end = offset
            .checked_add(bytes.len())
            .ok_or("Output length overflows")?;
        let capacity = if end > self.values.capacity() {
            end.max(self.values.capacity().saturating_mul(2))
        } else {
            end
        };
        self.grow(capacity, cancel, true)?;
        if end > self.values.len() {
            self.values.resize(end, 0);
        }
        self.values[offset..end].copy_from_slice(bytes);
        self.lease
            .budget
            .bind_storage(&self.values, &self.lease, &self.owner)
    }
}

impl<T: Copy> Drop for Allocation<T> {
    fn drop(&mut self) {
        if !self.transferred {
            let mut state = self
                .lease
                .budget
                .state
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            drop(std::mem::take(&mut self.values));
            self.lease.release_credit(&mut state, &self.owner, true);
        }
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
            #[cfg(test)]
            available_override: None,
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
                    let lease = state.sources.swap_remove(index).1;
                    expired.push(lease);
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
            materialized: AtomicU64::new(0),
            backing: AtomicU64::new(0),
            owner: Mutex::new(Weak::new()),
        });
        lease.resize(bytes, cancel)?;
        Ok(lease)
    }

    /// Admit a synchronous operation without waiting for other buffers.
    /// Args: required bytes and cancellation owner. Returns a lease or an immediate pressure error.
    pub fn reserve_now<C: CancellationCheck + ?Sized>(
        self: &Arc<Self>,
        bytes: u64,
        cancel: &C,
    ) -> Result<Arc<Lease>, String> {
        let lease = Arc::new(Lease {
            budget: self.clone(),
            bytes: AtomicU64::new(0),
            materialized: AtomicU64::new(0),
            backing: AtomicU64::new(0),
            owner: Mutex::new(Weak::new()),
        });
        lease.resize_now(bytes, cancel)?;
        Ok(lease)
    }

    /// Adopt already admitted storage without reserving the same bytes again.
    /// Args: owned plain samples and an exclusive same-pool lease covering their capacity. Returns immutable allocation ownership.
    pub fn adopt<T: Copy>(
        self: &Arc<Self>,
        values: Vec<T>,
        lease: Arc<Lease>,
    ) -> Result<Arc<Allocation<T>>, String> {
        let valid = Arc::ptr_eq(self, &lease.budget)
            && Arc::strong_count(&lease) == 1
            && allocation_bytes::<T>(values.capacity()).is_ok_and(|bytes| lease.bytes() >= bytes);
        if !valid {
            drop(values);
            drop(lease);
            return Err("Buffer adoption needs one exclusive same-pool lease covering its allocation capacity".into());
        }
        let owner = Arc::new(());
        if let Err(error) = self.bind_storage(&values, &lease, &owner) {
            drop(values);
            drop(lease);
            return Err(error);
        }
        Ok(Arc::new(Allocation {
            values,
            owner,
            lease,
            transferred: false,
        }))
    }

    /// Allocate a mutable working buffer inside aggregate admission.
    /// Args: plain sample count and cancellation owner. Returns initialized owned storage or an error before growth.
    pub fn allocate<T: Copy + Default, C: CancellationCheck + ?Sized>(
        self: &Arc<Self>,
        count: usize,
        cancel: &C,
    ) -> Result<Allocation<T>, String> {
        self.allocate_mode(count, cancel, true)
    }

    /// Allocate synchronous working samples without a pressure wait.
    /// Args: plain sample count and cancellation owner. Returns initialized storage or an immediate error.
    pub fn allocate_now<T: Copy + Default, C: CancellationCheck + ?Sized>(
        self: &Arc<Self>,
        count: usize,
        cancel: &C,
    ) -> Result<Allocation<T>, String> {
        self.allocate_mode(count, cancel, false)
    }
    fn allocate_mode<T: Copy + Default, C: CancellationCheck + ?Sized>(
        self: &Arc<Self>,
        count: usize,
        cancel: &C,
        wait: bool,
    ) -> Result<Allocation<T>, String> {
        let bytes = allocation_bytes::<T>(count)?;
        let lease = if wait {
            self.reserve(bytes, cancel)?
        } else {
            self.reserve_now(bytes, cancel)?
        };
        let mut values = Vec::new();
        values
            .try_reserve_exact(count)
            .map_err(|_| "Not enough memory to allocate pixel samples")?;
        lease.resize_mode(allocation_bytes::<T>(values.capacity())?, cancel, wait)?;
        values.resize(count, T::default());
        let owner = Arc::new(());
        self.bind_storage(&values, &lease, &owner)?;
        Ok(Allocation {
            values,
            owner,
            lease,
            transferred: false,
        })
    }

    /// Allocate an independent copy for editing without hidden Clone allocations.
    /// Args: plain source samples and cancellation owner. Returns owned mutable storage from this pool.
    pub fn copy<T: Copy, C: CancellationCheck + ?Sized>(
        self: &Arc<Self>,
        source: &[T],
        cancel: &C,
    ) -> Result<Allocation<T>, String> {
        self.copy_mode(source, cancel, true)
    }

    /// Copy synchronous edit samples without a pressure wait.
    /// Args: plain source samples and cancellation owner. Returns admitted independent storage or an immediate error.
    pub fn copy_now<T: Copy, C: CancellationCheck + ?Sized>(
        self: &Arc<Self>,
        source: &[T],
        cancel: &C,
    ) -> Result<Allocation<T>, String> {
        self.copy_mode(source, cancel, false)
    }
    fn copy_mode<T: Copy, C: CancellationCheck + ?Sized>(
        self: &Arc<Self>,
        source: &[T],
        cancel: &C,
        wait: bool,
    ) -> Result<Allocation<T>, String> {
        let bytes = allocation_bytes::<T>(source.len())?;
        let lease = if wait {
            self.reserve(bytes, cancel)?
        } else {
            self.reserve_now(bytes, cancel)?
        };
        let mut values = Vec::new();
        values
            .try_reserve_exact(source.len())
            .map_err(|_| "Not enough memory to copy pixel samples")?;
        lease.resize_mode(allocation_bytes::<T>(values.capacity())?, cancel, wait)?;
        values.extend_from_slice(source);
        let owner = Arc::new(());
        self.bind_storage(&values, &lease, &owner)?;
        Ok(Allocation {
            values,
            owner,
            lease,
            transferred: false,
        })
    }

    /// Bind initialized credit to one actual buffer's lifetime and capacity.
    /// Args: owned vector, covering same-pool lease and owner dropped after that vector. Returns only after recording its initialized length; spare capacity stays pending.
    fn bind_storage<T: Copy>(
        self: &Arc<Self>,
        values: &Vec<T>,
        lease: &Arc<Lease>,
        owner: &Arc<()>,
    ) -> Result<(), String> {
        let capacity = allocation_bytes::<T>(values.capacity())?;
        let initialized = allocation_bytes::<T>(values.len())?;
        let owner = Arc::downgrade(owner);
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if !Arc::ptr_eq(self, &lease.budget) || capacity > lease.bytes.load(Ordering::Acquire) {
            return Err(
                "Initialized storage needs a same-pool lease covering its actual capacity".into(),
            );
        }
        let mut previous = lease.owner.lock().unwrap_or_else(|e| e.into_inner());
        if previous.strong_count() > 0 && !previous.ptr_eq(&owner) {
            return Err("A memory lease already belongs to another initialized buffer".into());
        }
        *previous = owner;
        state.materialized =
            state.materialized - lease.materialized.load(Ordering::Acquire) + initialized;
        lease.materialized.store(initialized, Ordering::Release);
        lease.backing.store(capacity, Ordering::Release);
        self.wake.notify_all();
        Ok(())
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

    /// Inspect reservations not yet backed by initialized storage.
    /// Args: none. Returns pending bytes after expired source owners are reaped.
    pub fn pending_bytes(&self) -> u64 {
        self.reap();
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.used - state.materialized
    }

    fn available(&self) -> Option<u64> {
        #[cfg(test)]
        if let Some(observe) = self.available_override {
            return observe();
        }
        available_memory()
    }
}

impl Lease {
    fn release_credit(&self, state: &mut State, owner: &Arc<()>, freed: bool) {
        let mut current = self.owner.lock().unwrap_or_else(|e| e.into_inner());
        assert!(current.ptr_eq(&Arc::downgrade(owner)));
        *current = Weak::new();
        state.materialized -= self.materialized.swap(0, Ordering::AcqRel);
        if freed {
            self.backing.store(0, Ordering::Release);
        }
        self.budget.wake.notify_all();
    }

    /// Resize an admitted operation as its actual sensor/output geometry becomes known.
    /// Args: total required bytes and cancellation token. Returns an error before allocation if pressure or bounds cannot admit it.
    pub fn resize<C: CancellationCheck + ?Sized>(
        &self,
        bytes: u64,
        cancel: &C,
    ) -> Result<(), String> {
        self.resize_mode(bytes, cancel, true)
    }

    /// Resize synchronous admission without waiting on other owners.
    /// Args: total bytes and cancellation owner. Returns success or an immediate pressure error.
    pub fn resize_now<C: CancellationCheck + ?Sized>(
        &self,
        bytes: u64,
        cancel: &C,
    ) -> Result<(), String> {
        self.resize_mode(bytes, cancel, false)
    }
    fn resize_mode<C: CancellationCheck + ?Sized>(
        &self,
        bytes: u64,
        cancel: &C,
        wait: bool,
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
            if bytes < self.backing.load(Ordering::Acquire) {
                return Err("Cannot shrink admission below its owned buffer capacity".into());
            }
            let old = self.bytes.load(Ordering::Acquire);
            let old_materialized = self.materialized.load(Ordering::Acquire);
            let materialized = old_materialized.min(bytes);
            let extra = bytes.saturating_sub(old);
            let required = state.used.saturating_sub(old).saturating_add(bytes);
            let initialized = state.materialized - old_materialized + materialized;
            let pending = required - initialized;
            let host_ok = !self.budget.host_pressure
                || extra == 0
                || self
                    .budget
                    .available()
                    .is_none_or(|available| pending.saturating_add(HEADROOM) <= available);
            if required <= self.budget.limit && host_ok {
                state.used = state.used - old + bytes;
                state.materialized = initialized;
                self.bytes.store(bytes, Ordering::Release);
                self.materialized.store(materialized, Ordering::Release);
                self.budget.wake.notify_all();
                return Ok(());
            }
            if !wait || started.elapsed() > Duration::from_secs(30) {
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
        state.materialized = state
            .materialized
            .saturating_sub(self.materialized.load(Ordering::Acquire));
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
    fn typed_growth_retains_initialized_credit_and_admits_transient_old_plus_new() {
        #[derive(Clone, Copy, Debug, PartialEq)]
        struct Sample(u16);
        let check = || Ok(());
        let pool = Budget::new(111, false);
        let mut output = pool.copy_now(&[Sample(7); 16], &check).unwrap();
        output.reserve_now(24, &check).unwrap();
        assert_eq!((output.bytes(), pool.pending_bytes()), (48, 16));
        let pending = pool.reserve_now(15, &check).unwrap();
        let started = Instant::now();
        assert!(output.reserve_now(32, &check).is_err());
        assert!(started.elapsed() < Duration::from_secs(1));
        assert_eq!(
            (
                output.len(),
                output.bytes(),
                pool.usage().0,
                pool.pending_bytes()
            ),
            (16, 48, 63, 31)
        );
        drop(pending);
        assert!(
            output.reserve_now(32, &check).is_err(),
            "48 old + 64 new must not fit a111-byte pool"
        );
        output
            .extend_from_slice_now(&[Sample(9); 8], &check)
            .unwrap();
        assert_eq!((output.len(), pool.pending_bytes()), (24, 0));
        assert_eq!(&output[16..], &[Sample(9); 8]);
        let cancelled = || Err("cancelled".into());
        assert!(
            output
                .extend_from_slice_now(&[Sample(1)], &cancelled)
                .is_err()
        );
        assert_eq!(output.len(), 24);
        assert!(output.reserve_now(usize::MAX, &check).is_err());
        let (values, lease) = output.into_parts();
        assert_eq!(pool.pending_bytes(), 48);
        let output = pool.adopt(values, lease).unwrap();
        let alias = output.clone();
        drop(output);
        assert_eq!((pool.usage().0, pool.pending_bytes()), (48, 0));
        drop(alias);
        assert_eq!((pool.usage().0, pool.pending_bytes()), (0, 0));
    }

    #[test]
    fn cancellation_after_growth_admission_restores_the_existing_owner() {
        let pool = Budget::new(1024, false);
        let mut output = pool.copy_now(&[1u32; 8], &|| Ok(())).unwrap();
        let checks = std::sync::atomic::AtomicUsize::new(0);
        let cancel = || {
            if checks.fetch_add(1, Ordering::AcqRel) == 0 {
                Ok(())
            } else {
                Err("cancelled".into())
            }
        };
        assert!(output.reserve_now(32, &cancel).is_err());
        assert_eq!(
            (
                output.len(),
                output.bytes(),
                pool.usage().0,
                pool.pending_bytes()
            ),
            (8, 32, 32, 0)
        );
        assert_eq!(output.as_slice(), &[1; 8]);
        drop(output);
        assert_eq!(pool.usage().0, 0);
    }

    #[test]
    fn output_writes_credit_only_initialized_length_and_preserve_owners_on_failure() {
        let check = || Ok(());
        let pool = Budget::new(256, false);
        let mut output = pool.allocate_now::<u8, _>(0, &check).unwrap();
        output.write_at(0, &[1; 32], &check).unwrap();
        assert_eq!((pool.usage().0, pool.pending_bytes()), (32, 0));
        output.write_at(40, &[7; 8], &check).unwrap();
        assert_eq!(
            (output.len(), output.bytes(), pool.pending_bytes()),
            (48, 64, 16)
        );
        assert_eq!(&output[32..40], &[0; 8]);
        assert!(output.write_at(256, &[9], &check).is_err());
        assert_eq!((output.len(), output.bytes(), pool.usage().0), (48, 64, 64));
        let cancelled = || Err("cancelled".into());
        assert!(output.write_at(0, &[9], &cancelled).is_err());
        assert_eq!(output[0], 1);
        let (values, lease) = output.into_parts();
        assert_eq!(pool.pending_bytes(), 64);
        let output = pool.adopt(values, lease).unwrap();
        let alias = output.clone();
        drop(output);
        assert_eq!((pool.usage().0, pool.pending_bytes()), (64, 16));
        drop(alias);
        assert_eq!((pool.usage().0, pool.pending_bytes()), (0, 0));
    }
    #[test]
    fn initialized_credit_keeps_spare_capacity_pending_and_tracks_final_alias() {
        static FREE: AtomicU64 = AtomicU64::new(100);
        fn available() -> Option<u64> {
            Some(HEADROOM + FREE.load(Ordering::Acquire))
        }
        let mut pool = Budget::new(256, true);
        Arc::get_mut(&mut pool).unwrap().available_override = Some(available);
        let check = || Ok(());
        let mut values = Vec::with_capacity(80);
        values.extend_from_slice(&[7u8; 48]);
        let capacity = values.capacity() as u64;
        assert_eq!(capacity, 80);
        let lease = pool.reserve_now(capacity, &check).unwrap();
        let owner = pool.adopt(values, lease).unwrap();
        let alias = owner.clone();
        let memory = owner.lease().clone();
        FREE.store(52, Ordering::Release);
        assert_eq!((pool.usage().0, pool.pending_bytes()), (80, 32));
        assert!(memory.resize_now(79, &check).is_err());
        memory.resize_now(100, &check).unwrap();
        assert_eq!(pool.pending_bytes(), 52);
        memory.resize_now(80, &check).unwrap();
        let pending = pool.reserve_now(20, &check).unwrap();
        assert!(pool.reserve_now(1, &check).is_err());
        let cancelled = || Err("Cancelled".into());
        assert!(owner.try_clone_now(&cancelled).is_err());
        drop(owner);
        assert_eq!((pool.usage().0, pool.pending_bytes()), (100, 52));
        drop(alias);
        FREE.store(100, Ordering::Release);
        assert_eq!((pool.usage().0, pool.pending_bytes()), (100, 100));
        assert!(pool.reserve_now(1, &check).is_err());
        memory.resize_now(0, &check).unwrap();
        assert_eq!((pool.usage().0, pool.pending_bytes()), (20, 20));
        drop(memory);
        drop(pending);
        assert_eq!((pool.usage().0, pool.pending_bytes()), (0, 0));
    }

    #[test]
    fn copy_adopt_and_opaque_transfer_preserve_capacity_and_one_owner_credit() {
        let pool = Budget::new(128, false);
        let check = || Ok(());
        let owner = pool.copy_now(&[1u8; 32], &check).unwrap();
        assert_eq!((pool.usage().0, pool.pending_bytes()), (32, 0));
        let copy = owner.try_clone_now(&check).unwrap();
        assert_eq!((pool.usage().0, pool.pending_bytes()), (64, 0));
        let (values, memory) = owner.into_parts();
        assert_eq!((pool.usage().0, pool.pending_bytes()), (64, 32));
        assert!(memory.resize_now(31, &check).is_err());
        let owner = pool.adopt(values, memory).unwrap();
        assert_eq!((pool.usage().0, pool.pending_bytes()), (64, 0));
        assert!(pool.reserve_now(65, &check).is_err());
        drop(owner);
        drop(copy);
        assert_eq!((pool.usage().0, pool.pending_bytes()), (0, 0));
        let opaque = pool.reserve_now(80, &check).unwrap();
        assert_eq!(pool.pending_bytes(), 80);
        opaque.resize_now(40, &check).unwrap();
        assert_eq!(pool.pending_bytes(), 40);
        drop(opaque);
        assert_eq!(pool.pending_bytes(), 0);
    }

    #[test]
    fn host_snapshot_cannot_combine_old_availability_with_new_initialized_credit() {
        use std::sync::{Barrier, mpsc};
        static FREE: AtomicU64 = AtomicU64::new(100);
        static PAUSE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        static GATES: OnceLock<(Barrier, Barrier)> = OnceLock::new();
        fn available() -> Option<u64> {
            let value = FREE.load(Ordering::Acquire);
            if PAUSE.swap(false, Ordering::AcqRel) {
                let gates = GATES.get().unwrap();
                gates.0.wait();
                gates.1.wait();
            }
            Some(HEADROOM + value)
        }
        let gates = GATES.get_or_init(|| (Barrier::new(2), Barrier::new(2)));
        let mut pool = Budget::new(256, true);
        Arc::get_mut(&mut pool).unwrap().available_override = Some(available);
        let memory = pool.reserve_now(80, &|| Ok(())).unwrap();
        PAUSE.store(true, Ordering::Release);
        let contender_pool = pool.clone();
        let contender = std::thread::spawn(move || contender_pool.reserve_now(30, &|| Ok(())));
        gates.0.wait();
        let initialized_pool = pool.clone();
        let (ready, received) = mpsc::channel();
        let initializer = std::thread::spawn(move || {
            let values = vec![9u8; 80];
            FREE.store(20, Ordering::Release);
            let owner = Arc::new(());
            ready.send(()).unwrap();
            initialized_pool
                .bind_storage(&values, &memory, &owner)
                .unwrap();
            Allocation {
                values,
                owner,
                lease: memory,
                transferred: false,
            }
        });
        received.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(
            pool.state.try_lock().is_err(),
            "snapshot must retain the admission lock"
        );
        gates.1.wait();
        assert!(
            contender.join().unwrap().is_err(),
            "pending storage must remain charged through initialization"
        );
        let owner = initializer.join().unwrap();
        assert_eq!((pool.usage().0, pool.pending_bytes()), (80, 0));
        assert!(
            pool.reserve_now(30, &|| Ok(())).is_err(),
            "new credit must use a fresh host observation"
        );
        let pending = pool.reserve_now(20, &|| Ok(())).unwrap();
        assert_eq!(pool.pending_bytes(), 20);
        drop(owner);
        drop(pending);
        assert_eq!((pool.usage().0, pool.pending_bytes()), (0, 0));
    }

    #[test]
    fn synchronous_admission_never_waits_for_retained_buffers() {
        let pool = Budget::new(64, false);
        let check = || Ok(());
        let storage = pool.allocate_now::<u8, _>(48, &check).unwrap();
        let started = Instant::now();
        assert!(pool.reserve_now(17, &check).is_err());
        assert!(pool.allocate_now::<u16, _>(9, &check).is_err());
        assert!(pool.copy_now(&[0u8; 17], &check).is_err());
        assert!(storage.try_clone_now(&check).is_err());
        let lease = pool.reserve_now(16, &check).unwrap();
        assert!(lease.resize_now(17, &check).is_err());
        assert_eq!(lease.bytes(), 16);
        assert_eq!(pool.usage(), (64, 64));
        lease.resize_now(8, &check).unwrap();
        assert_eq!(pool.usage(), (56, 64));
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "synchronous admission waited for unavailable storage"
        );
        drop(storage);
        drop(lease);
        assert_eq!(pool.usage(), (0, 64));
    }
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

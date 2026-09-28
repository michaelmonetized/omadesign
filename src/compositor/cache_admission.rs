//! A small, aging frequency sketch for derived pixels. This only controls which
//! exact-key results stay resident; collisions can never change rendered pixels.
const WIDTH: usize = 512;
const ROWS: usize = 4;
const AGE_AFTER: usize = 1024;

pub(super) struct Admission {
    counters: [u8; WIDTH * ROWS],
    requests: usize,
}

impl Default for Admission {
    fn default() -> Self {
        Self {
            counters: [0; WIDTH * ROWS],
            requests: 0,
        }
    }
}

impl Admission {
    fn positions(key: u64) -> [usize; ROWS] {
        // Mix aligned path pointers as well as small, sequential document IDs.
        let mut hash = key.wrapping_add(0x9e3779b97f4a7c15);
        hash = (hash ^ (hash >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        hash = (hash ^ (hash >> 27)).wrapping_mul(0x94d049bb133111eb);
        hash ^= hash >> 31;
        std::array::from_fn(|row| row * WIDTH + ((hash >> (row * 13)) as usize & (WIDTH - 1)))
    }

    pub(super) fn record(&mut self, key: u64) {
        if self.requests == AGE_AFTER {
            for count in &mut self.counters {
                *count /= 2;
            }
            self.requests = 0;
        }
        self.requests += 1;
        for index in Self::positions(key) {
            self.counters[index] = self.counters[index].saturating_add(1);
        }
    }

    pub(super) fn frequency(&self, key: u64) -> u8 {
        Self::positions(key)
            .into_iter()
            .map(|i| self.counters[i])
            .min()
            .unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_age_so_a_new_document_can_replace_old_pixels() {
        let mut admission = Admission::default();
        for _ in 0..20 {
            admission.record(1);
        }
        assert!(admission.frequency(1) > admission.frequency(2));
        for _ in 0..AGE_AFTER * 5 {
            admission.record(2);
        }
        assert!(admission.frequency(2) > admission.frequency(1));
    }
}

//! A tiny xorshift64* generator.
//!
//! The game only needs "pick a free cell" and "scatter some sparks", which is
//! not worth a dependency or a cryptographic guarantee.

use std::time::{SystemTime, UNIX_EPOCH};

pub struct Rng(u64);

impl Rng {
    /// Seed from the clock, perturbed by an ASLR-dependent stack address and
    /// the pid so two processes started in the same nanosecond still diverge.
    pub fn from_entropy() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x2545_F491_4F6C_DD1D);
        let marker = 0u8;
        let aslr = &marker as *const u8 as usize as u64;
        let pid = std::process::id() as u64;
        Self::from_seed(nanos ^ aslr.rotate_left(17) ^ pid.rotate_left(41))
    }

    pub fn from_seed(seed: u64) -> Self {
        // A zero state is a fixed point for xorshift, so never allow it.
        Self(if seed == 0 {
            0x9E37_79B9_7F4A_7C15
        } else {
            seed
        })
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform in `0..n`. Returns 0 when `n == 0`.
    pub fn below(&mut self, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }
        // Modulo bias is irrelevant at these magnitudes, but use the high bits
        // anyway since xorshift's low bits are the weakest.
        ((self.next_u64() >> 32) % n as u64) as u32
    }

    /// Uniform in `0.0..1.0`.
    pub fn unit(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u32 << 24) as f32
    }

    pub fn range_f32(&mut self, lo: f32, hi: f32) -> f32 {
        lo + self.unit() * (hi - lo)
    }

    /// True with probability `p`.
    pub fn chance(&mut self, p: f32) -> bool {
        self.unit() < p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn below_stays_in_range() {
        let mut rng = Rng::from_seed(42);
        for n in 1..40u32 {
            for _ in 0..200 {
                assert!(rng.below(n) < n, "below({n}) escaped its range");
            }
        }
    }

    #[test]
    fn unit_is_normalised() {
        let mut rng = Rng::from_seed(7);
        for _ in 0..10_000 {
            let u = rng.unit();
            assert!((0.0..1.0).contains(&u), "unit() produced {u}");
        }
    }

    #[test]
    fn zero_seed_does_not_stick() {
        let mut rng = Rng::from_seed(0);
        assert_ne!(rng.next_u64(), 0);
        assert_ne!(rng.next_u64(), 0);
    }
}

// copied from quanta crate

use crate::stats::Variance;

// Run 500 rounds of calibration before we start actually seeing what the numbers look like.
const MINIMUM_CAL_ROUNDS: u64 = 500;

// We want our maximum error to be 10 nanoseconds.
const MAXIMUM_CAL_ERROR_NS: u64 = 10;

// Don't run the calibration loop for longer than 200ms of wall time.
const MAXIMUM_CAL_TIME_NS: u64 = 200 * 1000 * 1000;

#[inline]
fn mul_div_po2_u64(value: u64, numer: u64, denom: u32) -> u64 {
    // Modified muldiv routine where the denominator has to be a power of two. `denom` is expected
    // to be the number of bits to shift, not the actual decimal value.
    let mut v = u128::from(value);
    v *= u128::from(numer);
    v >>= denom;
    v as u64
}

#[derive(Debug, Copy, Clone)]
pub(crate) struct Calibration {
    pub ref_time: u64,
    pub src_time: u64,
    pub scale_factor: u64,
    pub scale_shift: u32,
}

impl Calibration {
    fn new() -> Calibration {
        Calibration {
            ref_time: 0,
            src_time: 0,
            scale_factor: 1,
            scale_shift: 0,
        }
    }

    fn reset_timebases(&mut self, reference: &impl Fn() -> u64, source: &impl Fn() -> u64) {
        self.ref_time = reference();
        self.src_time = source();
    }

    pub(crate) fn scale_src_to_ref(&self, src_raw: u64) -> u64 {
        let delta = src_raw.saturating_sub(self.src_time);
        let scaled = mul_div_po2_u64(delta, self.scale_factor, self.scale_shift);
        scaled + self.ref_time
    }

    pub(crate) fn calibrate(&mut self, reference: &impl Fn() -> u64, source: &impl Fn() -> u64) {
        let mut variance = Variance::default();
        let deadline = reference() + MAXIMUM_CAL_TIME_NS;

        self.reset_timebases(reference, source);

        // Each busy loop should spin for 1 microsecond. (1000 nanoseconds)
        let loop_delta = 1000;
        loop {
            // Busy loop to burn some time.
            let mut last = reference();
            let target = last + loop_delta;
            while last < target {
                last = reference();
            }

            // We put an upper bound on how long we run calibration before to provide a predictable
            // overhead to the calibration process.  In practice, even if we hit the calibration
            // deadline, we should still have run a sufficient number of rounds to get an accurate
            // calibration.
            if last >= deadline {
                break;
            }

            // Adjust our calibration before we take our measurement.
            self.adjust_cal_ratio(reference, source);

            let r_time = reference();
            let s_raw = source();
            let s_time = self.scale_src_to_ref(s_raw);
            variance.add(s_time as f64 - r_time as f64);

            // If we've collected enough samples, check what the mean and mean error are.  If we're
            // already within the target bounds, we can break out of the calibration loop early.
            if variance.has_significant_result() {
                let mean = variance.mean().abs();
                let mean_error = variance.mean_error().abs();
                let mwe = variance.mean_with_error();
                let samples = variance.samples();

                if samples > MINIMUM_CAL_ROUNDS
                    && mwe < MAXIMUM_CAL_ERROR_NS as f64
                    && mean_error / mean <= 1.0
                {
                    break;
                }
            }
        }
    }

    fn adjust_cal_ratio(&mut self, reference: &impl Fn() -> u64, source: &impl Fn() -> u64) {
        // Overall algorithm: measure the delta between our ref/src_time values and "now" versions
        // of them, calculate the ratio between the deltas, and then find a numerator and
        // denominator to express that ratio such that the denominator is always a power of two.
        //
        // In practice, this means we take the "source" delta, and find the next biggest number that
        // is a power of two.  We then figure out the ratio that describes the difference between
        // _those_ two values, and multiple the "reference" delta by that much, which becomes our
        // numerator while the power-of-two "source" delta becomes our denominator.
        //
        // Then, conversion from a raw value simply becomes a multiply and a bit shift instead of a
        // multiply and full-blown divide.
        let ref_end = reference();
        let src_end = source();

        let ref_d = ref_end.wrapping_sub(self.ref_time);
        let src_d = src_end.wrapping_sub(self.src_time);

        let src_d_po2 = src_d
            .checked_next_power_of_two()
            .unwrap_or_else(|| 2_u64.pow(63));

        // TODO: lossy conversion back and forth just to get an approximate value, can we do better
        // with integer math? not sure
        let po2_ratio = src_d_po2 as f64 / src_d as f64;
        self.scale_factor = (ref_d as f64 * po2_ratio) as u64;
        self.scale_shift = src_d_po2.trailing_zeros();
    }
}

impl Default for Calibration {
    fn default() -> Self {
        Self::new()
    }
}
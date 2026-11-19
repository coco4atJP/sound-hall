use std::f32::consts::TAU;

use super::math::{flush_denormal, soft_clip};
use super::waveguide::DelayLine;

/// Stereo modulated chorus using 2 quadrature LFO delay taps.
#[derive(Clone, Debug)]
pub struct StereoChorus {
    left_delay: DelayLine,
    right_delay: DelayLine,
    lfo_phase: f32,
    rate_hz: f32,
    depth_ms: f32,
    mix: f32,
}

impl StereoChorus {
    pub fn new(sr: f32) -> Self {
        let max_samples = ((sr * 0.06) as usize).max(256);
        Self {
            left_delay: DelayLine::new(max_samples),
            right_delay: DelayLine::new(max_samples),
            lfo_phase: 0.0,
            rate_hz: 0.75,
            depth_ms: 4.5,
            mix: 0.28,
        }
    }

    pub fn set_mix(&mut self, mix: f32) {
        self.mix = if mix.is_finite() {
            mix.clamp(0.0, 1.0)
        } else {
            0.0
        };
    }

    pub fn mix(&self) -> f32 {
        self.mix
    }

    pub fn reset(&mut self) {
        self.left_delay.clear();
        self.right_delay.clear();
        self.lfo_phase = 0.0;
    }

    #[inline]
    pub fn process(&mut self, left_in: f32, right_in: f32, sr: f32) -> (f32, f32) {
        self.left_delay.write(left_in);
        self.right_delay.write(right_in);

        if self.mix <= 1e-4 {
            return (0.0, 0.0);
        }

        let sr = sr.max(8000.0);
        self.lfo_phase = (self.lfo_phase + self.rate_hz / sr).rem_euclid(1.0);
        let lfo_l = (self.lfo_phase * TAU).sin();
        let lfo_r = ((self.lfo_phase + 0.25) * TAU).sin(); // 90 deg stereo spread

        let base_ms = 11.0;
        let d_l = ((base_ms + lfo_l * self.depth_ms) * 0.001 * sr).max(2.0);
        let d_r = ((base_ms + lfo_r * self.depth_ms) * 0.001 * sr).max(2.0);

        let w_l = self.left_delay.read_frac(d_l);
        let w_r = self.right_delay.read_frac(d_r);
        (w_l * self.mix, w_r * self.mix)
    }
}

#[derive(Clone, Debug)]
struct CombFilter {
    buf: Vec<f32>,
    idx: usize,
    filter_store: f32,
}

impl CombFilter {
    fn new(size: usize) -> Self {
        Self {
            buf: vec![0.0; size.max(8)],
            idx: 0,
            filter_store: 0.0,
        }
    }

    fn reset(&mut self) {
        self.buf.fill(0.0);
        self.idx = 0;
        self.filter_store = 0.0;
    }

    #[inline]
    fn process(&mut self, input: f32, feedback: f32, damp1: f32, damp2: f32) -> f32 {
        let out = self.buf[self.idx];
        self.filter_store = flush_denormal(out * damp2 + self.filter_store * damp1);
        self.buf[self.idx] = flush_denormal(input + self.filter_store * feedback);
        self.idx += 1;
        if self.idx >= self.buf.len() {
            self.idx = 0;
        }
        out
    }
}

#[derive(Clone, Debug)]
struct AllpassDiffuser {
    buf: Vec<f32>,
    idx: usize,
}

impl AllpassDiffuser {
    fn new(size: usize) -> Self {
        Self {
            buf: vec![0.0; size.max(8)],
            idx: 0,
        }
    }

    fn reset(&mut self) {
        self.buf.fill(0.0);
        self.idx = 0;
    }

    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        let buf_out = self.buf[self.idx];
        let out = -input + buf_out;
        self.buf[self.idx] = flush_denormal(input + buf_out * 0.5);
        self.idx += 1;
        if self.idx >= self.buf.len() {
            self.idx = 0;
        }
        out
    }
}

const COMB_TUNINGS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLPASS_TUNINGS: [usize; 4] = [556, 441, 341, 225];
const STEREO_SPREAD: usize = 23;

/// Schroeder / Freeverb-inspired stereo algorithmic hall reverb.
#[derive(Clone, Debug)]
pub struct HallReverb {
    combs_l: Vec<CombFilter>,
    combs_r: Vec<CombFilter>,
    allpasses_l: Vec<AllpassDiffuser>,
    allpasses_r: Vec<AllpassDiffuser>,
    room_size: f32,
    damp: f32,
    wet_mix: f32,
    width: f32,
}

impl HallReverb {
    pub fn new(sr: f32) -> Self {
        let scale = (sr.max(8000.0) / 44100.0).max(0.25);
        let spread = ((STEREO_SPREAD as f32) * scale).round() as usize;

        let combs_l = COMB_TUNINGS
            .iter()
            .map(|&t| CombFilter::new(((t as f32) * scale).round() as usize))
            .collect();
        let combs_r = COMB_TUNINGS
            .iter()
            .map(|&t| CombFilter::new(((t as f32) * scale).round() as usize + spread))
            .collect();
        let allpasses_l = ALLPASS_TUNINGS
            .iter()
            .map(|&t| AllpassDiffuser::new(((t as f32) * scale).round() as usize))
            .collect();
        let allpasses_r = ALLPASS_TUNINGS
            .iter()
            .map(|&t| AllpassDiffuser::new(((t as f32) * scale).round() as usize + spread))
            .collect();

        Self {
            combs_l,
            combs_r,
            allpasses_l,
            allpasses_r,
            room_size: 0.78,
            damp: 0.38,
            wet_mix: 0.30,
            width: 0.85,
        }
    }

    pub fn set_mix(&mut self, mix: f32) {
        self.wet_mix = if mix.is_finite() {
            mix.clamp(0.0, 1.0)
        } else {
            0.0
        };
    }

    pub fn mix(&self) -> f32 {
        self.wet_mix
    }

    pub fn set_room_size(&mut self, room_size: f32) {
        self.room_size = if room_size.is_finite() {
            room_size.clamp(0.1, 0.96)
        } else {
            0.78
        };
    }

    pub fn set_damp(&mut self, damp: f32) {
        self.damp = if damp.is_finite() {
            damp.clamp(0.0, 0.95)
        } else {
            0.38
        };
    }

    pub fn reset(&mut self) {
        for c in &mut self.combs_l {
            c.reset();
        }
        for c in &mut self.combs_r {
            c.reset();
        }
        for a in &mut self.allpasses_l {
            a.reset();
        }
        for a in &mut self.allpasses_r {
            a.reset();
        }
    }

    /// Processes wet send input `(left, right)` and returns wet stereo reverb output.
    #[inline]
    pub fn process_send(&mut self, left_send: f32, right_send: f32) -> (f32, f32) {
        if self.wet_mix <= 1e-4 {
            return (0.0, 0.0);
        }

        let input = (left_send + right_send) * 0.042;
        let feedback = 0.70 + self.room_size * 0.27;
        let damp1 = self.damp * 0.5;
        let damp2 = 1.0 - damp1;

        let mut out_l = 0.0_f32;
        let mut out_r = 0.0_f32;
        for c in &mut self.combs_l {
            out_l += c.process(input, feedback, damp1, damp2);
        }
        for c in &mut self.combs_r {
            out_r += c.process(input, feedback, damp1, damp2);
        }

        for a in &mut self.allpasses_l {
            out_l = a.process(out_l);
        }
        for a in &mut self.allpasses_r {
            out_r = a.process(out_r);
        }

        let wet1 = self.wet_mix * (self.width * 0.5 + 0.5);
        let wet2 = self.wet_mix * ((1.0 - self.width) * 0.5);
        (out_l * wet1 + out_r * wet2, out_r * wet1 + out_l * wet2)
    }
}

/// Gentle master bus soft limiter ensuring output stays strictly in [-1.0, 1.0].
#[derive(Clone, Debug, Default)]
pub struct MasterLimiter {
    knee: f32,
}

impl MasterLimiter {
    pub fn new() -> Self {
        Self { knee: 0.85 }
    }

    #[inline]
    pub fn process(&self, x: f32) -> f32 {
        if !x.is_finite() {
            return 0.0;
        }
        let abs_x = x.abs();
        if abs_x <= self.knee {
            x
        } else {
            let excess = (abs_x - self.knee) / (1.0 - self.knee);
            let shaped = self.knee + (1.0 - self.knee) * soft_clip(excess);
            (x.signum() * shaped).clamp(-1.0, 1.0)
        }
    }
}

use std::f32::consts::TAU;

use super::math::flush_denormal;
use super::waveguide::DelayLine;

/// Cheap quadrature sine oscillator (rotating phasor) for slow modulation.
///
/// Produces `sin` and `cos` of the same phase with two multiplies per sample, and
/// renormalises periodically so its amplitude never drifts.
#[derive(Clone, Debug)]
struct QuadLfo {
    re: f32,
    im: f32,
    c: f32,
    s: f32,
    counter: u32,
}

impl QuadLfo {
    fn new(rate_hz: f32, phase: f32, sr: f32) -> Self {
        let w = TAU * rate_hz / sr.max(8000.0);
        let p = TAU * phase;
        Self {
            re: p.cos(),
            im: p.sin(),
            c: w.cos(),
            s: w.sin(),
            counter: 0,
        }
    }

    /// Advances one sample and returns `(sin, cos)`.
    #[inline]
    fn next(&mut self) -> (f32, f32) {
        let re = self.re * self.c - self.im * self.s;
        let im = self.re * self.s + self.im * self.c;
        self.re = re;
        self.im = im;
        self.counter = self.counter.wrapping_add(1);
        if self.counter & 0x3FF == 0 {
            let norm = (re * re + im * im).sqrt().max(1e-6);
            self.re /= norm;
            self.im /= norm;
        }
        (self.im, self.re)
    }
}

/// Lush stereo chorus: two modulated taps per side with quadrature LFOs.
///
/// Two taps at different base delays and rates average out the obvious "seasick"
/// pitch wobble of a single-tap chorus while keeping the shimmer and width.
#[derive(Clone, Debug)]
pub struct StereoChorus {
    left_delay: DelayLine,
    right_delay: DelayLine,
    lfo_a: QuadLfo,
    lfo_b: QuadLfo,
    sr: f32,
    mix: f32,
}

impl StereoChorus {
    const BASE_A_MS: f32 = 8.5;
    const BASE_B_MS: f32 = 13.5;
    const DEPTH_A_MS: f32 = 2.4;
    const DEPTH_B_MS: f32 = 3.1;

    pub fn new(sr: f32) -> Self {
        let sr = sr.max(8000.0);
        let max_samples = ((sr * 0.06) as usize).max(256);
        Self {
            left_delay: DelayLine::new(max_samples),
            right_delay: DelayLine::new(max_samples),
            lfo_a: QuadLfo::new(0.43, 0.0, sr),
            lfo_b: QuadLfo::new(0.71, 0.37, sr),
            sr,
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
    }

    #[inline]
    pub fn process(&mut self, left_in: f32, right_in: f32, _sr: f32) -> (f32, f32) {
        self.left_delay.write(left_in);
        self.right_delay.write(right_in);

        if self.mix <= 1e-4 {
            return (0.0, 0.0);
        }

        let ms = 0.001 * self.sr;
        let (sa, ca) = self.lfo_a.next();
        let (sb, cb) = self.lfo_b.next();
        // Left uses sin, right uses cos (90 deg) of each LFO for a wide, stable image
        let la = (Self::BASE_A_MS + sa * Self::DEPTH_A_MS) * ms;
        let lb = (Self::BASE_B_MS + sb * Self::DEPTH_B_MS) * ms;
        let ra = (Self::BASE_A_MS + ca * Self::DEPTH_A_MS) * ms;
        let rb = (Self::BASE_B_MS - cb * Self::DEPTH_B_MS) * ms;

        let w_l = self.left_delay.read_frac(la) + self.left_delay.read_frac(lb);
        let w_r = self.right_delay.read_frac(ra) + self.right_delay.read_frac(rb);
        let g = self.mix * core::f32::consts::FRAC_1_SQRT_2;
        (w_l * g, w_r * g)
    }
}

/// Schroeder allpass diffuser with configurable coefficient.
#[derive(Clone, Debug)]
struct Diffuser {
    buf: Vec<f32>,
    idx: usize,
    g: f32,
}

impl Diffuser {
    fn new(len: usize, g: f32) -> Self {
        Self {
            buf: vec![0.0; len.max(4)],
            idx: 0,
            g,
        }
    }

    fn reset(&mut self) {
        self.buf.fill(0.0);
        self.idx = 0;
    }

    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let d = self.buf[self.idx];
        let v = x + self.g * d;
        self.buf[self.idx] = flush_denormal(v);
        self.idx += 1;
        if self.idx >= self.buf.len() {
            self.idx = 0;
        }
        d - self.g * v
    }
}

/// Fixed-length circular buffer read at arbitrary integer taps (pre-delay + early reflections).
#[derive(Clone, Debug)]
struct TapDelay {
    buf: Vec<f32>,
    idx: usize,
}

impl TapDelay {
    fn new(len: usize) -> Self {
        Self {
            buf: vec![0.0; len.max(8)],
            idx: 0,
        }
    }

    fn reset(&mut self) {
        self.buf.fill(0.0);
        self.idx = 0;
    }

    #[inline]
    fn write(&mut self, x: f32) {
        self.buf[self.idx] = x;
        self.idx += 1;
        if self.idx >= self.buf.len() {
            self.idx = 0;
        }
    }

    /// Reads the sample written `d` samples ago (`1 <= d < len`).
    #[inline]
    fn tap(&self, d: usize) -> f32 {
        let len = self.buf.len();
        let d = d.clamp(1, len - 1);
        let i = if self.idx >= d {
            self.idx - d
        } else {
            self.idx + len - d
        };
        self.buf[i]
    }
}

const FDN_N: usize = 8;
/// Tank delay lengths (ms) at room size 1.0 — spread ~1:2.4, chosen to avoid common factors.
const FDN_MS: [f32; FDN_N] = [31.3, 37.9, 43.1, 49.7, 56.3, 63.1, 71.9, 79.3];
/// Output sign patterns: mutually orthogonal so left/right tails are decorrelated.
const OUT_L: [f32; FDN_N] = [1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0];
const OUT_R: [f32; FDN_N] = [1.0, 1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0];
/// Early reflection taps (ms, gain) per side.
const ER_L: [(f32, f32); 6] = [
    (7.1, 0.82),
    (11.3, -0.66),
    (16.9, 0.58),
    (23.7, -0.47),
    (29.3, 0.39),
    (37.1, -0.30),
];
const ER_R: [(f32, f32); 6] = [
    (8.3, 0.80),
    (13.1, -0.64),
    (18.7, 0.55),
    (25.9, -0.45),
    (31.9, 0.37),
    (40.3, -0.28),
];

/// Smooth, dense stereo hall reverb: 8-line feedback delay network.
///
/// Signal flow: pre-delay → input band-limiting (low cut removes bass/kick mud, high cut
/// tames fizz) → allpass diffusion → early reflections + an 8×8 Householder FDN with
/// slowly modulated delay lines (no metallic ringing) and per-line absorption filters that
/// give an exact, frequency-dependent RT60 (highs decay faster than lows, like a real hall).
#[derive(Clone, Debug)]
pub struct HallReverb {
    sr: f32,
    predelay: TapDelay,
    predelay_side: TapDelay,
    predelay_samples: usize,
    in_hp_l: f32,
    in_hp_r: f32,
    in_lp_l: f32,
    in_lp_r: f32,
    hp_coeff: f32,
    lp_coeff: f32,
    diff_l: [Diffuser; 3],
    diff_r: [Diffuser; 3],
    lines: Vec<DelayLine>,
    line_len: [f32; FDN_N],
    damp_state: [f32; FDN_N],
    damp_gain: [f32; FDN_N],
    damp_pole: [f32; FDN_N],
    lfo_a: QuadLfo,
    lfo_b: QuadLfo,
    mod_depth: f32,
    er_l: [(usize, f32); 6],
    er_r: [(usize, f32); 6],
    room_size: f32,
    damp: f32,
    wet_mix: f32,
    width: f32,
}

impl HallReverb {
    /// Overall wet level calibration (keeps the send/return balance of the engine's mixes).
    const OUTPUT_GAIN: f32 = 1.6;
    const ER_LEVEL: f32 = 0.22;

    pub fn new(sr: f32) -> Self {
        let sr = sr.max(8000.0);
        let ms = sr * 0.001;
        let max_line = ((FDN_MS[FDN_N - 1] * 1.3 + 4.0) * ms) as usize + 16;
        let lines = (0..FDN_N).map(|_| DelayLine::new(max_line)).collect();
        let diff = |base_ms: [f32; 3]| {
            [
                Diffuser::new((base_ms[0] * ms) as usize, 0.62),
                Diffuser::new((base_ms[1] * ms) as usize, 0.58),
                Diffuser::new((base_ms[2] * ms) as usize, 0.52),
            ]
        };
        let er = |taps: &[(f32, f32); 6]| {
            let mut out = [(1usize, 0.0f32); 6];
            for (o, &(t, g)) in out.iter_mut().zip(taps) {
                *o = (((t * ms) as usize).max(1), g);
            }
            out
        };
        let mut rev = Self {
            sr,
            predelay: TapDelay::new((0.12 * sr) as usize),
            predelay_side: TapDelay::new((0.12 * sr) as usize),
            predelay_samples: (0.018 * sr) as usize,
            in_hp_l: 0.0,
            in_hp_r: 0.0,
            in_lp_l: 0.0,
            in_lp_r: 0.0,
            hp_coeff: (-TAU * 170.0 / sr).exp(),
            lp_coeff: (-TAU * 8500.0 / sr).exp(),
            diff_l: diff([4.77, 3.59, 2.73]),
            diff_r: diff([5.21, 3.87, 2.51]),
            lines,
            line_len: [0.0; FDN_N],
            damp_state: [0.0; FDN_N],
            damp_gain: [0.0; FDN_N],
            damp_pole: [0.0; FDN_N],
            lfo_a: QuadLfo::new(0.31, 0.0, sr),
            lfo_b: QuadLfo::new(0.53, 0.25, sr),
            mod_depth: 0.00032 * sr,
            er_l: er(&ER_L),
            er_r: er(&ER_R),
            room_size: 0.78,
            damp: 0.38,
            wet_mix: 0.30,
            width: 0.9,
        };
        rev.update_tank();
        rev
    }

    /// Recomputes line lengths and absorption filters from `room_size` and `damp`.
    fn update_tank(&mut self) {
        let ms = self.sr * 0.001;
        let size_scale = 0.62 + 0.48 * self.room_size;
        // RT60 at low frequencies: ~1.0 s (small) .. ~4.2 s (huge)
        let rt_low = 0.55 + self.room_size * self.room_size * 3.6;
        // High frequencies decay faster as damping increases
        let rt_high = (rt_low * (1.0 - 0.82 * self.damp)).max(0.12);
        for (i, &base_ms) in FDN_MS.iter().enumerate() {
            let len = base_ms * size_scale * ms;
            self.line_len[i] = len;
            let g_dc = 10.0_f32.powf(-3.0 * len / (rt_low * self.sr));
            let g_hf = 10.0_f32.powf(-3.0 * len / (rt_high * self.sr));
            let r = (g_hf / g_dc).clamp(1e-4, 1.0);
            // One-pole absorption filter: DC gain g_dc, Nyquist gain g_hf
            self.damp_gain[i] = g_dc;
            self.damp_pole[i] = (1.0 - r) / (1.0 + r);
        }
        self.predelay_samples = ((8.0 + 22.0 * self.room_size) * ms) as usize;
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
        self.update_tank();
    }

    pub fn set_damp(&mut self, damp: f32) {
        self.damp = if damp.is_finite() {
            damp.clamp(0.0, 0.95)
        } else {
            0.38
        };
        self.update_tank();
    }

    pub fn reset(&mut self) {
        self.predelay.reset();
        self.predelay_side.reset();
        self.in_hp_l = 0.0;
        self.in_hp_r = 0.0;
        self.in_lp_l = 0.0;
        self.in_lp_r = 0.0;
        for d in self.diff_l.iter_mut().chain(self.diff_r.iter_mut()) {
            d.reset();
        }
        for l in &mut self.lines {
            l.clear();
        }
        self.damp_state = [0.0; FDN_N];
    }

    /// Processes wet send input `(left, right)` and returns wet stereo reverb output.
    #[inline]
    pub fn process_send(&mut self, left_send: f32, right_send: f32) -> (f32, f32) {
        if self.wet_mix <= 1e-4 {
            return (0.0, 0.0);
        }

        // Input band-limiting: one-pole low cut (~170 Hz) and high cut (~8.5 kHz)
        let hp = self.hp_coeff;
        let lp = self.lp_coeff;
        self.in_hp_l = flush_denormal(left_send + hp * (self.in_hp_l - left_send));
        self.in_hp_r = flush_denormal(right_send + hp * (self.in_hp_r - right_send));
        let bl = left_send - self.in_hp_l;
        let br = right_send - self.in_hp_r;
        self.in_lp_l = flush_denormal(bl + lp * (self.in_lp_l - bl));
        self.in_lp_r = flush_denormal(br + lp * (self.in_lp_r - br));

        // Pre-delay in mid/side form
        let mid = 0.5 * (self.in_lp_l + self.in_lp_r);
        let side_in = 0.5 * (self.in_lp_l - self.in_lp_r);
        self.predelay.write(mid);
        self.predelay_side.write(side_in);
        let pd = self.predelay.tap(self.predelay_samples);
        let side = self.predelay_side.tap(self.predelay_samples);

        // Early reflections straight from the pre-delayed input
        let mut er_l = 0.0_f32;
        let mut er_r = 0.0_f32;
        for &(t, g) in &self.er_l {
            er_l += self.predelay.tap(self.predelay_samples + t) * g;
        }
        for &(t, g) in &self.er_r {
            er_r += self.predelay.tap(self.predelay_samples + t) * g;
        }

        // Diffuse into two decorrelated feeds (side component adds width to the onset)
        let mut feed_l = pd + side * 0.5;
        let mut feed_r = pd - side * 0.5;
        for d in &mut self.diff_l {
            feed_l = d.process(feed_l);
        }
        for d in &mut self.diff_r {
            feed_r = d.process(feed_r);
        }

        // Read tank lines (lines 0, 3, 5, 6 are gently modulated)
        let (sa, ca) = self.lfo_a.next();
        let (sb, cb) = self.lfo_b.next();
        let mods = [sa, 0.0, 0.0, cb, 0.0, ca, sb, 0.0];
        let mut taps = [0.0_f32; FDN_N];
        for i in 0..FDN_N {
            let raw = if mods[i] != 0.0 {
                self.lines[i].read_frac(self.line_len[i] + mods[i] * self.mod_depth)
            } else {
                self.lines[i].read_int(self.line_len[i] as usize)
            };
            // Absorption filter
            let p = self.damp_pole[i];
            let y = self.damp_gain[i] * (1.0 - p) * raw + p * self.damp_state[i];
            self.damp_state[i] = flush_denormal(y);
            taps[i] = y;
        }

        // Householder feedback: y_i = x_i - (2/N) * sum(x)
        let sum: f32 = taps.iter().sum();
        let house = sum * (2.0 / FDN_N as f32);
        let mut out_l = 0.0_f32;
        let mut out_r = 0.0_f32;
        for i in 0..FDN_N {
            let feed = if i % 2 == 0 { feed_l } else { feed_r };
            self.lines[i].write(taps[i] - house + feed);
            out_l += taps[i] * OUT_L[i];
            out_r += taps[i] * OUT_R[i];
        }

        let tail_norm = 1.0 / (FDN_N as f32).sqrt();
        let wl = (out_l * tail_norm + er_l * Self::ER_LEVEL) * Self::OUTPUT_GAIN;
        let wr = (out_r * tail_norm + er_r * Self::ER_LEVEL) * Self::OUTPUT_GAIN;

        let wet1 = self.wet_mix * (self.width * 0.5 + 0.5);
        let wet2 = self.wet_mix * ((1.0 - self.width) * 0.5);
        (wl * wet1 + wr * wet2, wr * wet1 + wl * wet2)
    }
}

/// Stereo-linked look-ahead brickwall limiter for the master bus.
///
/// Instead of waveshaping peaks (which distorts), it computes the gain each sample needs to
/// stay below the ceiling, holds the minimum over the look-ahead window, releases smoothly,
/// and averages the gain over the window so reductions ramp in *before* the peak arrives.
/// Below the ceiling it is bit-transparent.
#[derive(Clone, Debug)]
pub struct MasterLimiter {
    ceiling: f32,
    lookahead: usize,
    delay_l: Vec<f32>,
    delay_r: Vec<f32>,
    req_ring: Vec<f32>,
    avg_ring: Vec<f32>,
    pos: usize,
    avg_sum: f64,
    release_coeff: f32,
    env: f32,
}

impl Default for MasterLimiter {
    fn default() -> Self {
        Self::new(44100.0)
    }
}

impl MasterLimiter {
    /// Ceiling of -0.5 dBFS.
    pub const CEILING: f32 = 0.944;

    pub fn new(sr: f32) -> Self {
        let sr = if sr.is_finite() {
            sr.max(8000.0)
        } else {
            44100.0
        };
        let lookahead = ((0.0015 * sr) as usize).max(8);
        Self {
            ceiling: Self::CEILING,
            lookahead,
            delay_l: vec![0.0; lookahead],
            delay_r: vec![0.0; lookahead],
            req_ring: vec![1.0; lookahead],
            avg_ring: vec![1.0; lookahead],
            pos: 0,
            avg_sum: lookahead as f64,
            release_coeff: (-1.0 / (0.12 * sr)).exp(),
            env: 1.0,
        }
    }

    /// Latency introduced by the look-ahead, in samples.
    pub fn latency(&self) -> usize {
        self.lookahead - 1
    }

    /// Current gain reduction factor (1.0 = no limiting).
    pub fn gain(&self) -> f32 {
        (self.avg_sum / self.lookahead as f64) as f32
    }

    pub fn reset(&mut self) {
        self.delay_l.fill(0.0);
        self.delay_r.fill(0.0);
        self.req_ring.fill(1.0);
        self.avg_ring.fill(1.0);
        self.pos = 0;
        self.avg_sum = self.lookahead as f64;
        self.env = 1.0;
    }

    #[inline]
    pub fn process(&mut self, left: f32, right: f32) -> (f32, f32) {
        let l = if left.is_finite() { left } else { 0.0 };
        let r = if right.is_finite() { right } else { 0.0 };
        let n = self.lookahead;
        let pos = self.pos;

        let peak = l.abs().max(r.abs());
        let req = if peak > self.ceiling {
            self.ceiling / peak
        } else {
            1.0
        };
        self.req_ring[pos] = req;

        // Minimum required gain over the look-ahead window
        let mut min_req = 1.0_f32;
        for &g in &self.req_ring {
            min_req = min_req.min(g);
        }

        // Instant attack (the averaging below smooths it), exponential release
        self.env = if min_req < self.env {
            min_req
        } else {
            min_req + (self.env - min_req) * self.release_coeff
        };

        // Moving average over the window -> gain ramps reach `min_req` exactly on the peak
        self.avg_sum += (self.env - self.avg_ring[pos]) as f64;
        self.avg_ring[pos] = self.env;
        let gain = (self.avg_sum / n as f64) as f32;

        // Delay audio by n - 1 samples
        let read = (pos + 1) % n;
        let out_l = self.delay_l[read] * gain;
        let out_r = self.delay_r[read] * gain;
        self.delay_l[pos] = l;
        self.delay_r[pos] = r;
        self.pos = read;

        (out_l.clamp(-1.0, 1.0), out_r.clamp(-1.0, 1.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limiter_is_transparent_below_ceiling_and_bounds_peaks() {
        let sr = 44100.0;
        let mut lim = MasterLimiter::new(sr);
        let lat = lim.latency();
        let input: Vec<f32> = (0..4000)
            .map(|i| 0.5 * (i as f32 * TAU * 440.0 / sr).sin())
            .collect();
        let out: Vec<f32> = input.iter().map(|&x| lim.process(x, x).0).collect();
        for i in lat..input.len() {
            assert!((out[i] - input[i - lat]).abs() < 1e-6);
        }

        let mut lim = MasterLimiter::new(sr);
        let mut max_out = 0.0_f32;
        for i in 0..20000 {
            let x = 3.0 * (i as f32 * TAU * 110.0 / sr).sin();
            let (l, r) = lim.process(x, -x);
            max_out = max_out.max(l.abs()).max(r.abs());
        }
        assert!(
            max_out <= MasterLimiter::CEILING + 1e-3,
            "max_out={max_out}"
        );
    }

    #[test]
    fn reverb_tail_is_stable_and_decays() {
        let sr = 44100.0;
        let mut rev = HallReverb::new(sr);
        rev.set_mix(1.0);
        let mut energy_early = 0.0_f32;
        let mut energy_late = 0.0_f32;
        for i in 0..(sr as usize * 6) {
            let x = if i < 64 { 1.0 } else { 0.0 };
            let (l, r) = rev.process_send(x, x);
            assert!(l.is_finite() && r.is_finite());
            if i < sr as usize / 2 {
                energy_early += l * l + r * r;
            } else if i > sr as usize * 5 {
                energy_late += l * l + r * r;
            }
        }
        assert!(energy_early > 1e-4);
        assert!(energy_late < energy_early * 1e-4, "tail must decay");
    }
}

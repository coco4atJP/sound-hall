use std::f32::consts::TAU;

/// ADSR envelope times (in seconds) and sustain level [0.0, 1.0].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AdsrParams {
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
}

impl AdsrParams {
    pub const fn new(attack: f32, decay: f32, sustain: f32, release: f32) -> Self {
        Self {
            attack,
            decay,
            sustain,
            release,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AdsrStage {
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

/// Smooth click-free ADSR envelope generator.
#[derive(Clone, Debug)]
pub struct Adsr {
    params: AdsrParams,
    stage: AdsrStage,
    level: f32,
    release_start: f32,
    release_progress: f32,
    quick_release: bool,
}

impl Adsr {
    pub fn new(params: AdsrParams) -> Self {
        Self {
            params,
            stage: AdsrStage::Idle,
            level: 0.0,
            release_start: 0.0,
            release_progress: 0.0,
            quick_release: false,
        }
    }

    pub fn trigger(&mut self) {
        self.stage = AdsrStage::Attack;
        self.quick_release = false;
        self.release_progress = 0.0;
    }

    pub fn release(&mut self) {
        if self.stage != AdsrStage::Idle && self.stage != AdsrStage::Release {
            self.stage = AdsrStage::Release;
            self.release_start = self.level;
            self.release_progress = 0.0;
        }
    }

    /// Triggers a ~5ms smooth fade-out for voice stealing.
    pub fn force_quick_release(&mut self) {
        if self.stage != AdsrStage::Idle {
            self.stage = AdsrStage::Release;
            self.release_start = self.level;
            self.release_progress = 0.0;
            self.quick_release = true;
        }
    }

    #[inline]
    pub fn is_finished(&self) -> bool {
        self.stage == AdsrStage::Idle
    }

    #[inline]
    pub fn is_releasing(&self) -> bool {
        self.stage == AdsrStage::Release
    }

    #[inline]
    pub fn level(&self) -> f32 {
        self.level
    }

    #[inline]
    pub fn next(&mut self, sr: f32) -> f32 {
        let sr = sr.max(8000.0);
        let dt = 1.0 / sr;
        match self.stage {
            AdsrStage::Idle => 0.0,
            AdsrStage::Attack => {
                let atk = self.params.attack.max(0.0005);
                self.level += dt / atk;
                if self.level >= 1.0 {
                    self.level = 1.0;
                    self.stage = AdsrStage::Decay;
                }
                self.level
            }
            AdsrStage::Decay => {
                let sus = self.params.sustain.clamp(0.0, 1.0);
                let dec = self.params.decay.max(0.001);
                let coeff = (-4.5 * dt / dec).exp();
                self.level = sus + (self.level - sus) * coeff;
                if (self.level - sus).abs() < 0.0015 {
                    self.level = sus;
                    if sus <= 1e-5 {
                        self.level = 0.0;
                        self.stage = AdsrStage::Idle;
                    } else {
                        self.stage = AdsrStage::Sustain;
                    }
                }
                self.level
            }
            AdsrStage::Sustain => {
                self.level = self.params.sustain.clamp(0.0, 1.0);
                self.level
            }
            AdsrStage::Release => {
                let rel = if self.quick_release {
                    0.005
                } else {
                    self.params.release.max(0.002)
                };
                self.release_progress += dt / rel;
                if self.release_progress >= 1.0 {
                    self.level = 0.0;
                    self.stage = AdsrStage::Idle;
                } else {
                    // Exponential-like curve reaching exact 0 at release_progress == 1.0
                    let t = 1.0 - self.release_progress;
                    self.level = self.release_start * t * t;
                }
                self.level
            }
        }
    }
}

/// Low-frequency oscillator with onset delay and fade-in.
#[derive(Clone, Debug)]
pub struct Lfo {
    rate_hz: f32,
    delay_sec: f32,
    fade_sec: f32,
    phase: f32,
    elapsed: f32,
}

impl Lfo {
    pub fn new(rate_hz: f32, delay_sec: f32, fade_sec: f32) -> Self {
        Self {
            rate_hz: rate_hz.max(0.0),
            delay_sec: delay_sec.max(0.0),
            fade_sec: fade_sec.max(0.001),
            phase: 0.0,
            elapsed: 0.0,
        }
    }

    pub fn reset(&mut self) {
        self.phase = 0.0;
        self.elapsed = 0.0;
    }

    #[inline]
    pub fn next_sine(&mut self, sr: f32) -> f32 {
        let dt = 1.0 / sr.max(8000.0);
        self.elapsed += dt;
        self.phase = (self.phase + self.rate_hz * dt).rem_euclid(1.0);
        if self.elapsed < self.delay_sec {
            return 0.0;
        }
        let fade = ((self.elapsed - self.delay_sec) / self.fade_sec).clamp(0.0, 1.0);
        (self.phase * TAU).sin() * fade
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adsr_completes_cleanly() {
        let mut env = Adsr::new(AdsrParams::new(0.01, 0.05, 0.6, 0.05));
        env.trigger();
        let sr = 44100.0;
        let mut peak = 0.0_f32;
        for _ in 0..4410 {
            peak = peak.max(env.next(sr));
        }
        assert!((peak - 1.0).abs() < 0.01);
        env.release();
        for _ in 0..4410 {
            env.next(sr);
        }
        assert!(env.is_finished());
        assert_eq!(env.next(sr), 0.0);
    }
}

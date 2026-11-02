//! Voice effects, applied while a voice speaks: radio, telephone, megaphone,
//! robot, cave, hall, walkie-talkie, underwater and whisper.
//!
//! Each effect is a short chain of classic DSP blocks (RBJ biquads,
//! saturation, µ-law, ring modulation, comb filters, a Freeverb-style
//! reverb, a modulated delay, STFT phase randomisation). A [`Chain`] lives
//! for one message: its pieces pass through one after another, so filters
//! and reverbs carry on seamlessly between them, and [`Chain::finish`]
//! returns the tail (reverb decay, squelch). Everything runs on the mono
//! clip at its own rate before it is resampled for the device; the cost is
//! a fraction of a percent of real time (see `costs_little`).

use std::f32::consts::{PI, TAU};
use std::sync::Arc;

use realfft::num_complex::Complex;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};

/// Effect names as written in presets (`|fx=radio`).
pub const NAMES: [&str; 9] = ["radio", "telephone", "megaphone", "robot", "cave", "hall", "walkie", "underwater", "whisper"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fx {
    Radio,
    Telephone,
    Megaphone,
    Robot,
    Cave,
    Hall,
    Walkie,
    Underwater,
    Whisper,
}

pub const ALL: [Fx; 9] = [Fx::Radio, Fx::Telephone, Fx::Megaphone, Fx::Robot, Fx::Cave, Fx::Hall, Fx::Walkie, Fx::Underwater, Fx::Whisper];

impl Fx {
    pub fn parse(name: &str) -> Option<Self> {
        let n = name.trim();
        ALL.into_iter().find(|f| f.name().eq_ignore_ascii_case(n))
    }

    pub fn name(self) -> &'static str {
        NAMES[self as usize]
    }

    /// Loudness correction after the chain, measured on 60 real VCP voices
    /// brought to -20 dBFS (`cargo run --release --example fx_levels`): every effect plays at about the
    /// level of the dry voice.
    pub fn makeup_db(self) -> f32 {
        match self {
            Fx::Radio => -7.1,
            Fx::Telephone => -2.8,
            Fx::Megaphone => -9.9,
            Fx::Robot => 1.0,
            Fx::Cave => 0.7,
            Fx::Hall => 1.0,
            Fx::Walkie => -8.9,
            Fx::Underwater => 5.2,
            Fx::Whisper => 6.4,
        }
    }
}

fn db(v: f32) -> f32 {
    10f32.powf(v / 20.0)
}

/// Keeps filter states out of the (slow) denormal range during tails.
const ANTI_DENORMAL: f32 = 1e-20;

// ── building blocks ─────────────────────────────────────────────────────

#[derive(Clone, Copy)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Biquad {
    fn new(b: [f64; 3], a: [f64; 3]) -> Self {
        let n = a[0];
        Self { b0: (b[0] / n) as f32, b1: (b[1] / n) as f32, b2: (b[2] / n) as f32, a1: (a[1] / n) as f32, a2: (a[2] / n) as f32, z1: 0.0, z2: 0.0 }
    }

    fn w(rate: u32, f: f64, q: f64) -> (f64, f64) {
        let f = f.min(rate as f64 * 0.45);
        let w = std::f64::consts::TAU * f / rate as f64;
        (w.cos(), w.sin() / (2.0 * q))
    }

    fn lowpass(rate: u32, f: f64, q: f64) -> Self {
        let (c, a) = Self::w(rate, f, q);
        Self::new([(1.0 - c) / 2.0, 1.0 - c, (1.0 - c) / 2.0], [1.0 + a, -2.0 * c, 1.0 - a])
    }

    fn highpass(rate: u32, f: f64, q: f64) -> Self {
        let (c, a) = Self::w(rate, f, q);
        Self::new([(1.0 + c) / 2.0, -(1.0 + c), (1.0 + c) / 2.0], [1.0 + a, -2.0 * c, 1.0 - a])
    }

    fn peak(rate: u32, f: f64, q: f64, gain_db: f64) -> Self {
        let (c, a) = Self::w(rate, f, q);
        let g = 10f64.powf(gain_db / 40.0);
        Self::new([1.0 + a * g, -2.0 * c, 1.0 - a * g], [1.0 + a / g, -2.0 * c, 1.0 - a / g])
    }

    #[inline]
    fn run(&mut self, x: f32) -> f32 {
        let x = x + ANTI_DENORMAL;
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

/// xorshift32: cheap noise, deterministic per chain.
struct Noise(u32);

impl Noise {
    #[inline]
    fn next(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x as f32 / u32::MAX as f32) * 2.0 - 1.0
    }

    fn unit(&mut self) -> f32 {
        self.next() * 0.5 + 0.5
    }
}

/// Feedback comb with a damped (low-passed) loop, as in Freeverb.
struct Comb {
    buf: Vec<f32>,
    pos: usize,
    store: f32,
    feedback: f32,
    damp: f32,
}

impl Comb {
    fn new(len: usize, feedback: f32, damp: f32) -> Self {
        Self { buf: vec![0.0; len.max(1)], pos: 0, store: 0.0, feedback, damp }
    }

    #[inline]
    fn run(&mut self, x: f32) -> f32 {
        let out = self.buf[self.pos];
        self.store = out * (1.0 - self.damp) + self.store * self.damp + ANTI_DENORMAL;
        self.buf[self.pos] = x + self.store * self.feedback;
        self.pos += 1;
        if self.pos == self.buf.len() {
            self.pos = 0;
        }
        out
    }
}

struct Allpass {
    buf: Vec<f32>,
    pos: usize,
}

impl Allpass {
    fn new(len: usize) -> Self {
        Self { buf: vec![0.0; len.max(1)], pos: 0 }
    }

    #[inline]
    fn run(&mut self, x: f32) -> f32 {
        let b = self.buf[self.pos];
        self.buf[self.pos] = x + b * 0.5 + ANTI_DENORMAL;
        self.pos += 1;
        if self.pos == self.buf.len() {
            self.pos = 0;
        }
        b - x
    }
}

/// A plain delay line read at a fixed or fractional distance.
struct Delay {
    buf: Vec<f32>,
    pos: usize,
}

impl Delay {
    fn new(len: usize) -> Self {
        Self { buf: vec![0.0; len.max(2)], pos: 0 }
    }

    #[inline]
    fn push(&mut self, x: f32) {
        self.buf[self.pos] = x;
        self.pos += 1;
        if self.pos == self.buf.len() {
            self.pos = 0;
        }
    }

    /// The sample written `back` pushes ago (1 = the last one), fractional.
    #[inline]
    fn read(&self, back: f32) -> f32 {
        let len = self.buf.len();
        let back = back.clamp(1.0, (len - 1) as f32);
        let i = back.floor();
        let frac = back - i;
        let at = |k: usize| self.buf[(self.pos + len - k) % len];
        let a = at(i as usize);
        let b = at(i as usize + 1);
        a + (b - a) * frac
    }
}

/// Freeverb (Jezar's public-domain design), mono.
struct Reverb {
    combs: Vec<Comb>,
    allpasses: Vec<Allpass>,
    pre: Delay,
    pre_len: f32,
    wet: f32,
    dry: f32,
}

impl Reverb {
    /// `room` and `damp` 0…1 like Freeverb's knobs, `pre_ms` pre-delay.
    fn new(rate: u32, room: f32, damp: f32, pre_ms: f32, wet: f32, dry: f32) -> Self {
        const COMBS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
        const ALLPASSES: [usize; 4] = [556, 441, 341, 225];
        let scale = rate as f32 / 44_100.0;
        let len = |n: usize| ((n as f32 * scale).round() as usize).max(1);
        let feedback = room * 0.28 + 0.7;
        let pre_len = (pre_ms / 1000.0 * rate as f32).max(1.0);
        Self {
            combs: COMBS.iter().map(|&n| Comb::new(len(n), feedback, damp * 0.4)).collect(),
            allpasses: ALLPASSES.iter().map(|&n| Allpass::new(len(n))).collect(),
            pre: Delay::new(pre_len as usize + 2),
            pre_len,
            wet,
            dry,
        }
    }

    #[inline]
    fn run(&mut self, x: f32) -> f32 {
        self.pre.push(x);
        let input = self.pre.read(self.pre_len) * 0.015;
        let mut out = 0.0;
        for c in &mut self.combs {
            out += c.run(input);
        }
        for a in &mut self.allpasses {
            out = a.run(out);
        }
        x * self.dry + out * self.wet * 3.0
    }
}

/// Little rising "bloop"s, for underwater.
struct Bubbles {
    rate: f32,
    noise: Noise,
    active: Vec<(f32, f32, f32, f32, u32)>, // phase, freq, amp, decay, samples left
    rise: f32,
    on: bool,
}

impl Bubbles {
    fn new(rate: u32) -> Self {
        let r = rate as f32;
        Self { rate: r, noise: Noise(0x9E37_79B9), active: Vec::with_capacity(4), rise: 2f32.powf(1.0 / (0.05 * r)), on: true }
    }

    #[inline]
    fn run(&mut self, x: f32) -> f32 {
        if self.on && self.active.len() < 4 && self.noise.unit() < 2.5 / self.rate {
            let f = 280.0 + self.noise.unit() * 600.0;
            let amp = 0.03 + self.noise.unit() * 0.04;
            self.active.push((0.0, f, amp, (-1.0 / (0.025 * self.rate)).exp(), (0.08 * self.rate) as u32));
        }
        let mut y = x;
        let rate = self.rate;
        let rise = self.rise;
        self.active.retain_mut(|(phase, f, amp, decay, left)| {
            y += phase.sin() * *amp;
            *phase += TAU * *f / rate;
            if *phase > TAU {
                *phase -= TAU;
            }
            *f *= rise;
            *amp *= *decay;
            *left -= 1;
            *left > 0
        });
        y
    }
}

/// Short-time Fourier transform with random phases: the voice keeps its
/// spectral envelope (what is said, the timbre) but loses its pitch, which
/// is what whispering is.
struct Whisper {
    n: usize,
    hop: usize,
    fft: Arc<dyn RealToComplex<f32>>,
    ifft: Arc<dyn ComplexToReal<f32>>,
    window: Vec<f32>,
    input: Vec<f32>,
    fill: usize,
    acc: Vec<f32>,
    frame: Vec<f32>,
    spectrum: Vec<Complex<f32>>,
    scratch: Vec<Complex<f32>>,
    phasors: Vec<Complex<f32>>,
    noise: Noise,
}

impl Whisper {
    fn new(rate: u32) -> Self {
        // ~11 ms frames, 75 % overlap
        let n = ((rate as f32 * 0.0107) as usize).next_power_of_two().max(64);
        let mut planner = RealFftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(n);
        let ifft = planner.plan_fft_inverse(n);
        let spectrum = fft.make_output_vec();
        let scratch = vec![Complex::default(); fft.get_scratch_len().max(ifft.get_scratch_len())];
        Self {
            n,
            hop: n / 4,
            window: (0..n).map(|i| (PI * i as f32 / n as f32).sin()).collect(),
            input: vec![0.0; n],
            fill: 0,
            acc: vec![0.0; n],
            frame: vec![0.0; n],
            spectrum,
            scratch,
            phasors: (0..1024).map(|i| Complex::from_polar(1.0, TAU * i as f32 / 1024.0)).collect(),
            noise: Noise(0x2545_F491),
            fft,
            ifft,
        }
    }

    /// Output lags the input by n − hop samples; `finish` flushes them.
    fn process(&mut self, x: &[f32], out: &mut Vec<f32>) {
        let (n, hop) = (self.n, self.hop);
        // sqrt-Hann in and out with hop n/4 sums to 2; the FFT pair scales by n
        let norm = 0.5 / n as f32;
        for &s in x {
            self.input[n - hop + self.fill] = s;
            self.fill += 1;
            if self.fill < hop {
                continue;
            }
            self.fill = 0;
            for ((f, i), w) in self.frame.iter_mut().zip(&self.input).zip(&self.window) {
                *f = i * w;
            }
            if self.fft.process_with_scratch(&mut self.frame, &mut self.spectrum, &mut self.scratch).is_ok() {
                let last = self.spectrum.len() - 1;
                for (k, c) in self.spectrum.iter_mut().enumerate() {
                    if k == 0 || k == last {
                        *c = Complex::default();
                        continue;
                    }
                    let i = (self.noise.0 >> 22) as usize;
                    self.noise.next();
                    *c = self.phasors[i] * c.norm();
                }
                if self.ifft.process_with_scratch(&mut self.spectrum, &mut self.frame, &mut self.scratch).is_ok() {
                    for ((a, f), w) in self.acc.iter_mut().zip(&self.frame).zip(&self.window) {
                        *a += f * w * norm;
                    }
                }
            }
            out.extend_from_slice(&self.acc[..hop]);
            self.acc.copy_within(hop.., 0);
            self.acc[n - hop..].fill(0.0);
            self.input.copy_within(hop.., 0);
        }
    }
}

// ── effect chains ───────────────────────────────────────────────────────

enum Stage {
    Filter(Biquad),
    /// tanh saturation, normalised so a full-scale peak stays full scale
    Drive(f32, f32),
    MuLaw,
    /// constant hiss at this amplitude
    Hiss(Noise, f32),
    /// ring modulator: phase, phase step, dry, ring
    Ring(f32, f32, f32, f32),
    /// metallic comb: delay, length, feedback
    Resonator(Vec<f32>, usize, f32),
    Echo(Delay, f32, f32, Biquad),
    /// modulated delay: line, phase, step, base, depth, wet
    Wobble(Delay, f32, f32, f32, f32, f32),
    Reverb(Box<Reverb>),
    Bubbles(Box<Bubbles>),
    Whisper(Box<Whisper>),
}

impl Stage {
    fn process(&mut self, buf: &mut Vec<f32>) {
        match self {
            Stage::Filter(f) => buf.iter_mut().for_each(|s| *s = f.run(*s)),
            Stage::Drive(k, norm) => buf.iter_mut().for_each(|s| *s = (*s * *k).tanh() * *norm),
            Stage::MuLaw => buf.iter_mut().for_each(|s| *s = mulaw8(*s)),
            Stage::Hiss(noise, amp) => buf.iter_mut().for_each(|s| *s += noise.next() * *amp),
            Stage::Ring(phase, step, dry, ring) => {
                for s in buf.iter_mut() {
                    *s = *s * *dry + *s * phase.sin() * *ring;
                    *phase += *step;
                    if *phase > TAU {
                        *phase -= TAU;
                    }
                }
            }
            Stage::Resonator(line, pos, fb) => {
                for s in buf.iter_mut() {
                    let y = *s + line[*pos] * *fb + ANTI_DENORMAL;
                    line[*pos] = y;
                    *pos = (*pos + 1) % line.len();
                    *s = y;
                }
            }
            Stage::Echo(line, back, gain, lp) => {
                for s in buf.iter_mut() {
                    let e = lp.run(line.read(*back));
                    line.push(*s + e * 0.35);
                    *s += e * *gain;
                }
            }
            Stage::Wobble(line, phase, step, base, depth, wet) => {
                for s in buf.iter_mut() {
                    line.push(*s);
                    let d = *base + *depth * phase.sin();
                    *phase += *step;
                    if *phase > TAU {
                        *phase -= TAU;
                    }
                    *s = *s * (1.0 - *wet) + line.read(d) * *wet;
                }
            }
            Stage::Reverb(r) => buf.iter_mut().for_each(|s| *s = r.run(*s)),
            Stage::Bubbles(b) => buf.iter_mut().for_each(|s| *s = b.run(*s)),
            Stage::Whisper(w) => {
                let mut out = Vec::with_capacity(buf.len() + w.hop);
                w.process(buf, &mut out);
                *buf = out;
            }
        }
    }
}

/// G.711-style 8-bit µ-law: the grainy sound of a phone line.
#[inline]
fn mulaw8(x: f32) -> f32 {
    const MU: f32 = 255.0;
    let ln_mu = (1.0 + MU).ln();
    let y = (1.0 + MU * x.abs().min(1.0)).ln() / ln_mu;
    let q = (y * 127.0).round() / 127.0;
    x.signum() * ((1.0 + MU).powf(q) - 1.0) / MU
}

fn drive(k: f32) -> Stage {
    Stage::Drive(k, 1.0 / k.tanh())
}

/// One message's effect: feed its pieces through [`Chain::process`] in
/// order, then take the tail from [`Chain::finish`].
pub struct Chain {
    pub fx: Fx,
    pub rate: u32,
    stages: Vec<Stage>,
    makeup: f32,
    /// Squelch noise: before the first piece and after the tail (walkie).
    squelch: Option<Noise>,
    started: bool,
    /// Longest tail and how much at least (seconds).
    tail: (f32, f32),
    latency: usize,
}

impl Chain {
    pub fn new(fx: Fx, rate: u32) -> Self {
        let r = rate;
        let lp = |f, q| Stage::Filter(Biquad::lowpass(r, f, q));
        let hp = |f, q| Stage::Filter(Biquad::highpass(r, f, q));
        let peak = |f, q, g| Stage::Filter(Biquad::peak(r, f, q, g));
        let ms = |v: f32| v / 1000.0 * r as f32;
        let mut latency = 0;
        let (stages, tail, squelch) = match fx {
            Fx::Radio => (vec![hp(180.0, 0.7), lp(4800.0, 0.7), lp(4800.0, 0.7), peak(1300.0, 0.8, 3.5), drive(2.2), Stage::Hiss(Noise(0x1234_5678), 0.0025)], (0.2, 0.05), None),
            Fx::Telephone => (vec![hp(320.0, 0.7), hp(320.0, 0.7), lp(3300.0, 0.7), lp(3300.0, 0.7), peak(1800.0, 1.0, 2.0), drive(1.6), Stage::MuLaw], (0.1, 0.02), None),
            Fx::Megaphone => (
                vec![hp(550.0, 0.7), hp(550.0, 0.7), lp(3800.0, 0.7), lp(3800.0, 0.7), peak(1700.0, 1.4, 7.0), drive(6.0), Stage::Echo(Delay::new(ms(140.0) as usize + 4), ms(110.0), 0.22, Biquad::lowpass(r, 2500.0, 0.7))],
                (0.6, 0.25),
                None,
            ),
            Fx::Robot => (
                vec![Stage::Ring(0.0, TAU * 70.0 / r as f32, 0.35, 0.9), Stage::Resonator(vec![0.0; ((r as f32 / 140.0) as usize).max(1)], 0, 0.5), hp(120.0, 0.7), lp(7000.0, 0.7)],
                (0.2, 0.05),
                None,
            ),
            Fx::Cave => (
                vec![
                    Stage::Echo(Delay::new(ms(260.0) as usize + 4), ms(230.0), 0.28, Biquad::lowpass(r, 1800.0, 0.7)),
                    Stage::Reverb(Box::new(Reverb::new(r, 0.84, 0.7, 35.0, 0.42, 0.8))),
                    lp(7000.0, 0.7),
                ],
                (3.0, 0.8),
                None,
            ),
            Fx::Hall => (vec![Stage::Reverb(Box::new(Reverb::new(r, 0.68, 0.35, 20.0, 0.3, 0.9)))], (2.2, 0.5), None),
            Fx::Walkie => (vec![hp(500.0, 0.7), hp(500.0, 0.7), lp(2700.0, 0.7), lp(2700.0, 0.7), peak(1500.0, 1.2, 4.0), drive(5.0), Stage::Hiss(Noise(0xDEAD_BEEF), 0.006)], (0.05, 0.0), Some(Noise(0xC0FF_EE11))),
            Fx::Underwater => (
                vec![
                    lp(560.0, 0.9),
                    lp(560.0, 0.9),
                    Stage::Wobble(Delay::new(ms(16.0) as usize + 4), 0.0, TAU * 0.8 / r as f32, ms(8.0), ms(3.0), 0.55),
                    Stage::Reverb(Box::new(Reverb::new(r, 0.05, 0.8, 10.0, 0.25, 0.9))),
                    Stage::Bubbles(Box::new(Bubbles::new(r))),
                ],
                (0.8, 0.3),
                None,
            ),
            Fx::Whisper => {
                let w = Whisper::new(r);
                latency = w.n;
                (vec![Stage::Whisper(Box::new(w)), hp(380.0, 0.7), peak(3200.0, 0.8, 4.0)], (0.05, 0.0), None)
            }
        };
        Self { fx, rate, stages, makeup: db(fx.makeup_db()), squelch, started: false, tail, latency }
    }

    /// Runs the next piece (with the voice's loudness correction `gain`).
    /// The result can be a little shorter or longer than the input: what an
    /// effect holds back comes out with the next piece or the tail.
    pub fn process(&mut self, mut samples: Vec<f32>, gain: f32) -> Vec<f32> {
        if gain != 1.0 {
            samples.iter_mut().for_each(|s| *s *= gain);
        }
        self.run(&mut samples);
        if !self.started {
            self.started = true;
            if let Some(n) = self.squelch.as_mut() {
                let mut intro = burst(n, self.rate, 0.035, 0.05);
                intro.extend_from_slice(&samples);
                return intro;
            }
        }
        samples
    }

    fn run(&mut self, buf: &mut Vec<f32>) {
        for s in &mut self.stages {
            s.process(buf);
        }
        let m = self.makeup;
        for s in buf.iter_mut() {
            *s = soft_clip(*s * m);
        }
    }

    /// The tail after the last piece: reverb decay, held-back samples,
    /// squelch. Ends with a short fade.
    pub fn finish(mut self) -> Vec<f32> {
        for s in &mut self.stages {
            if let Stage::Bubbles(b) = s {
                b.on = false;
            }
        }
        let rate = self.rate as f32;
        let block = (rate * 0.05) as usize;
        let (max, min) = self.tail;
        let max_len = (max * rate) as usize + self.latency;
        let min_len = (min * rate) as usize + self.latency;
        let mut out = Vec::new();
        while out.len() < max_len {
            let mut b = vec![0.0; block];
            self.run(&mut b);
            let quiet = b.iter().map(|s| s * s).sum::<f32>() / block.max(1) as f32 <= 1e-8;
            out.extend_from_slice(&b);
            if quiet && out.len() >= min_len {
                break;
            }
        }
        if let Some(n) = self.squelch.as_mut() {
            out.extend(burst(n, self.rate, 0.16, 0.12));
        }
        let fade = ((rate * 0.02) as usize).min(out.len());
        let len = out.len();
        for (i, s) in out[len - fade..].iter_mut().enumerate() {
            *s *= 1.0 - (i + 1) as f32 / fade as f32;
        }
        out
    }
}

/// Band-limited noise burst with a fast attack and decay (walkie squelch).
fn burst(n: &mut Noise, rate: u32, secs: f32, amp: f32) -> Vec<f32> {
    let len = (secs * rate as f32) as usize;
    let mut hp = Biquad::highpass(rate, 900.0, 0.7);
    let mut lp = Biquad::lowpass(rate, 4200.0, 0.7);
    (0..len)
        .map(|i| {
            let t = i as f32 / len as f32;
            let env = (t * 12.0).min(1.0) * (1.0 - t).powf(1.5);
            lp.run(hp.run(n.next())) * amp * env
        })
        .collect()
}

/// Transparent below 0.8, then rounds peaks off instead of clipping.
#[inline]
fn soft_clip(x: f32) -> f32 {
    let a = x.abs();
    if a <= 0.8 { x } else { x.signum() * (0.8 + 0.2 * ((a - 0.8) / 0.2).tanh()) }
}

/// Applies an effect to a whole clip at once (tail included).
pub fn apply(fx: Fx, samples: Vec<f32>, rate: u32, gain: f32) -> Vec<f32> {
    let mut c = Chain::new(fx, rate);
    let mut out = c.process(samples, gain);
    out.extend(c.finish());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A speech-like test signal: a gliding pitch with harmonics through
    /// three formants, in syllables with short pauses.
    fn speech(rate: u32, secs: f32) -> Vec<f32> {
        let n = (rate as f32 * secs) as usize;
        let mut f1 = Biquad::peak(rate, 700.0, 2.0, 12.0);
        let mut f2 = Biquad::peak(rate, 1200.0, 2.0, 10.0);
        let mut f3 = Biquad::peak(rate, 2600.0, 2.0, 8.0);
        let mut phase = 0f32;
        (0..n)
            .map(|i| {
                let t = i as f32 / rate as f32;
                let f0 = 140.0 + 30.0 * (t * 2.1).sin();
                phase = (phase + TAU * f0 / rate as f32) % TAU;
                let mut s = 0.0;
                for h in 1..30 {
                    s += (phase * h as f32).sin() / h as f32;
                }
                let syllable = (t * 4.0 * PI).sin().max(0.0);
                f3.run(f2.run(f1.run(s * 0.04 * syllable)))
            })
            .collect()
    }

    fn loud(x: &[f32]) -> f32 {
        let (mut sum, mut n) = (0f64, 0usize);
        for s in x.iter().filter(|s| s.abs() > 0.01) {
            sum += (*s as f64).powi(2);
            n += 1;
        }
        10.0 * (sum / n.max(1) as f64).log10() as f32
    }

    #[test]
    fn names_round_trip() {
        for f in ALL {
            assert_eq!(Fx::parse(f.name()), Some(f));
        }
        assert_eq!(Fx::parse(" Radio "), Some(Fx::Radio));
        assert_eq!(Fx::parse("vinyl"), None);
    }

    #[test]
    fn every_effect_is_finite_bounded_and_about_as_loud() {
        let rate = 24_000;
        let dry = speech(rate, 3.0);
        let dry_db = loud(&dry);
        for f in ALL {
            let wet = apply(f, dry.clone(), rate, 1.0);
            assert!(wet.iter().all(|s| s.is_finite() && s.abs() <= 1.0), "{f:?}");
            assert!(wet.len() >= dry.len(), "{f:?} lost audio: {} < {}", wet.len(), dry.len());
            assert!(wet.len() <= dry.len() + 3 * rate as usize + 2048, "{f:?} tail too long");
            let d = loud(&wet) - dry_db;
            // the makeup gains are measured on real voices; this synthetic
            // one only has to land in the same neighbourhood
            assert!(d.abs() < 6.0, "{f:?}: {d:+.1} dB");
            assert_eq!(*wet.last().unwrap(), 0.0, "{f:?} ends with a fade");
        }
    }

    #[test]
    fn pieces_join_seamlessly() {
        // the same audio in one piece or in three gives the same result
        let rate = 24_000;
        let dry = speech(rate, 2.0);
        for f in ALL {
            let whole = apply(f, dry.clone(), rate, 0.8);
            let mut c = Chain::new(f, rate);
            let mut parts = Vec::new();
            for chunk in [&dry[..10_000], &dry[10_000..10_037], &dry[10_037..]] {
                parts.extend(c.process(chunk.to_vec(), 0.8));
            }
            parts.extend(c.finish());
            assert_eq!(whole.len(), parts.len(), "{f:?}");
            let diff = whole.iter().zip(&parts).map(|(a, b)| (a - b).abs()).fold(0f32, f32::max);
            assert!(diff < 1e-5, "{f:?} differs by {diff}");
        }
    }

    #[test]
    fn telephone_removes_bass_and_highs() {
        let rate = 24_000;
        let tone = |f: f32| -> Vec<f32> { (0..rate).map(|i| (TAU * f * i as f32 / rate as f32).sin() * 0.3).collect() };
        let level = |x: Vec<f32>| {
            let w = apply(Fx::Telephone, x, rate, 1.0);
            let mid = &w[rate as usize / 4..rate as usize * 3 / 4];
            (mid.iter().map(|s| s * s).sum::<f32>() / mid.len() as f32).sqrt()
        };
        let (low, mid, high) = (level(tone(80.0)), level(tone(1000.0)), level(tone(8000.0)));
        assert!(low < mid * 0.1 && high < mid * 0.1, "80 Hz {low}, 1 kHz {mid}, 8 kHz {high}");
    }

    #[test]
    fn whisper_has_no_pitch() {
        // a pure 200 Hz tone: whispered, its energy at 200 Hz drops a lot
        let rate = 24_000;
        let tone: Vec<f32> = (0..rate * 2).map(|i| (TAU * 200.0 * i as f32 / rate as f32).sin() * 0.3).collect();
        let w = apply(Fx::Whisper, tone, rate, 1.0);
        let at = |x: &[f32], f: f32| {
            let (mut re, mut im) = (0f32, 0f32);
            for (i, s) in x.iter().enumerate() {
                let p = TAU * f * i as f32 / rate as f32;
                re += s * p.cos();
                im += s * p.sin();
            }
            (re * re + im * im).sqrt() / x.len() as f32
        };
        let seg = &w[rate as usize / 2..rate as usize * 3 / 2];
        let total = (seg.iter().map(|s| s * s).sum::<f32>() / seg.len() as f32).sqrt();
        assert!(at(seg, 200.0) < total * 0.2, "200 Hz {} of rms {total}", at(seg, 200.0));
    }

    #[test]
    fn costs_little() {
        // 60 s of speech through every effect; prints the share of real time
        let rate = 24_000;
        let dry = speech(rate, 60.0);
        for f in ALL {
            let t = std::time::Instant::now();
            let out = apply(f, dry.clone(), rate, 1.0);
            let secs = t.elapsed().as_secs_f32();
            println!("{:<10} {:>6.1} ms per minute of speech ({:.3} % of real time)", f.name(), secs * 1000.0, secs / 60.0 * 100.0);
            assert!(out.len() > dry.len());
            // generous for debug builds on slow CI machines
            assert!(secs < 6.0, "{f:?} took {secs} s");
        }
    }
}

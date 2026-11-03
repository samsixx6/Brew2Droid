// SPDX-License-Identifier: Apache-2.0
//
// ma_fm_core.cpp -- the fm dsp. phase generator, exponential adsr, operators,
//                   2-op/4-op algorithms, and a compact built-in gm patch bank.
// ----------------------------------------------------------------------------
// the chip did all of this in fixed point with log/exp lookup roms. we do it in
// double, which is both simpler and cleaner than the original, and nobody's ear
// on a ringtone will miss the last bit of the exp table. what matters is that
// the ENVELOPE SHAPE and the OPERATOR ALGEBRA match: fm timbre lives in the
// modulator/carrier ratio and the attack/decay curve, not in the mantissa.

#include "ma_fm_core.h"

#include <cmath>
#include <algorithm>

namespace fxchain::smaf {

namespace {
constexpr double kTwoPi = 6.283185307179586;

// total-level (attenuation) -> linear gain. the chip steps 0.75 dB per tl unit,
// 63 units ~= -48 dB ~= silence. classic OPL scale.
inline double tlToGain(uint8_t tl) {
    if (tl >= 63) return 0.0;
    return std::pow(10.0, (-0.75 * double(tl)) / 20.0);
}

// a chip "rate" (0..15) mapped to a per-sample linear step for the envelope.
// higher rate = faster. rate 0 = effectively frozen, 15 = near-instant. the
// curve is tuned by ear against real ringtones: fast attacks, musical decays.
inline double rateToStep(uint8_t rate, double sampleRate, bool attack) {
    if (rate == 0) {
        // rate 0: an attack completes instantly (the caller floors it), but a
        // decay/sustain/release with a zero step would freeze the level and the
        // voice would never reach Idle (a stuck drone). floor the non-attack
        // step to a slow ~20 s glide so every voice eventually retires.
        return attack ? 0.0 : (1.0 / (sampleRate * 20.0));
    }
    // time constant in seconds for the fastest (15) vs slowest (1) rate.
    // attacks are quicker than decays on this family.
    const double fast = attack ? 0.0008 : 0.004;   // rate 15
    const double slow = attack ? 0.35   : 4.0;      // rate 1
    double t = slow * std::pow(fast / slow, (double(rate) - 1.0) / 14.0);
    double samples = std::max(1.0, t * sampleRate);
    return 1.0 / samples;
}
} // namespace

// ── FmVoicePatch ─────────────────────────────────────────────────────────────
FmVoicePatch FmVoicePatch::defaultPatch() {
    FmVoicePatch v;
    v.fourOp = false;
    v.algorithm = 0;   // op0 modulates op1 (carrier)
    v.feedback = 0;
    // op0 modulator: bright-ish ratio 2, quick decay
    v.ops[0] = FmOpPatch{ /*multi*/2, /*tl*/20, /*ar*/15, /*dr*/6, /*sr*/2,
                          /*rr*/7, /*sl*/4, /*ksl*/0, /*ksr*/0, /*wave*/0,
                          /*dt*/0, /*fb*/0, /*am*/false, /*vib*/false, /*egType*/true };
    // op1 carrier: full level, sustaining
    v.ops[1] = FmOpPatch{ /*multi*/1, /*tl*/0,  /*ar*/15, /*dr*/4, /*sr*/1,
                          /*rr*/7, /*sl*/2, /*ksl*/0, /*ksr*/0, /*wave*/0,
                          /*dt*/0, /*fb*/0, /*am*/false, /*vib*/false, /*egType*/true };
    return v;
}

const FmVoicePatch& FmVoicePatch::gmApprox(int program) {
    // a small, deterministic, rom-free approximation of the gm map. we key a
    // handful of timbral families off the gm program ranges and reuse a base
    // patch with tweaked modulator ratio / decay. it is intentionally compact:
    // the goal is "sounds like the right KIND of instrument", not sample-accuracy.
    //
    // built via a c++ magic static (thread-safe once-init): two decoders CAN
    // hit this concurrently (a SmartScan probe + a playback open), and a plain
    // `static bool built` flag would race.
    static const std::array<FmVoicePatch, 128> bank = [] {
        std::array<FmVoicePatch, 128> b{};
        for (int i = 0; i < 128; ++i) {
            FmVoicePatch v = FmVoicePatch::defaultPatch();
            int fam = i / 8;                 // 16 gm families
            switch (fam) {
                case 0:  v.ops[0].multi=1; v.ops[0].dr=7; break;      // pianos
                case 1:  v.ops[0].multi=2; v.ops[0].tl=14; break;     // chromatic perc
                case 2:  v.ops[0].multi=1; v.ops[1].sr=0; v.ops[0].dr=2; break; // organs (sustain)
                case 3:  v.ops[0].multi=1; v.ops[0].dr=5; break;      // guitars
                case 4:  v.ops[0].multi=2; v.ops[0].dr=4; break;      // basses
                case 5:  v.ops[0].multi=1; v.ops[1].sr=0; v.ops[0].dr=1; break; // strings
                case 6:  v.ops[0].multi=1; v.ops[1].sr=0; break;      // ensemble
                case 7:  v.ops[0].multi=3; v.ops[1].sr=0; break;      // brass
                case 8:  v.ops[0].multi=1; v.ops[1].sr=0; v.ops[0].tl=26; break; // reed
                case 9:  v.ops[0].multi=1; v.ops[1].sr=0; v.ops[0].tl=28; break; // pipe
                case 10: v.ops[0].multi=4; v.ops[0].wave=1; break;    // synth lead
                case 11: v.ops[0].multi=1; v.ops[0].wave=2; v.ops[1].sr=0; break; // synth pad
                case 12: v.ops[0].multi=6; v.ops[0].wave=3; break;    // synth fx
                case 13: v.ops[0].multi=2; v.ops[0].dr=6; break;      // ethnic
                case 14: v.ops[0].multi=8; v.ops[0].dr=10; v.ops[0].wave=1; break; // percussive
                default: v.ops[0].multi=12; v.ops[0].wave=4; v.ops[0].dr=12; break; // sfx
            }
            b[i] = v;
        }
        return b;
    }();
    if (program < 0) program = 0;
    if (program > 127) program = 127;
    return bank[program];
}

const FmVoicePatch& FmVoicePatch::drumApprox(int note) {
    // three short percussive hits, all egType=false so they retire on their own.
    static const std::array<FmVoicePatch, 3> kit = [] {
        std::array<FmVoicePatch, 3> k{};
        // kick-ish: sub carrier, fast pitch-less thump.
        k[0] = FmVoicePatch::defaultPatch();
        k[0].ops[0] = FmOpPatch{ /*multi*/0, /*tl*/8,  /*ar*/15, /*dr*/10, /*sr*/0,
                                 /*rr*/12, /*sl*/15, 0,0, /*wave*/0, 0, 0, false, false, false };
        k[0].ops[1] = FmOpPatch{ /*multi*/0, /*tl*/0,  /*ar*/15, /*dr*/9,  /*sr*/0,
                                 /*rr*/12, /*sl*/15, 0,0, /*wave*/0, 0, /*fb*/5, false, false, false };
        // snare-ish: mid noise burst (high-multi modulator, heavy feedback).
        k[1] = FmVoicePatch::defaultPatch();
        k[1].ops[0] = FmOpPatch{ /*multi*/11, /*tl*/4, /*ar*/15, /*dr*/9,  /*sr*/0,
                                 /*rr*/11, /*sl*/15, 0,0, /*wave*/0, 0, /*fb*/7, false, false, false };
        k[1].ops[1] = FmOpPatch{ /*multi*/1,  /*tl*/2, /*ar*/15, /*dr*/8,  /*sr*/0,
                                 /*rr*/11, /*sl*/15, 0,0, /*wave*/0, 0, 0, false, false, false };
        // hat-ish: short metallic tick.
        k[2] = FmVoicePatch::defaultPatch();
        k[2].ops[0] = FmOpPatch{ /*multi*/15, /*tl*/6, /*ar*/15, /*dr*/12, /*sr*/0,
                                 /*rr*/13, /*sl*/15, 0,0, /*wave*/0, 0, /*fb*/7, false, false, false };
        k[2].ops[1] = FmOpPatch{ /*multi*/9,  /*tl*/8, /*ar*/15, /*dr*/12, /*sr*/0,
                                 /*rr*/13, /*sl*/15, 0,0, /*wave*/0, 0, 0, false, false, false };
        return k;
    }();
    return kit[note < 44 ? 0 : note < 52 ? 1 : 2];
}

// ── FmEnvelope ───────────────────────────────────────────────────────────────
void FmEnvelope::configure(const FmOpPatch& p, double sampleRate, double rateScale) {
    atkStep_ = rateToStep(p.ar, sampleRate, true)  * rateScale;
    decStep_ = rateToStep(p.dr, sampleRate, false) * rateScale;
    susStep_ = rateToStep(p.sr, sampleRate, false) * rateScale;
    relStep_ = rateToStep(p.rr, sampleRate, false) * rateScale;
    susLevel_ = 1.0 - (double(p.sl) / 15.0);   // sl 0 = top, 15 = ~silent
    sustaining_ = p.egType;
    xof_ = p.xof;
    phase_ = Phase::Idle;
    level_ = 0.0;
}

void FmEnvelope::keyOn() { phase_ = Phase::Attack; }

void FmEnvelope::keyOff() {
    // XOF: the note ignores key-off and rings out on its own envelope. only
    // honoured for percussive shapes; a sustaining XOF voice would never retire.
    if (xof_ && !sustaining_) return;
    if (phase_ != Phase::Idle) phase_ = Phase::Release;
}

float FmEnvelope::advance() {
    switch (phase_) {
        case Phase::Idle: return 0.0f;
        case Phase::Attack:
            level_ += (atkStep_ <= 0.0 ? 1.0 : atkStep_);
            if (level_ >= 1.0) { level_ = 1.0; phase_ = Phase::Decay; }
            break;
        case Phase::Decay:
            level_ -= decStep_;
            if (level_ <= susLevel_) {
                level_ = susLevel_;
                phase_ = sustaining_ ? Phase::Sustain : Phase::Release;
            }
            break;
        case Phase::Sustain:
            // second-decay (sr): a slow bleed toward silence, chip behaviour.
            level_ -= susStep_;
            if (level_ <= 0.0) { level_ = 0.0; phase_ = Phase::Idle; }
            break;
        case Phase::Release:
            level_ -= relStep_;
            if (level_ <= 0.0) { level_ = 0.0; phase_ = Phase::Idle; }
            break;
    }
    return float(level_);
}

// ── FmOperator ───────────────────────────────────────────────────────────────
void FmOperator::configure(const FmOpPatch& p, double sampleRate, double rateScale) {
    patch_ = p;
    sampleRate_ = sampleRate;
    wave_ = p.wave;
    tlGain_ = tlToGain(p.tl);
    // vibrato depth (DVB 0..3) as a phase-inc factor: ~3.4/6.7/13.4/26.8 cents,
    // the MA/YMF825 depth family. tremolo depth (DAM 0..3) as a linear gain
    // span: ~1.2/2.4/4.8/9.6 dB dips. both only engage when VIB/AM are set.
    static constexpr double kVib[4] = { 0.00196, 0.00387, 0.00774, 0.01548 };
    static constexpr float  kTrem[4] = { 0.129f, 0.242f, 0.424f, 0.669f };
    vibAmt_ = p.vib ? kVib[p.dvb & 3] : 0.0;
    amAmt_  = p.am  ? kTrem[p.dam & 3] : 0.0f;
    env_.configure(p, sampleRate, rateScale);
}

void FmOperator::recalc_() {
    // MA/YMF825 frequency multiple: value 0 => x0.5, value n(1..15) => x n.
    // LINEAR, not the OPL {..10,10,12,12,15,15} doubling table (that gave every
    // high-MULTI voice the wrong operator ratio, a big source of the harshness).
    double m = (patch_.multi == 0) ? 0.5 : double(patch_.multi & 15);
    double detune = 1.0 + (double(patch_.dt) - 3.5) * 0.0006; // subtle
    phaseInc_ = (freqHz_ * m * detune) / sampleRate_;
    // key-scale level: attenuate as the note rises above middle C, 0/1.5/3/6 dB
    // per octave by KSL (high notes thin out on the chip instead of blaring).
    static constexpr double kKslDbPerOct[4] = { 0.0, 1.5, 3.0, 6.0 };
    double oct = std::log2(std::max(freqHz_, 1.0) / 261.63);
    double att = kKslDbPerOct[patch_.ksl & 3] * std::max(0.0, oct);
    kslGain_ = std::pow(10.0, -att / 20.0);
    // nyquist guard: a high note through a high MULTI (percussion patches use
    // 14/15) can land the operator above fs/2; mute it instead of splattering
    // alias noise across the band.
    nyMute_ = (phaseInc_ >= 0.5);
}

void FmOperator::noteOn(double freqHz) {
    freqHz_ = freqHz;
    phase_ = 0.0;
    recalc_();
    env_.keyOn();
}

void FmOperator::noteOff() { env_.keyOff(); }

float FmOperator::waveform_(uint8_t wave, double phase01) {
    // the OPL/YMF825 waveform family, all sine-derived (that is the whole point
    // of the chip's warm character). WS 0..7 are the OPL3 set; the MA/SD-1 higher
    // shapes (8..30) are softer derivatives, approximated here as the OPL set on
    // the low 3 bits until the exact SD-1 rom is dumped. squares/saws are avoided
    // as defaults so an out-of-range WS never turns into a harsh buzz.
    double x = phase01 - std::floor(phase01);
    double s = std::sin(kTwoPi * x);
    switch (wave & 7) {
        case 0: return float(s);                                  // sine
        case 1: return float(s > 0 ? s : 0.0);                    // half sine
        case 2: return float(std::fabs(s));                       // abs sine (rectified)
        case 3: {                                                 // quarter sine
            double q = x - std::floor(x * 2.0) * 0.5;             // fold to [0,0.5)
            return float(q < 0.25 ? std::fabs(std::sin(kTwoPi * q)) : 0.0);
        }
        case 4: return float(x < 0.5 ? std::sin(kTwoPi * 2.0 * x) : 0.0);          // even sine
        case 5: return float(x < 0.5 ? std::fabs(std::sin(kTwoPi * 2.0 * x)) : 0.0); // even abs
        case 6: {                                                 // soft "square":
            // a rounded pulse (tanh-shaped) instead of a hard +-1 edge; keeps the
            // hollow character without the top-octave buzz of an ideal square.
            return float(std::tanh(s * 3.0));
        }
        default: {                                                // soft saw:
            // band-limited-ish ramp built from the first few sine partials, far
            // less aliased than a raw 2x-1 ramp.
            double v = std::sin(kTwoPi*x) + 0.5*std::sin(kTwoPi*2*x) + 0.333*std::sin(kTwoPi*3*x);
            return float(v * 0.6);
        }
    }
}

float FmOperator::tick(double modCycles, double lfoSin) {
    // modCycles: phase modulation input in CYCLES (0..1 = one period), added
    // straight to the phase. the caller scales the modulator output into a
    // musical index, so this stays simple. lfoSin drives this operator's own
    // vibrato (phase-inc scale) and tremolo (gain dip) when VIB/AM are set.
    float envGain = env_.advance();
    if (nyMute_) return 0.0f;      // above fs/2: silent, never alias
    double inc = phaseInc_;
    if (vibAmt_ != 0.0) inc *= 1.0 + vibAmt_ * lfoSin;
    phase_ += inc;
    if (phase_ >= 1.0) phase_ -= std::floor(phase_);
    double out = waveform_(wave_, phase_ + modCycles);
    float amGain = (amAmt_ != 0.0f) ? 1.0f - amAmt_ * float(0.5 + 0.5 * lfoSin) : 1.0f;
    return float(out * tlGain_ * kslGain_ * envGain * amGain);
}

// ── FmVoice ──────────────────────────────────────────────────────────────────
namespace {
// how deep a modulator drives a carrier's phase. a full-scale modulator (+-1)
// shifts the carrier by +-kModDepth cycles. the old code used 1.0 (a whole
// period, index 2*pi) which is why everything screamed; ~0.42 cycles (index
// ~2.6 rad) is the musical FM range yamaha voices sit in and sounds warm.
constexpr double kModDepth = 0.42;
}

void FmVoice::noteOn(const FmVoicePatch& patch, double freqHz, float velocity) {
    patch_ = patch;
    fourOp_ = patch.fourOp;
    algo_ = patch.algorithm & 7;
    feedback_ = patch.feedback;
    velocity_ = std::clamp(velocity, 0.0f, 1.0f);
    for (int i = 0; i < 4; ++i) { fbMem_[i][0] = fbMem_[i][1] = 0.0; opFb_[i] = patch.ops[i].fb; }
    // VMA/2-op voices carry feedback only in the voice header; honour it on op0.
    if (patch.feedback && !opFb_[0]) opFb_[0] = patch.feedback;
    // one LFO per voice, rate selected by the patch (the chip's four speeds).
    static constexpr double kLfoHz[4] = { 1.8, 4.0, 6.0, 9.7 };
    lfoInc_ = kLfoHz[patch.lfo & 3] / sampleRate_;
    lfoPhase_ = 0.0; lfoSin_ = 0.0;
    // KSR key-scaling: envelopes speed up as the note rises (strong when the
    // op sets KSR, mild otherwise, like the chip's two scaling curves).
    double semisUp = 12.0 * std::log2(std::max(freqHz, 1.0) / 261.63);
    int nops = fourOp_ ? 4 : 2;
    for (int i = 0; i < nops; ++i) {
        double rs = std::pow(2.0, semisUp / (patch.ops[i].ksr ? 24.0 : 96.0));
        ops_[i].configure(patch.ops[i], sampleRate_, std::clamp(rs, 0.5, 4.0));
        ops_[i].noteOn(freqHz);
    }
    active_ = true;
}

void FmVoice::noteOff() {
    int nops = fourOp_ ? 4 : 2;
    for (int i = 0; i < nops; ++i) ops_[i].noteOff();
}

void FmVoice::setPitch(double freqHz) {
    int nops = fourOp_ ? 4 : 2;
    for (int i = 0; i < nops; ++i) ops_[i].setBaseFreq(freqHz);
}

// run operator i with its own feedback added to the external phase-mod input.
float FmVoice::modOp_(int i, double extModCycles) {
    double fbc = 0.0;
    if (opFb_[i] > 0) {
        // feedback in cycles, averaged over the last two outputs (OPL trick to
        // damp the self-oscillation), scaled gently by the 0..7 amount.
        double avg = (fbMem_[i][0] + fbMem_[i][1]) * 0.5;
        fbc = avg * (double(opFb_[i]) / 24.0);
    }
    float o = ops_[i].tick(extModCycles + fbc, lfoSin_);
    fbMem_[i][1] = fbMem_[i][0]; fbMem_[i][0] = o;
    return o;
}

float FmVoice::tick() {
    if (!active_) return 0.0f;
    // advance the voice LFO once; every VIB/AM-enabled operator reads it.
    lfoPhase_ += lfoInc_;
    if (lfoPhase_ >= 1.0) lfoPhase_ -= 1.0;
    lfoSin_ = std::sin(2.0 * 3.14159265358979 * lfoPhase_);
    const double d = kModDepth;
    double out = 0.0;

    // the real MA/YMF825 connection algorithms (enums/voice.go). op indices 0..3
    // = chip ops 1..4. carriers are summed; "a->b" = a modulates b's phase.
    switch (algo_) {
        case 0: {                                   // FB(1)->2        [2-op]
            double m = modOp_(0, 0.0);
            out = modOp_(1, m * d);
        } break;
        case 1: {                                   // FB(1) + 2       [2-op]
            double a = modOp_(0, 0.0);
            double b = modOp_(1, 0.0);
            out = (a + b) * 0.5;
        } break;
        case 2: {                                   // FB(1)+2+FB(3)+4 (all direct)
            double a = modOp_(0, 0.0);
            double b = modOp_(1, 0.0);
            double c = modOp_(2, 0.0);
            double e = modOp_(3, 0.0);
            out = (a + b + c + e) * 0.35;
        } break;
        case 3: {                                   // (FB(1)+2->3)->4
            double a = modOp_(0, 0.0);
            double b = modOp_(1, 0.0);
            double c = modOp_(2, (a + b) * d);
            out = modOp_(3, c * d);
        } break;
        case 4: {                                   // FB(1)->2->3->4 (serial)
            double a = modOp_(0, 0.0);
            double b = modOp_(1, a * d);
            double c = modOp_(2, b * d);
            out = modOp_(3, c * d);
        } break;
        case 5: {                                   // FB(1)->2 + FB(3)->4
            double a = modOp_(0, 0.0);
            double b = modOp_(1, a * d);
            double c = modOp_(2, 0.0);
            double e = modOp_(3, c * d);
            out = (b + e) * 0.5;
        } break;
        case 6: {                                   // FB(1) + 2->3->4
            double a = modOp_(0, 0.0);
            double b = modOp_(1, 0.0);
            double c = modOp_(2, b * d);
            double e = modOp_(3, c * d);
            out = (a + e) * 0.5;
        } break;
        default: {                                  // 7: FB(1) + 2->3 + 4
            double a = modOp_(0, 0.0);
            double b = modOp_(1, 0.0);
            double c = modOp_(2, b * d);
            double e = modOp_(3, 0.0);
            out = (a + c + e) * 0.4;
        } break;
    }

    // all operators finished -> voice is done.
    bool anyLive = false;
    int nops = fourOp_ ? 4 : 2;
    for (int i = 0; i < nops; ++i) if (!ops_[i].finished()) { anyLive = true; break; }
    if (!anyLive) active_ = false;

    return float(out * velocity_ * volume_ * 0.7);
}

} // namespace fxchain::smaf

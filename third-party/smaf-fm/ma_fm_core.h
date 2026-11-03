// SPDX-License-Identifier: Apache-2.0
#pragma once
//
// ma_fm_core.h -- the fm voice engine of the yamaha MA-series ringtone chips.
// ----------------------------------------------------------------------------
// the MA-3/MA-5 sound source is a close cousin of the classic OPL/OPN yamaha
// fm line: sine-family operators, an exponential envelope generator, 2-op and
// 4-op voices with a small set of connection algorithms. this file is the pure
// dsp: phase generator + envelope generator + operator + voice, all fixed-point
// where the chip was and float where we can afford to be. no ringtone-format
// knowledge lives here, so it unit-tests as "give me a patch + a note, hand me
// samples".
//
// ROM-FREE BY DESIGN. the chips also had a small gm sample rom for the "voices"
// a ringtone could reference by program number. that rom is not public and we
// do not ship it. instead a compact built-in fm patch set approximates the gm
// map, so a file that asks for "program 0" gets a plausible piano-ish fm voice
// rather than silence. it will not be bit-identical to a 2003 handset, and that
// is fine and honest: recognised, and it plays.
//
// references read as documentation (no code copied): the public OPL/OPLL
// register descriptions, the yamaha ymf825 (sd-1) application notes for a
// modern same-family part, and umjammer/vavi-sound for behaviour.

#include <cstdint>
#include <array>
#include <vector>

namespace fxchain::smaf {

// ── one operator's patch ────────────────────────────────────────────────────
// values are in the chip's own units (0..15 rates, 0..63 tl, ...) so the
// sequencer can drop register bytes straight in. the core converts to internal
// scales at note-on.
struct FmOpPatch {
    uint8_t multi   = 1;   // frequency multiple (0..15; 0 = x0.5)
    uint8_t tl      = 0;   // total level / attenuation (0..63, 0 = loudest)
    uint8_t ar      = 15;  // attack rate  (0..15)
    uint8_t dr      = 0;   // decay rate   (0..15)
    uint8_t sr      = 0;   // sustain rate (0..15)  (aka second decay)
    uint8_t rr      = 7;   // release rate (0..15)
    uint8_t sl      = 0;   // sustain level(0..15)
    uint8_t ksl     = 0;   // key-scale level (0..3)
    uint8_t ksr     = 0;   // key-scale rate  (0..1)
    uint8_t wave    = 0;   // waveform select (0..31; the YMF825/OPL wave set)
    uint8_t dt      = 0;   // detune (0..7)
    uint8_t fb      = 0;   // per-operator self-feedback (0..7)
    bool    am      = false; // amplitude modulation (tremolo) enable
    bool    vib     = false; // vibrato enable
    bool    egType  = true;  // true = sustaining (hold at SL), false = percussive
    // appended after the 15 positional fields so existing brace-inits stay valid.
    uint8_t dvb     = 0;     // vibrato depth (0..3)
    uint8_t dam     = 0;     // tremolo depth (0..3)
    bool    xof     = false; // ignore key-off (drums ring out fully)
};

// ── a voice patch: 2-op or 4-op, one connection algorithm ───────────────────
struct FmVoicePatch {
    bool     fourOp   = false;      // false = 2-op, true = 4-op
    uint8_t  algorithm = 0;         // connection algo (0..1 for 2-op, 0..7 4-op)
    uint8_t  feedback  = 0;         // op1 self-feedback (0..7)
    int      noteShift = 0;         // basic-octave transpose in semitones (BO)
    float    panDefault = 0.0f;     // patch panpot default, -1..+1 (0 = centre)
    uint8_t  lfo = 0;               // voice LFO rate select (0..3)
    std::array<FmOpPatch, 4> ops{}; // op[0..1] used for 2-op, all four for 4-op
    // a simple, safe default patch: single carrier sine, medium decay.
    static FmVoicePatch defaultPatch();
    // the built-in gm-approximation bank, indexed 0..127 by program number.
    static const FmVoicePatch& gmApprox(int program);
    // percussion fallback for rhythm-channel notes with no bound voice: a short
    // fm hit picked by register (low = kick-ish, mid = snare-ish, high = hat-ish)
    // so drums never fall back to a melodic piano.
    static const FmVoicePatch& drumApprox(int note);
};

// ── envelope generator (adsr, exponential, chip-style discrete rates) ───────
class FmEnvelope {
public:
    // rateScale > 1 speeds every stage up (KSR key-scaling: high notes decay
    // faster on the chip).
    void configure(const FmOpPatch& p, double sampleRate, double rateScale = 1.0);
    void keyOn();
    void keyOff();
    float advance();                 // returns linear gain 0..1 for this sample
    bool  isFinished() const { return phase_ == Phase::Idle; }
private:
    enum class Phase { Idle, Attack, Decay, Sustain, Release };
    Phase phase_ = Phase::Idle;
    double level_ = 0.0;             // 0 (silent) .. 1 (peak), linear
    double atkStep_ = 0.0, decStep_ = 0.0, susStep_ = 0.0, relStep_ = 0.0;
    double susLevel_ = 0.0;
    bool   sustaining_ = true;
    bool   xof_ = false;             // ignore key-off (drums ring out fully)
};

// ── operator: phase generator + waveform + envelope ─────────────────────────
class FmOperator {
public:
    void configure(const FmOpPatch& p, double sampleRate, double rateScale = 1.0);
    void noteOn(double freqHz);
    void noteOff();
    // modIn is the phase-modulation input in cycles (0 for a pure carrier).
    // lfoSin is the voice LFO value (-1..1); the operator applies its own
    // vibrato/tremolo depth from the patch when VIB/AM are enabled.
    // returns the operator output in roughly [-1, 1] scaled by tl + envelope.
    float tick(double modIn, double lfoSin = 0.0);
    bool  finished() const { return env_.isFinished(); }
    void  setBaseFreq(double f) { freqHz_ = f; recalc_(); }
private:
    void recalc_();
    FmOpPatch  patch_{};
    FmEnvelope env_{};
    double sampleRate_ = 48000.0;
    double freqHz_     = 440.0;
    double phase_      = 0.0;   // 0..1
    double phaseInc_   = 0.0;
    double tlGain_     = 1.0;   // linear gain from total-level
    double kslGain_    = 1.0;   // key-scale level attenuation (freq-dependent)
    double vibAmt_     = 0.0;   // vibrato depth as a phase-inc factor
    float  amAmt_      = 0.0f;  // tremolo depth as a linear gain span
    bool   nyMute_     = false; // operator lands above fs/2 -> silent, no alias
    uint8_t wave_      = 0;
    static float waveform_(uint8_t wave, double phase01);
};

// ── a playing voice ─────────────────────────────────────────────────────────
class FmVoice {
public:
    void setSampleRate(double sr) { sampleRate_ = sr; }
    void noteOn(const FmVoicePatch& patch, double freqHz, float velocity);
    void noteOff();
    void setPitch(double freqHz);         // pitch bend / portamento
    void setVolume(float v) { volume_ = v; }   // channel volume 0..1
    float tick();                          // one mono sample, ~[-1,1]
    bool  active() const { return active_; }
    int   note() const { return note_; }
    int   channel = -1;                    // owning smaf channel (for stealing)
    int   note_   = -1;
private:
    FmVoicePatch patch_{};
    std::array<FmOperator, 4> ops_{};
    double sampleRate_ = 48000.0;
    float  velocity_ = 1.0f;
    float  volume_   = 1.0f;
    bool   active_   = false;
    bool   fourOp_   = false;
    uint8_t algo_    = 0;
    uint8_t feedback_= 0;
    uint8_t opFb_[4] = {0,0,0,0};          // per-operator feedback amount (0..7)
    double  fbMem_[4][2] = {{0,0},{0,0},{0,0},{0,0}};  // per-op feedback history
    double  lfoPhase_ = 0.0, lfoInc_ = 0.0;            // one LFO per voice
    double  lfoSin_   = 0.0;                           // this sample's LFO value
    // one modulator step: run op[i] with its own feedback + an external phase-mod
    // input (in cycles). returns its output. keeps the FM index musical.
    float modOp_(int i, double extModCycles);
};

} // namespace fxchain::smaf

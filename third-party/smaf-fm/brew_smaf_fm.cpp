// Fronteira local do Brew2Droid; o DSP Apache-2.0 permanece sem alterações.
#include "ma_fm_core.h"
#include "smaf_voice.h"
#include <algorithm>
#include <array>
#include <cmath>
#include <cstdint>
#include <new>
#include <vector>

namespace {
using namespace fxchain::smaf;
struct Channel { float gain, left, right; };
struct Definition { uint8_t bank, program; FmVoicePatch patch; };
struct Slot {
    FmVoice voice;
    uint32_t token = 0, channel = 0;
    uint64_t order = 0;
    bool live = false;
};
struct Engine {
    uint32_t rate;
    uint64_t order = 0;
    std::array<Slot, 48> slots{};
    std::vector<Definition> patches;
    explicit Engine(uint32_t value) : rate(value) {}
};
bool patch_size(const uint8_t* p, size_t n) {
    // O byte2 privado não participa do endereço Bank/PC em parseVoiceExclusive.
    if (!p || n < 7 || p[0] != 0x43 || p[1] != 3 || p[4] > 127 || p[6] != 1) return false;
    return n == 7u + 5u * ((p[5] & 7) < 2 ? 2u : 4u);
}
}

extern "C" {
void* brew_smaf_fm_new(uint32_t rate) noexcept {
    if (rate < 8000 || rate > 48000) return nullptr;
    try { return new Engine(rate); } catch (...) { return nullptr; }
}
void brew_smaf_fm_delete(void* value) noexcept { delete static_cast<Engine*>(value); }
uint32_t brew_smaf_fm_patch(void* value, const uint8_t* p, size_t n) noexcept {
    if (!value || !patch_size(p, n)) return 0;
    try {
        auto& engine = *static_cast<Engine*>(value);
        const auto voice = parseVoiceExclusive(p, n);
        if (!voice.valid || voice.isPcm) return 0;
        for (auto& definition : engine.patches) {
            if (definition.bank == p[3] && definition.program == p[4]) {
                definition.patch = voice.patch;
                return 1;
            }
        }
        if (engine.patches.size() >= 256) return 0;
        engine.patches.push_back({p[3], p[4], voice.patch});
        return 1;
    } catch (...) { return 0; }
}
uint32_t brew_smaf_fm_on(void* value, uint32_t token, uint32_t channel, uint8_t bank,
                         uint8_t program, uint8_t rhythm, uint8_t note) noexcept {
    if (!value || !token || channel >= 32 || program > 127 || note > 127 || rhythm > 1) return 0;
    auto& engine = *static_cast<Engine*>(value);
    const FmVoicePatch* patch = nullptr;
    for (const auto& definition : engine.patches)
        if (definition.bank == bank && definition.program == program) patch = &definition.patch;
    if (!patch) {
        if (bank != 0 && bank != 128) return 0;
        patch = (rhythm || bank == 128) ? &FmVoicePatch::drumApprox(note) : &FmVoicePatch::gmApprox(program);
    }
    Slot* selected = nullptr;
    for (auto& slot : engine.slots) {
        if (slot.live && slot.token == token) return 0;
        if (!selected && (!slot.live || !slot.voice.active())) selected = &slot;
    }
    if (!selected) selected = &*std::min_element(engine.slots.begin(), engine.slots.end(),
        [](const Slot& a, const Slot& b) { return a.order < b.order; });
    *selected = Slot{};
    selected->token = token;
    selected->channel = channel;
    selected->order = ++engine.order;
    selected->live = true;
    selected->voice.setSampleRate(engine.rate);
    const double frequency = 440.0 * std::pow(2.0, (int(note) - 69 + patch->noteShift) / 12.0);
    selected->voice.noteOn(*patch, frequency, 1.0f);
    return 1;
}
void brew_smaf_fm_off(void* value, uint32_t token) noexcept {
    if (!value) return;
    for (auto& slot : static_cast<Engine*>(value)->slots)
        if (slot.live && slot.token == token) slot.voice.noteOff();
}
void brew_smaf_fm_mute(void* value, uint32_t first, uint32_t count) noexcept {
    if (!value || first > 32 || count > 32 - first) return;
    for (auto& slot : static_cast<Engine*>(value)->slots)
        if (slot.channel >= first && slot.channel - first < count) slot.live = false;
}
uint32_t brew_smaf_fm_render(void* value, uint32_t frames, const Channel* channels, float* output) noexcept {
    if (!value || !channels || !output) return 0;
    auto& engine = *static_cast<Engine*>(value);
    if (frames > engine.rate * 120) return 0;
    for (size_t i = 0; i < 32; ++i) {
        const auto c = channels[i];
        if (!std::isfinite(c.gain) || !std::isfinite(c.left) || !std::isfinite(c.right) ||
            c.gain < 0 || c.gain > 1 || c.left < 0 || c.left > 1 || c.right < 0 || c.right > 1) return 0;
    }
    for (size_t i = 0; i < frames; ++i) {
        for (auto& slot : engine.slots) {
            if (!slot.live) continue;
            const float sample = slot.voice.tick() * 0.25f;
            if (!std::isfinite(sample)) return 0;
            const auto c = channels[slot.channel];
            output[i * 2] += sample * c.gain * c.left;
            output[i * 2 + 1] += sample * c.gain * c.right;
            if (!slot.voice.active()) slot.live = false;
        }
    }
    return 1;
}
}

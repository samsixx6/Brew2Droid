// SPDX-License-Identifier: Apache-2.0
#pragma once
//
// smaf_voice.h -- decode yamaha VM35 / VMA voice exclusives into an FmVoicePatch.
// ----------------------------------------------------------------------------
// a smaf file that uses a custom instrument carries it as a yamaha sysex blob in
// the setup chunk (Mtsu) or inline in the sequence: 43 79 06/07 7F 01 ... for
// the MA-3/MA-5 form, 43 03 ... for the MA-1/2 form. this turns those bytes into
// the parameters the fm core wants. files that only use the chip's built-in gm
// bank carry NO such blob; those channels fall back to the core's own gm patch
// set (that is the whole reason we ship one).
//
// byte layouts per the go-smaf voice/{vm35fm,vmafm}.go reference (read as spec).

#include "ma_fm_core.h"
#include <cstdint>
#include <vector>

namespace fxchain::smaf {

// which instrument slot a voice binds to. captured from bank-select + program-
// change on the channel; drumNote != 0 marks a percussion voice.
struct VoiceKey {
    int bankMSB = 0, bankLSB = 0, pc = 0, drumNote = 0;
    bool operator==(const VoiceKey& o) const {
        return bankMSB == o.bankMSB && bankLSB == o.bankLSB &&
               pc == o.pc && drumNote == o.drumNote;
    }
};

// pcm (sampled) voice: a drum or sampled instrument that plays an Mwa/Awa wave.
struct PcmParams {
    int      fs      = 8000;   // sampling rate from the body (Hz)
    int      waveId  = 0;      // binds to the Mwa/Awa wave with this number
    int      loopPt  = 0;      // loop point (sample index into the decoded wave)
    int      endPt   = 0;      // end point  (sample index; 0 = whole wave)
    bool     loop    = false;  // RM flag: loop between loopPt and endPt
    FmOpPatch env;             // AR/DR/SR/RR/SL/TL reused for the amplitude EG
};

struct ParsedVoice {
    VoiceKey     key;
    FmVoicePatch patch;
    PcmParams    pcm;             // valid when isPcm
    bool         isPcm = false;   // pcm voices play a sampled wave, not fm
    bool         valid = false;
};

// parse ONE exclusive payload (the bytes between the F0 length and the trailing
// F7, i.e. starting at the 0x43 maker id). returns valid=false for anything we
// do not synthesise (pcm voices, unknown makers).
ParsedVoice parseVoiceExclusive(const uint8_t* p, size_t n);

} // namespace fxchain::smaf

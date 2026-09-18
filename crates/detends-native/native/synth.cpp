// Chime synthesis.
//
// The sound a timer makes when it finishes. It has to carry across a room
// without being startling, which rules out both a beep and anything with a
// sharp attack — détends should never demand attention (§15).
//
// A struck bar or bell is a small number of inharmonic partials, each decaying
// at its own rate, with the higher ones dying first. That last detail is what
// separates a chime from a synthesiser tone: the sound gets *darker* as it
// fades, the way a real struck object does.
//
// C++ rather than C for the small amount of structure this wants — a partial is
// naturally a type with its own behaviour.

#include "detends.h"

#include <cmath>

namespace {

struct Partial {
    float ratio;  // frequency, relative to the fundamental
    float gain;   // how much of the sound it is at the moment of the strike
    float decay;  // how fast it dies, relative to the fundamental's rate
};

// Ratios from a struck bar rather than a harmonic series: the small
// inharmonicity is what makes it read as an object rather than as an organ.
constexpr Partial kPartials[] = {
    {1.000f, 1.00f, 1.00f},
    {2.756f, 0.42f, 1.65f},
    {5.404f, 0.22f, 2.60f},
    {8.933f, 0.11f, 3.80f},
    {13.34f, 0.05f, 5.20f},
};

constexpr float kTwoPi = 6.283185307179586f;

}  // namespace

void detends_chime(float *out, size_t frames, unsigned channels, float sample_rate,
                   float base_hz, float seconds) {
    if (!out || channels == 0 || sample_rate <= 0.0f || seconds <= 0.0f) {
        return;
    }

    const float inv_rate = 1.0f / sample_rate;
    // The fundamental should still be just audible as the tail ends.
    const float decay_rate = 5.0f / seconds;

    for (size_t frame = 0; frame < frames; ++frame) {
        const float t = static_cast<float>(frame) * inv_rate;

        float sample = 0.0f;
        for (const Partial &partial : kPartials) {
            const float envelope = std::exp(-t * decay_rate * partial.decay);
            // Once a partial is inaudible, stop paying for it.
            if (envelope < 1.0e-4f) {
                continue;
            }
            sample += partial.gain * envelope *
                      std::sin(kTwoPi * base_hz * partial.ratio * t);
        }

        // A short rise at the very start. Beginning a sine at full amplitude
        // puts a step in the waveform, which is heard as a click before the
        // note itself — the one thing a calm sound cannot have.
        constexpr float kAttack = 0.004f;
        if (t < kAttack) {
            sample *= t / kAttack;
        }

        sample *= 0.32f;  // headroom; this is never the loudest thing playing

        for (unsigned c = 0; c < channels; ++c) {
            out[frame * channels + c] = sample;
        }
    }
}

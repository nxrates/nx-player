// WASM AudioWorklet time-stretch processor
// Wraps Signalsmith Stretch with zero-allocation process() path
//
// All buffers are pre-allocated in stretch_init().
// The JS side writes interleaved float samples into the input buffer
// (obtained via stretch_get_input_ptr()), calls stretch_process(),
// then reads interleaved output from the output buffer
// (obtained via stretch_get_output_ptr()).

#include "signalsmith-stretch/signalsmith-stretch.h"

#include <cstdlib>
#include <cstring>

static signalsmith::stretch::SignalsmithStretch<float> stretcher;

static int g_channels = 0;
static int g_maxFrames = 0;

// Interleaved buffers exposed to JS via pointer
static float* g_inputInterleaved  = nullptr;
static float* g_outputInterleaved = nullptr;

// Per-channel deinterleaved buffers (contiguous block, channel-strided)
static float* g_inputDeinterleaved  = nullptr;
static float* g_outputDeinterleaved = nullptr;

// Lightweight wrapper so stretcher.process() can index [channel][sample]
struct ChannelView {
    float* base;
    int stride; // number of frames per channel
    float* operator[](int ch) const { return base + ch * stride; }
};

extern "C" {

/// Initialise the stretcher and pre-allocate all buffers.
/// Must be called once before any other function.
///   channels   – number of audio channels (1 or 2 typically)
///   sampleRate – e.g. 44100 or 48000
///   maxFrames  – upper bound on frames per process() call
void stretch_init(int channels, int sampleRate, int maxFrames) {
    g_channels  = channels;
    g_maxFrames = maxFrames;

    // Free any previous buffers (safe to free nullptr)
    std::free(g_inputInterleaved);
    std::free(g_outputInterleaved);
    std::free(g_inputDeinterleaved);
    std::free(g_outputDeinterleaved);

    // Allocate interleaved buffers (channels * maxFrames floats each)
    size_t interleavedSize = static_cast<size_t>(channels) * maxFrames;
    g_inputInterleaved   = static_cast<float*>(std::malloc(interleavedSize * sizeof(float)));
    g_outputInterleaved  = static_cast<float*>(std::malloc(interleavedSize * sizeof(float)));

    // Allocate deinterleaved buffers (same total size, laid out channel-major)
    g_inputDeinterleaved  = static_cast<float*>(std::malloc(interleavedSize * sizeof(float)));
    g_outputDeinterleaved = static_cast<float*>(std::malloc(interleavedSize * sizeof(float)));

    // Zero everything
    std::memset(g_inputInterleaved,   0, interleavedSize * sizeof(float));
    std::memset(g_outputInterleaved,  0, interleavedSize * sizeof(float));
    std::memset(g_inputDeinterleaved, 0, interleavedSize * sizeof(float));
    std::memset(g_outputDeinterleaved,0, interleavedSize * sizeof(float));

    // Configure stretcher with default preset (no split computation)
    stretcher.presetDefault(channels, static_cast<float>(sampleRate));
}

void stretch_set_transpose_semitones(float semitones) {
    stretcher.setTransposeSemitones(semitones);
}

void stretch_set_transpose_factor(float factor) {
    stretcher.setTransposeFactor(factor);
}

void stretch_reset() {
    stretcher.reset();
}

/// Returns pointer to the interleaved input buffer.
/// JS should write (channels * inputFrames) floats here before calling process().
float* stretch_get_input_ptr() {
    return g_inputInterleaved;
}

/// Returns pointer to the interleaved output buffer.
/// JS reads (channels * outputFrames) floats from here after process().
float* stretch_get_output_ptr() {
    return g_outputInterleaved;
}

/// Deinterleave input, run stretcher, reinterleave output.
/// ZERO allocations — all buffers were set up in init().
///   inputFrames  – number of input frames written to the input buffer
///   outputFrames – number of output frames requested
void stretch_process(int inputFrames, int outputFrames) {
    const int ch = g_channels;

    // --- Deinterleave input: interleaved [f0c0 f0c1 f1c0 f1c1 ...] → per-channel ---
    for (int c = 0; c < ch; ++c) {
        float* dst = g_inputDeinterleaved + c * g_maxFrames;
        for (int f = 0; f < inputFrames; ++f) {
            dst[f] = g_inputInterleaved[f * ch + c];
        }
    }

    // --- Process ---
    ChannelView inView  { g_inputDeinterleaved,  g_maxFrames };
    ChannelView outView { g_outputDeinterleaved, g_maxFrames };
    stretcher.process(inView, inputFrames, outView, outputFrames);

    // --- Reinterleave output: per-channel → interleaved ---
    for (int c = 0; c < ch; ++c) {
        const float* src = g_outputDeinterleaved + c * g_maxFrames;
        for (int f = 0; f < outputFrames; ++f) {
            g_outputInterleaved[f * ch + c] = src[f];
        }
    }
}

} // extern "C"

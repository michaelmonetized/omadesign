// Omadesign's original adapter is MIT licensed. LibRaw is used under CDDL 1.0;
// its complete, unchanged source and license are in vendor/libraw.
#include "libraw/libraw.h"
#include <chrono>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <memory>
#include <new>

struct OmaRawImage {
    void *owner;
    const unsigned char *data;
    uint64_t data_size;
    uint32_t width, height, colors, bits, warnings, raw_count, dng_opcodes;
    int32_t orientation;
    float iso, shutter, aperture, focal_length, camera_wb[4], baseline_exposure;
    int64_t timestamp;
    char make[64], model[64], lens[128], error[256];
};

namespace {
constexpr uint64_t max_pixels = 64000000;
constexpr size_t max_input = 512ull * 1024 * 1024;
struct DecodeState {
    std::chrono::steady_clock::time_point started = std::chrono::steady_clock::now();
    bool damaged = false;
};

int progress(void *context, LibRaw_progress, int, int) {
    const auto *state = static_cast<DecodeState *>(context);
    return std::chrono::steady_clock::now() - state->started > std::chrono::seconds(120);
}
void damaged(void *context, const char *, INT64) {
    static_cast<DecodeState *>(context)->damaged = true;
}
int fail(OmaRawImage *output, const char *message) {
    std::snprintf(output->error, sizeof(output->error), "%s", message);
    return 1;
}
int failure(OmaRawImage *output, int code) {
    if (code == LIBRAW_CANCELLED_BY_CALLBACK)
        return fail(output, "RAW decoding exceeded the two-minute processing limit");
    return fail(output, libraw_strerror(code));
}
bool bounded(unsigned width, unsigned height) {
    return width > 0 && height > 0 && uint64_t(width) * height <= max_pixels;
}
template <size_t N, size_t M> void copy_text(char (&to)[N], const char (&from)[M]) {
    // Camera metadata is not guaranteed to terminate within its fixed field.
    size_t count = 0;
    while (count < M && count + 1 < N && from[count]) ++count;
    std::memcpy(to, from, count);
    to[count] = 0;
}
}

extern "C" int oma_raw_decode(const unsigned char *bytes, size_t length,
                              OmaRawImage *output) noexcept {
    if (!output) return 1;
    *output = OmaRawImage{};
    if (!bytes || !length || length > max_input)
        return fail(output, "RAW input must contain between 1 byte and 512 MiB");
    try {
        DecodeState state;
        LibRaw raw(LIBRAW_OPTIONS_NO_DATAERR_CALLBACK);
        raw.set_progress_handler(progress, &state);
        raw.set_dataerror_handler(damaged, &state);
        raw.imgdata.rawparams.max_raw_memory_mb = 1024;
        raw.imgdata.rawparams.use_rawspeed = 0;
        raw.imgdata.rawparams.use_dngsdk = 0;
        raw.imgdata.rawparams.options |= LIBRAW_RAWOPTIONS_CAMERAWB_FALLBACK_TO_DAYLIGHT;
        auto &params = raw.imgdata.params;
        params.use_camera_wb = 1;
        params.use_auto_wb = 0;
        params.no_auto_bright = 1;
        params.output_color = 1; // linear sRGB primaries; transfer happens in Rust.
        params.output_bps = 16;
        params.gamm[0] = 1;
        params.gamm[1] = 1;
        params.user_qual = 3; // AHD; X-Trans uses LibRaw's matching demosaic.
        params.highlight = 0;
        params.half_size = 0;
        int code = raw.open_buffer(bytes, length);
        if (code) return failure(output, code);
        if (!bounded(raw.imgdata.sizes.width, raw.imgdata.sizes.height) ||
            !bounded(raw.imgdata.sizes.raw_width, raw.imgdata.sizes.raw_height))
            return fail(output, "RAW images are limited to 64 megapixels");
        // LibRaw stretches non-square sensor pixels during processing. Check
        // that output before it allocates the stretched image, not just after.
        const double aspect = raw.imgdata.sizes.pixel_aspect;
        if (!std::isfinite(aspect) || aspect <= 0)
            return fail(output, "The RAW image has an invalid pixel aspect ratio");
        const double expanded_width = std::ceil(raw.imgdata.sizes.width * (aspect > 1 ? aspect : 1));
        const double expanded_height = std::ceil(raw.imgdata.sizes.height / (aspect < 1 ? aspect : 1));
        if (expanded_width > 65535 || expanded_height > 65535 ||
            expanded_width * expanded_height > max_pixels)
            return fail(output, "RAW images after pixel-aspect correction are limited to 64 megapixels");
        if (!raw.imgdata.idata.raw_count)
            return fail(output, "This file does not contain a supported camera RAW image");
        copy_text(output->make, raw.imgdata.idata.make);
        copy_text(output->model, raw.imgdata.idata.model);
        copy_text(output->lens, raw.imgdata.lens.Lens);
        output->orientation = raw.imgdata.sizes.flip;
        output->raw_count = raw.imgdata.idata.raw_count;
        output->iso = raw.imgdata.other.iso_speed;
        output->shutter = raw.imgdata.other.shutter;
        output->aperture = raw.imgdata.other.aperture;
        output->focal_length = raw.imgdata.other.focal_len;
        output->timestamp = raw.imgdata.other.timestamp;
        for (int i = 0; i < 4; ++i) output->camera_wb[i] = raw.imgdata.color.cam_mul[i];
        output->baseline_exposure = raw.imgdata.color.dng_levels.baseline_exposure;
        for (int i = 0; i < 3; ++i)
            if (raw.imgdata.color.dng_levels.rawopcodes[i].len)
                output->dng_opcodes |= (1u << i);
        const bool missing_wb = !std::isfinite(output->camera_wb[0]) ||
            output->camera_wb[0] <= 0.00001f;
        if (missing_wb) params.use_camera_wb = 0;
        code = raw.unpack();
        if (code) return failure(output, code);
        if (state.damaged)
            return fail(output, "RAW sensor data is truncated or damaged");
        code = raw.dcraw_process();
        if (code) return failure(output, code);
        if (state.damaged)
            return fail(output, "RAW sensor data is truncated or damaged");
        if (!bounded(raw.imgdata.sizes.width, raw.imgdata.sizes.height))
            return fail(output, "Developed RAW images are limited to 64 megapixels");
        std::unique_ptr<libraw_processed_image_t, decltype(&LibRaw::dcraw_clear_mem)> image(
            raw.dcraw_make_mem_image(&code), LibRaw::dcraw_clear_mem);
        if (!image) return failure(output, code);
        if (image->type != LIBRAW_IMAGE_BITMAP || image->bits != 16 ||
            (image->colors != 3 && image->colors != 1) ||
            !bounded(image->width, image->height) ||
            uint64_t(image->data_size) != uint64_t(image->width) * image->height * image->colors * 2)
            return fail(output, "The RAW decoder returned an unsupported pixel layout");
        output->width = image->width;
        output->height = image->height;
        output->colors = image->colors;
        output->bits = image->bits;
        output->warnings = raw.imgdata.process_warnings |
            (missing_wb ? LIBRAW_WARN_BAD_CAMERA_WB : 0);
        output->data_size = image->data_size;
        output->data = image->data;
        output->owner = image.release();
        return 0;
    } catch (const std::bad_alloc &) {
        return fail(output, "Not enough memory to decode this RAW image");
    } catch (...) {
        return fail(output, "The camera RAW decoder could not read this file");
    }
}

extern "C" void oma_raw_free(void *owner) noexcept {
    if (owner) LibRaw::dcraw_clear_mem(static_cast<libraw_processed_image_t *>(owner));
}

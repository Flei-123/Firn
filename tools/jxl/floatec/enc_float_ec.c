#include <jxl/encode.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <math.h>
#include <stdint.h>

static float quantize(float v, int mant) {
    if (v == 0.0f) return v;
    int e;
    float m = frexpf(v, &e);
    return ldexpf(roundf(ldexpf(m, mant + 1)), e - mant - 1);
}

static void fail(const char *m) { fprintf(stderr, "error: %s\n", m); exit(1); }

int main(int argc, char **argv) {
    if (argc < 8) {
        fprintf(stderr, "usage: enc W H distance resampling ec_resampling modular(0/1) out.jxl [pattern] [bits] [exp]\n");
        return 2;
    }
    uint32_t w = atoi(argv[1]), h = atoi(argv[2]);
    float dist = atof(argv[3]);
    int rs = atoi(argv[4]), ers = atoi(argv[5]), modular = atoi(argv[6]);
    const char *out = argv[7];
    int pattern = argc > 8 ? atoi(argv[8]) : 0;
    uint32_t abits = argc > 9 ? atoi(argv[9]) : 32;
    uint32_t aexp = argc > 10 ? atoi(argv[10]) : 8;
    float *px = malloc((size_t)w * h * 4 * sizeof(float));
    for (uint32_t y = 0; y < h; y++)
        for (uint32_t x = 0; x < w; x++) {
            float *p = px + ((size_t)y * w + x) * 4;
            p[0] = 0.5f + 0.5f * sinf(x * 0.11f + y * 0.05f);
            p[1] = 0.5f + 0.5f * cosf(x * 0.07f - y * 0.13f);
            p[2] = (float)(x ^ y & 31) / 31.0f;
            float a;
            if (pattern == 0) a = 0.5f + 0.5f * sinf(x * 0.19f) * cosf(y * 0.17f);
            else if (pattern == 1) a = ((x / 8 + y / 8) & 1) ? 1.0f : 0.0f;
            else a = (float)x / (float)(w - 1);
            p[3] = abits < 32 ? quantize(a, (int)(abits - aexp - 1)) : a;
        }
    JxlEncoder *enc = JxlEncoderCreate(NULL);
    JxlBasicInfo bi;
    JxlEncoderInitBasicInfo(&bi);
    bi.xsize = w; bi.ysize = h;
    bi.bits_per_sample = 16; bi.exponent_bits_per_sample = 0;
    bi.uses_original_profile = modular == 1 ? JXL_TRUE : JXL_FALSE;
    bi.num_color_channels = 3;
    bi.num_extra_channels = 1;
    bi.alpha_bits = abits;
    bi.alpha_exponent_bits = aexp;
    bi.alpha_premultiplied = JXL_FALSE;
    if (JxlEncoderSetBasicInfo(enc, &bi) != JXL_ENC_SUCCESS) fail("basic info");
    JxlExtraChannelInfo ei;
    JxlEncoderInitExtraChannelInfo(JXL_CHANNEL_ALPHA, &ei);
    ei.bits_per_sample = abits;
    ei.exponent_bits_per_sample = aexp;
    ei.alpha_premultiplied = JXL_FALSE;
    if (JxlEncoderSetExtraChannelInfo(enc, 0, &ei) != JXL_ENC_SUCCESS) fail("extra channel info");
    JxlColorEncoding ce;
    JxlColorEncodingSetToSRGB(&ce, JXL_FALSE);
    if (JxlEncoderSetColorEncoding(enc, &ce) != JXL_ENC_SUCCESS) fail("color encoding");
    JxlEncoderFrameSettings *fs = JxlEncoderFrameSettingsCreate(enc, NULL);
    JxlEncoderFrameSettingsSetOption(fs, JXL_ENC_FRAME_SETTING_EFFORT, 7);
    if (dist == 0.0f) {
        if (JxlEncoderSetFrameLossless(fs, JXL_TRUE) != JXL_ENC_SUCCESS) fail("lossless");
    } else {
        JxlEncoderSetFrameDistance(fs, dist);
    }
    if (modular) JxlEncoderFrameSettingsSetOption(fs, JXL_ENC_FRAME_SETTING_MODULAR, 1);
    if (rs > 1) JxlEncoderFrameSettingsSetOption(fs, JXL_ENC_FRAME_SETTING_RESAMPLING, rs);
    if (ers > 1) JxlEncoderFrameSettingsSetOption(fs, JXL_ENC_FRAME_SETTING_EXTRA_CHANNEL_RESAMPLING, ers);
    JxlPixelFormat pf = {4, JXL_TYPE_FLOAT, JXL_NATIVE_ENDIAN, 0};
    JxlEncoderStatus st = JxlEncoderAddImageFrame(fs, &pf, px, (size_t)w * h * 4 * sizeof(float));
    if (st != JXL_ENC_SUCCESS) { fprintf(stderr, "add frame status %d err %d\n", st, JxlEncoderGetError(enc)); return 1; }
    JxlEncoderCloseInput(enc);
    size_t cap = 1 << 20, len = 0;
    uint8_t *buf = malloc(cap);
    for (;;) {
        uint8_t *next = buf + len;
        size_t avail = cap - len;
        st = JxlEncoderProcessOutput(enc, &next, &avail);
        len = cap - avail;
        if (st == JXL_ENC_NEED_MORE_OUTPUT) { cap *= 2; buf = realloc(buf, cap); continue; }
        if (st != JXL_ENC_SUCCESS) { fprintf(stderr, "process status %d\n", st); return 1; }
        break;
    }
    FILE *f = fopen(out, "wb");
    fwrite(buf, 1, len, f);
    fclose(f);
    JxlEncoderDestroy(enc);
    return 0;
}

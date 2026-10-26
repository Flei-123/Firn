#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "aom/aom_encoder.h"
#include "aom/aomcx.h"

static int die(const char *m) {
    fprintf(stderr, "aom_still: %s\n", m);
    return 1;
}

int main(int argc, char **argv) {
    if (argc < 13) {
        return die("usage: in.yuv w h chroma(400|420|422|444) bitdepth lossless sr_mode sr_denom cdef lr cq out.obu");
    }
    const char *in_path = argv[1];
    int w = atoi(argv[2]);
    int h = atoi(argv[3]);
    int chroma = atoi(argv[4]);
    int bd = atoi(argv[5]);
    int lossless = atoi(argv[6]);
    int sr_mode = atoi(argv[7]);
    int sr_denom = atoi(argv[8]);
    int cdef = atoi(argv[9]);
    int lr = atoi(argv[10]);
    int cq = atoi(argv[11]);
    const char *out_path = argv[12];
    int sx = chroma == 444 ? 0 : 1;
    int sy = chroma == 420 ? 1 : 0;
    int planes = chroma == 400 ? 1 : 3;
    int unit = bd > 8 ? 2 : 1;
    aom_img_fmt_t fmt;
    if (bd > 8) {
        fmt = chroma == 444 ? AOM_IMG_FMT_I44416 : chroma == 422 ? AOM_IMG_FMT_I42216 : AOM_IMG_FMT_I42016;
    } else {
        fmt = chroma == 444 ? AOM_IMG_FMT_I444 : chroma == 422 ? AOM_IMG_FMT_I422 : AOM_IMG_FMT_I420;
    }
    aom_image_t img;
    if (!aom_img_alloc(&img, fmt, w, h, 1)) {
        return die("img alloc");
    }
    FILE *fi = fopen(in_path, "rb");
    if (!fi) {
        return die("open input");
    }
    for (int p = 0; p < planes; p++) {
        int pw = p ? (w + sx) >> sx : w;
        int ph = p ? (h + sy) >> sy : h;
        for (int y = 0; y < ph; y++) {
            if (fread(img.planes[p] + (size_t)y * img.stride[p], unit, pw, fi) != (size_t)pw) {
                return die("short input");
            }
        }
    }
    fclose(fi);
    if (planes == 1) {
        int pw = (w + 1) >> 1;
        int ph = (h + 1) >> 1;
        for (int p = 1; p < 3; p++) {
            for (int y = 0; y < ph; y++) {
                if (unit == 2) {
                    for (int x = 0; x < pw; x++) {
                        ((uint16_t *)(img.planes[p] + (size_t)y * img.stride[p]))[x] = 1 << (bd - 1);
                    }
                } else {
                    memset(img.planes[p] + (size_t)y * img.stride[p], 128, pw);
                }
            }
        }
    }
    aom_codec_iface_t *iface = aom_codec_av1_cx();
    aom_codec_enc_cfg_t cfg;
    if (aom_codec_enc_config_default(iface, &cfg, 0)) {
        return die("config default");
    }
    cfg.g_w = w;
    cfg.g_h = h;
    cfg.g_bit_depth = bd == 12 ? AOM_BITS_12 : bd == 10 ? AOM_BITS_10 : AOM_BITS_8;
    cfg.g_input_bit_depth = bd;
    cfg.g_profile = (chroma == 444 || bd == 12) ? (bd == 12 && chroma != 444 ? 2 : 1) : 0;
    if (chroma == 422) {
        cfg.g_profile = 2;
    }
    cfg.g_lag_in_frames = 0;
    cfg.g_limit = 1;
    cfg.kf_mode = AOM_KF_DISABLED;
    cfg.rc_end_usage = AOM_Q;
    cfg.rc_min_quantizer = 0;
    cfg.rc_max_quantizer = lossless == 2 ? 0 : 63;
    cfg.rc_superres_mode = (aom_superres_mode)sr_mode;
    cfg.rc_superres_denominator = sr_denom;
    cfg.rc_superres_kf_denominator = sr_denom;
    cfg.rc_superres_qthresh = 1;
    cfg.rc_superres_kf_qthresh = 1;
    aom_codec_ctx_t ctx;
    if (aom_codec_enc_init(&ctx, iface, &cfg, bd > 8 ? AOM_CODEC_USE_HIGHBITDEPTH : 0)) {
        fprintf(stderr, "%s\n", aom_codec_error_detail(&ctx));
        return die("enc init");
    }
    aom_codec_control(&ctx, AOME_SET_CPUUSED, 4);
    aom_codec_control(&ctx, AOME_SET_CQ_LEVEL, cq);
    aom_codec_control(&ctx, AV1E_SET_LOSSLESS, lossless == 1);
    aom_codec_control(&ctx, AV1E_SET_ENABLE_CDEF, cdef);
    aom_codec_control(&ctx, AV1E_SET_ENABLE_RESTORATION, lr);
    aom_codec_control(&ctx, AV1E_SET_ENABLE_SUPERRES, sr_mode != 0);
    aom_codec_control(&ctx, AV1E_SET_ENABLE_PALETTE, 0);
    aom_codec_control(&ctx, AV1E_SET_ENABLE_INTRABC, 0);
    aom_codec_control(&ctx, AV1E_SET_SUPERBLOCK_SIZE, AOM_SUPERBLOCK_SIZE_64X64);
    FILE *fo = fopen(out_path, "wb");
    if (!fo) {
        return die("open output");
    }
    for (int pass = 0; pass < 2; pass++) {
        if (aom_codec_encode(&ctx, pass ? NULL : &img, 0, 1, 0)) {
            return die("encode");
        }
        aom_codec_iter_t it = NULL;
        const aom_codec_cx_pkt_t *pkt;
        while ((pkt = aom_codec_get_cx_data(&ctx, &it)) != NULL) {
            if (pkt->kind == AOM_CODEC_CX_FRAME_PKT) {
                fwrite(pkt->data.frame.buf, 1, pkt->data.frame.sz, fo);
            }
        }
    }
    fclose(fo);
    aom_codec_destroy(&ctx);
    return 0;
}

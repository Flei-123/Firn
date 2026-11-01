#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
#include "av1/common/av1_inv_txfm1d.h"

typedef void (*Fn)(const int32_t *, int32_t *, int8_t, const int8_t *);

static const int WL[19] = {2, 3, 4, 5, 6, 2, 3, 3, 4, 4, 5, 5, 6, 2, 4, 3, 5, 4, 6};
static const int HL[19] = {2, 3, 4, 5, 6, 3, 2, 4, 3, 5, 4, 6, 5, 4, 2, 5, 3, 6, 4};
static const int SH0[19] = {0, -1, -2, -2, -2, 0, 0, -1, -1, -1, -1, -1, -1, -1, -1, -2, -2, -2, -2};
static const int VT[16] = {0, 1, 0, 1, 2, 0, 2, 1, 2, 3, 0, 3, 1, 3, 2, 3};
static const int HT[16] = {0, 0, 1, 1, 0, 2, 2, 2, 1, 3, 3, 0, 3, 1, 3, 2};

static Fn pick(int kind, int lg) {
  if (kind == 3) {
    if (lg == 2) return av1_iidentity4_c;
    if (lg == 3) return av1_iidentity8_c;
    if (lg == 4) return av1_iidentity16_c;
    if (lg == 5) return av1_iidentity32_c;
    return NULL;
  }
  if (kind == 0) {
    Fn t[] = {NULL, NULL, av1_idct4, av1_idct8, av1_idct16, av1_idct32, av1_idct64};
    return t[lg];
  }
  Fn a[] = {NULL, NULL, av1_iadst4, av1_iadst8, av1_iadst16};
  return lg <= 4 ? a[lg] : NULL;
}

static int clip_px(int v, int bd) {
  int hi = (1 << bd) - 1;
  return v < 0 ? 0 : (v > hi ? hi : v);
}

static void txfm2d(const int32_t *coef, uint16_t *dst, int stride, int ts, int tt, int bd) {
  int wl = WL[ts], hl = HL[ts], w = 1 << wl, h = 1 << hl;
  int ud = tt == 4 || tt == 6 || tt == 8 || tt == 14;
  int lr = tt == 5 || tt == 6 || tt == 7 || tt == 15;
  int rowr = bd == 8 ? 16 : (bd == 10 ? 18 : 20);
  int colr = bd == 12 ? 18 : 16;
  int8_t sr_row[16], sr_col[16];
  memset(sr_row, rowr, 16);
  memset(sr_col, colr, 16);
  Fn frow = pick(HT[tt], wl), fcol = pick(VT[tt], hl);
  if (!frow || !fcol) { fprintf(stderr, "invalid combo\n"); exit(2); }
  int rect = wl - hl;
  int32_t tin[64], tout[64];
  int32_t *buf = malloc(sizeof(int32_t) * w * h);
  for (int r = 0; r < h; ++r) {
    for (int c = 0; c < w; ++c) {
      int64_t v = coef[c * h + r];
      tin[c] = (rect == 1 || rect == -1) ? round_shift(v * NewInvSqrt2, NewSqrt2Bits) : (int32_t)v;
    }
    clamp_buf(tin, w, bd + 8);
    frow(tin, buf + r * w, 12, sr_row);
    if (SH0[ts]) for (int c = 0; c < w; ++c) buf[r * w + c] = round_shift(buf[r * w + c], -SH0[ts]);
  }
  for (int c = 0; c < w; ++c) {
    for (int r = 0; r < h; ++r) tin[r] = buf[r * w + (lr ? w - c - 1 : c)];
    clamp_buf(tin, h, bd + 6 > 16 ? bd + 6 : 16);
    fcol(tin, tout, 12, sr_col);
    for (int r = 0; r < h; ++r) tout[r] = round_shift(tout[r], 4);
    for (int r = 0; r < h; ++r) {
      int v = tout[ud ? h - r - 1 : r];
      dst[r * stride + c] = clip_px(dst[r * stride + c] + v, bd);
    }
  }
  free(buf);
}

static void wht(const int32_t *input, uint16_t *dest, int stride, int bd) {
  int i;
  int32_t output[16], a1, b1, c1, d1, e1;
  const int32_t *ip = input;
  int32_t *op = output;
  for (i = 0; i < 4; i++) {
    a1 = ip[4 * 0] >> 2; c1 = ip[4 * 1] >> 2; d1 = ip[4 * 2] >> 2; b1 = ip[4 * 3] >> 2;
    a1 += c1; d1 -= b1; e1 = (a1 - d1) >> 1; b1 = e1 - b1; c1 = e1 - c1; a1 -= b1; d1 += c1;
    op[4 * 0] = a1; op[4 * 1] = b1; op[4 * 2] = c1; op[4 * 3] = d1;
    ip++; op++;
  }
  ip = output;
  for (i = 0; i < 4; i++) {
    a1 = ip[0]; c1 = ip[1]; d1 = ip[2]; b1 = ip[3];
    a1 += c1; d1 -= b1; e1 = (a1 - d1) >> 1; b1 = e1 - b1; c1 = e1 - c1; a1 -= b1; d1 += c1;
    dest[stride * 0] = clip_px(dest[stride * 0] + a1, bd);
    dest[stride * 1] = clip_px(dest[stride * 1] + b1, bd);
    dest[stride * 2] = clip_px(dest[stride * 2] + c1, bd);
    dest[stride * 3] = clip_px(dest[stride * 3] + d1, bd);
    ip += 4; dest++;
  }
}

int main(void) {
  int ts, tt, bd;
  while (scanf("%d %d %d", &ts, &tt, &bd) == 3) {
    int w = ts < 0 ? 4 : 1 << WL[ts], h = ts < 0 ? 4 : 1 << HL[ts];
    int32_t *rm = malloc(4 * w * h), *cm = malloc(4 * w * h);
    uint16_t *dst = malloc(2 * w * h);
    for (int i = 0; i < w * h; ++i) scanf("%d", &rm[i]);
    for (int i = 0; i < w * h; ++i) { int v; scanf("%d", &v); dst[i] = v; }
    for (int r = 0; r < h; ++r) for (int c = 0; c < w; ++c) cm[c * h + r] = rm[r * w + c];
    if (ts < 0) wht(cm, dst, w, bd); else txfm2d(cm, dst, w, ts, tt, bd);
    for (int i = 0; i < w * h; ++i) printf("%d ", dst[i]);
    printf("\n");
    free(rm); free(cm); free(dst);
  }
  return 0;
}

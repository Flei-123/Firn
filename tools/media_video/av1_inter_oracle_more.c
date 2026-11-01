static uint8_t *wedge_tab;
static int blk_w[22] = {4,4,8,8,8,16,16,16,32,32,32,64,64,64,128,128,4,16,8,32,16,64};
static int blk_h[22] = {4,8,4,8,16,8,16,32,16,32,64,32,64,128,64,128,16,4,32,8,64,16};
static int wedge_ok(int bs) { int w = blk_w[bs], h = blk_h[bs]; return w >= 8 && h >= 8 && w <= 32 && h <= 32 && w * 4 >= h && h * 4 >= w; }
static long wedge_off(int bs, int sign, int widx) {
  long o = 0;
  for (int b = 0; b < bs; b++) if (wedge_ok(b)) o += 32L * blk_w[b] * blk_h[b];
  return o + ((long)widx * 2 + sign) * blk_w[bs] * blk_h[bs];
}
static long smooth_off(int mode, int bs) {
  long o = 0;
  for (int b = 0; b < 22; b++) if (wedge_ok(b) || (blk_w[b] <= 32 && blk_h[b] <= 32)) { if (b == bs) break; o += blk_w[b] * blk_h[b]; }
  long base = 0;
  for (int b = 0; b < 22; b++) if (wedge_ok(b)) base += 32L * blk_w[b] * blk_h[b];
  long per = 0;
  for (int b = 0; b < 22; b++) if (blk_w[b] <= 32 && blk_h[b] <= 32) per += blk_w[b] * blk_h[b];
  return base + mode * per + o;
}

static void dist_weights_lib(int d0, int d1, int *fwd_offset, int *bck_offset) {
  const int order = d0 <= d1;
  if (d0 == 0 || d1 == 0) {
    *fwd_offset = quant_dist_lookup_table[3][order];
    *bck_offset = quant_dist_lookup_table[3][1 - order];
    return;
  }
  int i;
  for (i = 0; i < 3; ++i) {
    int c0 = quant_dist_weight[i][order];
    int c1 = quant_dist_weight[i][!order];
    int d0_c0 = d0 * c0;
    int d1_c1 = d1 * c1;
    if ((d0 > d1 && d0_c0 < d1_c1) || (d0 <= d1 && d0_c0 > d1_c1)) break;
  }
  *fwd_offset = quant_dist_lookup_table[i][order];
  *bck_offset = quant_dist_lookup_table[i][1 - order];
}

static uint32_t case_dist(const int *p) {
  int f, b;
  dist_weights_lib(p[0], p[1], &f, &b);
  return (uint32_t)(f * 256 + b);
}

static int post_of(int bd) { int r0 = bd == 12 ? 5 : 3; return 14 - r0 - 7; }

static uint32_t case_mask(const int *p) {
  int bd = p[0], w = p[1], h = p[2], ssx = p[3], ssy = p[4], type = p[5], bs = p[6], widx = p[7], sg = p[8], inv = p[9];
  rs_state = p[10];
  int bwl = w << ssx, bhl = h << ssy;
  int r0 = bd == 12 ? 5 : 3, post = post_of(bd);
  int off_bits = bd + 14 - r0;
  int round_off = (1 << (off_bits - 7)) + (1 << (off_bits - 7 - 1));
  uint32_t hsh = 2166136261u;
  uint16_t *dst = calloc(w * h, 2);
  uint8_t *mask = malloc(bwl * bhl);
  if (type == 1) {
    uint16_t *l0 = malloc(2 * bwl * bhl), *l1 = malloc(2 * bwl * bhl);
    uint16_t *c0 = malloc(2 * w * h), *c1 = malloc(2 * w * h);
    for (int i = 0; i < bwl * bhl; i++) { int pix = bigr(1 << bd); int v = (pix << post) + rnd(1 << post) - (1 << post) / 2; l0[i] = v + round_off; }
    for (int i = 0; i < bwl * bhl; i++) { int pix = bigr(1 << bd); int v = (pix << post) + rnd(1 << post) - (1 << post) / 2; l1[i] = v + round_off; }
    for (int i = 0; i < w * h; i++) { int pix = bigr(1 << bd); int v = (pix << post) + rnd(1 << post) - (1 << post) / 2; c0[i] = v + round_off; }
    for (int i = 0; i < w * h; i++) { int pix = bigr(1 << bd); int v = (pix << post) + rnd(1 << post) - (1 << post) / 2; c1[i] = v + round_off; }
    ConvolveParams cp; memset(&cp, 0, sizeof(cp)); cp.round_0 = r0; cp.round_1 = 7; cp.is_compound = 1;
    av1_build_compound_diffwtd_mask_d16_c(mask, inv ? DIFFWTD_38_INV : DIFFWTD_38, l0, bwl, l1, bwl, bhl, bwl, &cp, bd);
    for (int i = 0; i < bwl * bhl; i++) hsh = fnv(hsh, mask[i]);
    aom_highbd_blend_a64_d16_mask_c(CONVERT_TO_BYTEPTR(dst), w, c0, w, c1, w, mask, bwl, w, h, ssx, ssy, &cp, bd);
    free(l0); free(l1); free(c0); free(c1);
  } else if (type == 0) {
    uint16_t *c0 = malloc(2 * w * h), *c1 = malloc(2 * w * h);
    for (int i = 0; i < w * h; i++) { int pix = bigr(1 << bd); int v = (pix << post) + rnd(1 << post) - (1 << post) / 2; c0[i] = v + round_off; }
    for (int i = 0; i < w * h; i++) { int pix = bigr(1 << bd); int v = (pix << post) + rnd(1 << post) - (1 << post) / 2; c1[i] = v + round_off; }
    memcpy(mask, wedge_tab + wedge_off(bs, sg, widx), bwl * bhl);
    for (int i = 0; i < bwl * bhl; i++) hsh = fnv(hsh, mask[i]);
    ConvolveParams cp; memset(&cp, 0, sizeof(cp)); cp.round_0 = r0; cp.round_1 = 7; cp.is_compound = 1;
    aom_highbd_blend_a64_d16_mask_c(CONVERT_TO_BYTEPTR(dst), w, c0, w, c1, w, mask, bwl, w, h, ssx, ssy, &cp, bd);
    free(c0); free(c1);
  } else {
    uint16_t *c0 = malloc(2 * w * h), *c1 = malloc(2 * w * h);
    for (int i = 0; i < w * h; i++) c0[i] = bigr(1 << bd);
    for (int i = 0; i < w * h; i++) c1[i] = bigr(1 << bd);
    if (type == 3) memcpy(mask, wedge_tab + wedge_off(bs, 0, widx), bwl * bhl);
    else memcpy(mask, wedge_tab + smooth_off(sg, bs), bwl * bhl);
    for (int i = 0; i < bwl * bhl; i++) hsh = fnv(hsh, mask[i]);
    aom_highbd_blend_a64_mask_c(CONVERT_TO_BYTEPTR(dst), w, CONVERT_TO_BYTEPTR(c0), w, CONVERT_TO_BYTEPTR(c1), w, mask, bwl, w, h, ssx, ssy, bd);
    free(c0); free(c1);
  }
  for (int i = 0; i < w * h; i++) hsh = fnv(hsh, dst[i]);
  free(dst); free(mask);
  return hsh;
}

static uint32_t case_obmc(const int *p) {
  int bd = p[0], w = p[1], h = p[2], ov = p[3], dir = p[4];
  rs_state = p[5];
  uint16_t *cur = malloc(2 * w * h), *nb = malloc(2 * w * h);
  for (int i = 0; i < w * h; i++) cur[i] = bigr(1 << bd);
  for (int i = 0; i < w * h; i++) nb[i] = bigr(1 << bd);
  const uint8_t *mk = av1_get_obmc_mask(ov);
  if (dir == 0) aom_highbd_blend_a64_vmask_c(CONVERT_TO_BYTEPTR(cur), w, CONVERT_TO_BYTEPTR(cur), w, CONVERT_TO_BYTEPTR(nb), w, mk, w, ov, bd);
  else aom_highbd_blend_a64_hmask_c(CONVERT_TO_BYTEPTR(cur), w, CONVERT_TO_BYTEPTR(cur), w, CONVERT_TO_BYTEPTR(nb), w, mk, ov, h, bd);
  uint32_t hsh = 2166136261u;
  for (int i = 0; i < w * h; i++) hsh = fnv(hsh, cur[i]);
  free(cur); free(nb);
  return hsh;
}

static void warp_one(const int *mat6, const uint16_t *ref, int rw, int rh, int bd, int w, int h, int pc, int pr, int ssx, int ssy, ConvolveParams *cp, uint16_t *dst, int *ok) {
  WarpedMotionParams wm; memset(&wm, 0, sizeof(wm)); wm.wmtype = AFFINE;
  for (int i = 0; i < 6; i++) wm.wmmat[i] = mat6[i];
  *ok = av1_get_shear_params(&wm);
  if (!*ok) return;
  av1_highbd_warp_affine_c(wm.wmmat, ref, rw, rh, rw, dst, pc, pr, w, h, w, ssx, ssy, bd, cp, wm.alpha, wm.beta, wm.gamma, wm.delta);
}

static uint32_t case_warp(const int *p) {
  int bd = p[0], w = p[1], h = p[2], pc = p[3], pr = p[4], ssx = p[5], ssy = p[6], rw = p[7], rh = p[8], mode = p[9], fwd = p[10], bck = p[11];
  rs_state = p[12];
  const int *m0 = p + 13, *m1 = p + 19;
  uint16_t *ref = malloc(2 * rw * rh);
  for (int i = 0; i < rw * rh; i++) ref[i] = bigr(1 << bd);
  uint16_t *dst = calloc(w * h, 2), *d16 = calloc(w * h, 2);
  ConvolveParams cp; memset(&cp, 0, sizeof(cp));
  int r0 = bd == 12 ? 5 : 3;
  cp.round_0 = r0; cp.is_compound = mode != 0;
  cp.round_1 = cp.is_compound ? COMPOUND_ROUND1_BITS : 2 * FILTER_BITS - r0;
  cp.dst = d16; cp.dst_stride = w;
  int ok = 0;
  warp_one(m0, ref, rw, rh, bd, w, h, pc, pr, ssx, ssy, &cp, dst, &ok);
  uint32_t hsh = 2166136261u;
  if (!ok) { free(ref); free(dst); free(d16); return 0; }
  if (mode) {
    cp.do_average = 1; cp.use_dist_wtd_comp_avg = mode == 2; cp.fwd_offset = fwd; cp.bck_offset = bck;
    warp_one(m1, ref, rw, rh, bd, w, h, pc, pr, ssx, ssy, &cp, dst, &ok);
    if (!ok) { free(ref); free(dst); free(d16); return 0; }
  }
  for (int i = 0; i < w * h; i++) hsh = fnv(hsh, dst[i]);
  free(ref); free(dst); free(d16);
  return hsh;
}

static uint32_t case_warpest(const int *p) {
  int np = p[0], bw = p[1], bh = p[2], mvy = p[3], mvx = p[4], mi_row = p[5], mi_col = p[6];
  int pts1[16], pts2[16];
  for (int i = 0; i < 16; i++) { pts1[i] = p[7 + i]; pts2[i] = p[23 + i]; }
  WarpedMotionParams wm; memset(&wm, 0, sizeof(wm)); wm.wmtype = AFFINE;
  int bs = 0;
  for (int b = 0; b < 22; b++) if (blk_w[b] == bw && blk_h[b] == bh) bs = b;
  int bad = av1_find_projection(np, pts1, pts2, bs, mvy, mvx, &wm, mi_row, mi_col);
  uint32_t hsh = 2166136261u;
  hsh = fnv(hsh, bad ? 0 : 1);
  if (!bad) {
    for (int i = 0; i < 6; i++) hsh = fnv(hsh, (uint32_t)wm.wmmat[i]);
    hsh = fnv(hsh, (uint16_t)wm.alpha); hsh = fnv(hsh, (uint16_t)wm.beta); hsh = fnv(hsh, (uint16_t)wm.gamma); hsh = fnv(hsh, (uint16_t)wm.delta);
  }
  return hsh;
}

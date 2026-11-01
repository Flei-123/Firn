static int64_t rs_state;
static int rnd(int n) { rs_state = (rs_state * 1103515245 + 12345) & 2147483647; return (int)(((rs_state >> 16) & 32767) % n); }
static int bigr(int n) { int a = rnd(32768); int b = rnd(32768); return (int)(((int64_t)a * 32768 + b) % n); }
static uint32_t fnv(uint32_t h, uint32_t b) { return (uint32_t)(((uint64_t)(h ^ b) * 16777619ULL) & 4294967295ULL); }
#define PAD 1024
static int64_t round2_signed(int64_t v, int n) { return v >= 0 ? ((v + ((int64_t)1 << (n - 1))) >> n) : -((-v + ((int64_t)1 << (n - 1))) >> n); }

static void run_pred(const uint16_t *pad, int pstride, int rw, int rh, int bd, int w, int h, int fxt, int fyt,
                     int sx, int sy, int sc_x, int sc_y, int x, int y, int ssx, int ssy, int mvx, int mvy,
                     int compound, ConvolveParams *cp, uint16_t *dst, int dst_stride) {
  int64_t ox = ((int64_t)x << 4) + ((2 * mvx) >> ssx) + 8;
  int64_t oy = ((int64_t)y << 4) + ((2 * mvy) >> ssy) + 8;
  int64_t bx = ox * sc_x - ((int64_t)8 << 14);
  int64_t by = oy * sc_y - ((int64_t)8 << 14);
  int64_t stx = round2_signed(bx, 8) + 32;
  int64_t sty = round2_signed(by, 8) + 32;
  int stepx = (int)round2_signed(sc_x, 4);
  int stepy = (int)round2_signed(sc_y, 4);
  const uint16_t *src = pad + (PAD + (sty >> 10)) * pstride + PAD + (stx >> 10);
  const InterpFilterParams *fpx = av1_get_interp_filter_params_with_block_size(fxt, w);
  const InterpFilterParams *fpy = av1_get_interp_filter_params_with_block_size(fyt, h);
  av1_highbd_convolve_2d_scale_c(src, pstride, dst, dst_stride, w, h, fpx, fpy, (int)(stx & 1023), stepx,
                                 (int)(sty & 1023), stepy, cp, bd);
}

static void build_pad(uint16_t *pad, int pstride, const uint16_t *ref, int rw, int rh) {
  int total_h = rh + 2 * PAD;
  for (int yy = 0; yy < total_h; yy++) {
    int sy = yy - PAD; sy = sy < 0 ? 0 : (sy >= rh ? rh - 1 : sy);
    for (int xx = 0; xx < pstride; xx++) {
      int sx = xx - PAD; sx = sx < 0 ? 0 : (sx >= rw ? rw - 1 : sx);
      pad[yy * pstride + xx] = ref[sy * rw + sx];
    }
  }
}

static uint32_t case_mc(const int *p) {
  int bd = p[0], w = p[1], h = p[2], fxt = p[3], fyt = p[4], rw = p[5], rh = p[6], fw = p[7], fh = p[8];
  int x = p[9], y = p[10], ssx = p[11], ssy = p[12], mvx0 = p[13], mvy0 = p[14], mvx1 = p[15], mvy1 = p[16];
  int mode = p[17], fwd = p[18], bck = p[19];
  rs_state = p[20];
  uint16_t *ref = malloc(sizeof(uint16_t) * rw * rh);
  for (int i = 0; i < rw * rh; i++) ref[i] = bigr(1 << bd);
  int pstride = rw + 2 * PAD;
  uint16_t *pad = malloc(sizeof(uint16_t) * pstride * (rh + 2 * PAD));
  build_pad(pad, pstride, ref, rw, rh);
  int sc_x = (int)((((int64_t)rw << 14) + fw / 2) / fw);
  int sc_y = (int)((((int64_t)rh << 14) + fh / 2) / fh);
  uint16_t *dst = calloc(w * h, 2);
  uint16_t *d16 = calloc(w * h, 2);
  ConvolveParams cp;
  memset(&cp, 0, sizeof(cp));
  int r0 = bd == 12 ? 5 : 3;
  cp.round_0 = r0;
  cp.is_compound = mode != 0;
  cp.round_1 = cp.is_compound ? COMPOUND_ROUND1_BITS : 2 * FILTER_BITS - r0;
  cp.dst = d16; cp.dst_stride = w;
  run_pred(pad, pstride, rw, rh, bd, w, h, fxt, fyt, 0, 0, sc_x, sc_y, x, y, ssx, ssy, mvx0, mvy0, mode, &cp, dst, w);
  if (mode) {
    cp.do_average = 1;
    cp.use_dist_wtd_comp_avg = mode == 2;
    cp.fwd_offset = fwd; cp.bck_offset = bck;
    run_pred(pad, pstride, rw, rh, bd, w, h, fxt, fyt, 0, 0, sc_x, sc_y, x, y, ssx, ssy, mvx1, mvy1, mode, &cp, dst, w);
  }
  uint32_t hsh = 2166136261u;
  for (int i = 0; i < w * h; i++) hsh = fnv(hsh, dst[i]);
  free(ref); free(pad); free(dst); free(d16);
  return hsh;
}

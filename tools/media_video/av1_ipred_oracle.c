#define PW 144
#define PH 144

static int64_t lcg_state;

static int lcg_rnd(int n) {
  lcg_state = (lcg_state * 1103515245 + 12345) & 2147483647;
  return (int)(((lcg_state >> 16) & 32767) % n);
}

static int lcg_big(int n) {
  int a = lcg_rnd(32768);
  int b = lcg_rnd(32768);
  return (int)(((int64_t)a * 32768 + b) % n);
}

static void fill_plane(uint16_t *p, int bd) {
  for (int i = 0; i < PW * PH; ++i) p[i] = (uint16_t)lcg_big(1 << bd);
}

static uint32_t hash_plane(const uint16_t *p) {
  uint32_t h = 2166136261u;
  for (int i = 0; i < PW * PH; ++i) h = (h ^ p[i]) * 16777619u;
  return h;
}

static int log2i(int n) {
  int l = 0;
  while ((1 << l) < n) ++l;
  return l;
}

static uint32_t run_pred(int bd, int ts, int mode, int angle_delta, int fm, int have_left, int have_above,
                         int have_tr, int have_bl, int xr, int yd, int edge_type, int edge_filter, int x,
                         int y, int64_t seed) {
  static uint16_t plane[PW * PH];
  lcg_state = seed;
  fill_plane(plane, bd);
  const int txw = tx_size_wide[ts];
  const int txh = tx_size_high[ts];
  uint16_t *ref = plane + y * PW + x;
  const int use_filter_intra = fm >= 0;
  const int is_dr = mode >= V_PRED && mode <= D67_PRED;
  const int n_top_px = have_above ? clamp(xr + txw, 0, txw) : 0;
  const int n_left_px = have_left ? clamp(yd + txh, 0, txh) : 0;
  if (!use_filter_intra && !is_dr) {
    highbd_build_non_directional_intra_predictors((const uint8_t *)ref, PW, (uint8_t *)ref, PW, mode, ts,
                                                  n_top_px, n_left_px, bd);
    return hash_plane(plane);
  }
  int p_angle = 0;
  int need_tr = extend_modes[mode] & NEED_ABOVERIGHT;
  int need_bl = extend_modes[mode] & NEED_BOTTOMLEFT;
  if (use_filter_intra) {
    need_tr = 0;
    need_bl = 0;
  }
  if (is_dr) {
    p_angle = mode_to_angle_map[mode] + angle_delta * 3;
    need_tr = p_angle < 90;
    need_bl = p_angle > 180;
  }
  const int htr = need_tr ? have_tr : -1;
  const int hbl = need_bl ? have_bl : -1;
  const int n_topright_px = htr > 0 ? clamp(xr, 0, txw) : htr;
  const int n_bottomleft_px = hbl > 0 ? clamp(yd, 0, txh) : hbl;
  highbd_build_directional_and_filter_intra_predictors(
      (const uint8_t *)ref, PW, (uint8_t *)ref, PW, mode, p_angle, use_filter_intra ? fm : FILTER_INTRA_MODES,
      ts, !edge_filter, n_top_px, n_topright_px, n_left_px, n_bottomleft_px, edge_type, bd);
  return hash_plane(plane);
}

static uint32_t run_cfl(int bd, int ts, int sx, int sy, int alpha, int vw, int vh, int lx, int ly,
                        int64_t seed) {
  static uint16_t luma[PW * PH];
  static uint16_t chroma[PW * PH];
  lcg_state = seed;
  fill_plane(luma, bd);
  fill_plane(chroma, bd);
  const int w = tx_size_wide[ts];
  const int h = tx_size_high[ts];
  CFL_CTX cfl;
  memset(&cfl, 0, sizeof(cfl));
  const uint16_t *in = luma + ly * PW + lx;
  if (sx && sy) {
    cfl_luma_subsampling_420_hbd_c(in, PW, cfl.recon_buf_q3, vw << sx, vh << sy);
  } else if (sx) {
    cfl_luma_subsampling_422_hbd_c(in, PW, cfl.recon_buf_q3, vw << sx, vh << sy);
  } else {
    cfl_luma_subsampling_444_hbd_c(in, PW, cfl.recon_buf_q3, vw, vh);
  }
  cfl.buf_width = vw;
  cfl.buf_height = vh;
  cfl_pad(&cfl, w, h);
  int16_t ac[CFL_BUF_SQUARE];
  subtract_average_c(cfl.recon_buf_q3, ac, w, h, (w * h) >> 1, log2i(w * h));
  uint16_t *dst = chroma + 4 * PW + 4;
  cfl_predict_hbd_c(ac, dst, PW, alpha, bd, w, h);
  return hash_plane(chroma) ^ (hash_plane(luma) * 31u);
}

int main(void) {
  init_intra_predictors_internal();
  int kind;
  while (scanf("%d", &kind) == 1) {
    if (kind == 0) {
      int bd, ts, mode, ad, fm, hl, ha, htr, hbl, xr, yd, et, ef, x, y;
      long long seed;
      if (scanf("%d %d %d %d %d %d %d %d %d %d %d %d %d %d %d %lld", &bd, &ts, &mode, &ad, &fm, &hl, &ha, &htr,
                &hbl, &xr, &yd, &et, &ef, &x, &y, &seed) != 16)
        return 2;
      printf("%u\n", run_pred(bd, ts, mode, ad, fm, hl, ha, htr, hbl, xr, yd, et, ef, x, y, seed));
    } else {
      int bd, ts, sx, sy, alpha, vw, vh, lx, ly;
      long long seed;
      if (scanf("%d %d %d %d %d %d %d %d %d %lld", &bd, &ts, &sx, &sy, &alpha, &vw, &vh, &lx, &ly, &seed) != 10)
        return 2;
      printf("%u\n", run_cfl(bd, ts, sx, sy, alpha, vw, vh, lx, ly, seed));
    }
  }
  return 0;
}

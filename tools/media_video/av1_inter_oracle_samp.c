static uint32_t case_samp(const int *p) {
  int mi_row = p[0], mi_col = p[1], bw4 = p[2], bh4 = p[3];
  int stride = bw4 + 2;
  int rows = bh4 + 1;
  MB_MODE_INFO *grid[64 * 40];
  MB_MODE_INFO cells[80];
  for (int i = 0; i < 64 * 40; i++) grid[i] = &cells[0];
  const int *c = p + 13;
  int ncell = bw4 + 2 + bh4;
  for (int i = 0; i < ncell; i++) {
    cells[i].ref_frame[0] = (int8_t)c[i * 5];
    cells[i].ref_frame[1] = (int8_t)c[i * 5 + 1];
    cells[i].bsize = c[i * 5 + 2];
    cells[i].mv[0].as_mv.row = (int16_t)c[i * 5 + 3];
    cells[i].mv[0].as_mv.col = (int16_t)c[i * 5 + 4];
  }
  MB_MODE_INFO **origin = &grid[stride + 1];
  for (int i = 0; i < bw4 + 2; i++) origin[-stride - 1 + i] = &cells[i];
  for (int r = 0; r < bh4; r++) origin[r * stride - 1] = &cells[bw4 + 2 + r];
  MB_MODE_INFO cur;
  memset(&cur, 0, sizeof(cur));
  cur.ref_frame[0] = (int8_t)p[9];
  cur.ref_frame[1] = NONE_FRAME;
  origin[0] = &cur;
  AV1_COMMON cm;
  cm.mi_params.mi_rows = p[4];
  cm.mi_params.mi_cols = p[5];
  MACROBLOCKD xd;
  memset(&xd, 0, sizeof(xd));
  xd.mi = origin;
  xd.mi_stride = stride;
  xd.mi_row = mi_row;
  xd.mi_col = mi_col;
  xd.width = bw4;
  xd.height = bh4;
  xd.up_available = p[6];
  xd.left_available = p[7];
  g_tr_ok = p[8];
  int pts[16], inref[16];
  memset(pts, 0, sizeof(pts));
  memset(inref, 0, sizeof(inref));
  int np = av1_findSamples(&cm, &xd, pts, inref);
  uint32_t hsh = 2166136261u;
  hsh = fnv(hsh, np);
  int np2 = np;
  if (np > 1) {
    MV mv; mv.row = (int16_t)p[10]; mv.col = (int16_t)p[11];
    np2 = av1_selectSamples(&mv, pts, inref, np, p[12]);
  }
  hsh = fnv(hsh, np2);
  for (int i = 0; i < np2 * 2; i++) { hsh = fnv(hsh, (uint32_t)pts[i]); hsh = fnv(hsh, (uint32_t)inref[i]); }
  return hsh;
}

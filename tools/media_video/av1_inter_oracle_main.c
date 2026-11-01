int main(int argc, char **argv) {
  int p[512];
  int kind;
  FILE *wf = fopen(argv[1], "rb");
  fseek(wf, 0, SEEK_END); long sz = ftell(wf); fseek(wf, 0, SEEK_SET);
  wedge_tab = malloc(sz); fread(wedge_tab, 1, sz, wf); fclose(wf);
  while (scanf("%d", &kind) == 1) {
    int n = 0;
    switch (kind) { case 0: n = 21; break; case 1: n = 2; break; case 2: n = 11; break; case 3: n = 6; break; case 4: n = 25; break; case 5: n = 39; break; case 6: n = 13; break; default: return 2; }
    for (int i = 0; i < n; i++) scanf("%d", &p[i]);
    if (kind == 6) { int extra = (p[2] + 2 + p[3]) * 5; for (int i = 0; i < extra; i++) scanf("%d", &p[n + i]); }
    uint32_t r = 0;
    if (kind == 0) r = case_mc(p);
    else if (kind == 1) r = case_dist(p);
    else if (kind == 2) r = case_mask(p);
    else if (kind == 3) r = case_obmc(p);
    else if (kind == 4) r = case_warp(p);
    else if (kind == 5) r = case_warpest(p);
    else r = case_samp(p);
    printf("%u\n", r);
  }
  return 0;
}

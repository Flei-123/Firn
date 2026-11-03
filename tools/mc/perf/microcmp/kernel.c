#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>

#define P 18
#define N 16
#define NSEC 64

static uint8_t grid[NSEC][P * P * P];
static uint16_t mask[N * N];

static uint32_t rng_next(uint32_t *s) {
    *s = *s * 1664525u + 1013904223u;
    return *s >> 8;
}

static void fill(int sec) {
    uint32_t s = 12345u + (uint32_t)sec * 7919u;
    int h[P * P];
    for (int i = 0; i < P * P; i++) h[i] = 4 + (int)(rng_next(&s) % 9);
    for (int pass = 0; pass < 3; pass++)
        for (int z = 1; z < P - 1; z++)
            for (int x = 1; x < P - 1; x++)
                h[z * P + x] = (h[z * P + x] * 2 + h[z * P + x - 1] + h[z * P + x + 1] + h[(z - 1) * P + x] + h[(z + 1) * P + x]) / 6;
    for (int y = 0; y < P; y++)
        for (int z = 0; z < P; z++)
            for (int x = 0; x < P; x++) {
                uint32_t r = rng_next(&s);
                uint8_t b = 0;
                if (y <= h[z * P + x]) b = (uint8_t)(1 + (r % 3));
                if (b && (r & 0xff0000) < 0x080000) b = 0;
                grid[sec][(y * P + z) * P + x] = b;
            }
}

static inline uint32_t ao_level(uint32_t s1, uint32_t s2, uint32_t c) {
    if (s1 && s2) return 0;
    return 3 - (s1 + s2 + c);
}

static uint64_t mesh(const uint8_t *g) {
    static const int dirs[6][3] = {{1, 0, 0}, {-1, 0, 0}, {0, 1, 0}, {0, -1, 0}, {0, 0, 1}, {0, 0, -1}};
    uint64_t hash = 2166136261ull;
    uint32_t quads = 0;
    for (int d = 0; d < 6; d++) {
        int ax = d >> 1;
        int u_ax = (ax + 1) % 3, v_ax = (ax + 2) % 3;
        int sgn = dirs[d][ax];
        for (int slice = 0; slice < N; slice++) {
            int any = 0;
            for (int v = 0; v < N; v++)
                for (int u = 0; u < N; u++) {
                    int c[3];
                    c[ax] = slice + 1;
                    c[u_ax] = u + 1;
                    c[v_ax] = v + 1;
                    int idx = (c[1] * P + c[2]) * P + c[0];
                    uint8_t b = g[idx];
                    uint16_t m = 0;
                    if (b) {
                        int n[3] = {c[0], c[1], c[2]};
                        n[ax] += sgn;
                        uint8_t nb = g[(n[1] * P + n[2]) * P + n[0]];
                        if (!nb) {
                            uint32_t s[8];
                            int k = 0;
                            for (int dv = -1; dv <= 1; dv++)
                                for (int du = -1; du <= 1; du++) {
                                    if (!du && !dv) continue;
                                    int q[3] = {n[0], n[1], n[2]};
                                    q[u_ax] += du;
                                    q[v_ax] += dv;
                                    s[k++] = g[(q[1] * P + q[2]) * P + q[0]] != 0;
                                }
                            uint32_t a0 = ao_level(s[3], s[1], s[0]);
                            uint32_t a1 = ao_level(s[4], s[1], s[2]);
                            uint32_t a2 = ao_level(s[3], s[6], s[5]);
                            uint32_t a3 = ao_level(s[4], s[6], s[7]);
                            m = (uint16_t)(1u | ((uint32_t)b << 1) | (a0 << 3) | (a1 << 5) | (a2 << 7) | (a3 << 9));
                            any = 1;
                        }
                    }
                    mask[v * N + u] = m;
                }
            if (!any) continue;
            for (int v = 0; v < N; v++) {
                for (int u = 0; u < N;) {
                    uint16_t m = mask[v * N + u];
                    if (!m) { u++; continue; }
                    int w = 1;
                    while (u + w < N && mask[v * N + u + w] == m) w++;
                    int h = 1;
                    int ok = 1;
                    while (v + h < N && ok) {
                        for (int k = 0; k < w; k++)
                            if (mask[(v + h) * N + u + k] != m) { ok = 0; break; }
                        if (ok) h++;
                    }
                    for (int dv = 0; dv < h; dv++)
                        for (int du = 0; du < w; du++) mask[(v + dv) * N + u + du] = 0;
                    quads++;
                    hash = ((hash ^ (uint64_t)(((uint32_t)m << 16) | ((uint32_t)u << 12) | ((uint32_t)v << 8) | ((uint32_t)w << 4) | (uint32_t)h)) * 16777619ull) & 0xffffffffull;
                    hash = ((hash ^ (uint64_t)(d * 16 + slice)) * 16777619ull) & 0xffffffffull;
                    u += w;
                }
            }
        }
    }
    return hash ^ quads;
}

static uint64_t now_ns(void) {
    struct timespec t;
    clock_gettime(CLOCK_MONOTONIC, &t);
    return (uint64_t)t.tv_sec * 1000000000ull + (uint64_t)t.tv_nsec;
}

int main(int argc, char **argv) {
    int reps = argc > 1 ? atoi(argv[1]) : 200;
    for (int s = 0; s < NSEC; s++) fill(s);
    uint64_t total = 0;
    for (int s = 0; s < NSEC; s++) total ^= mesh(grid[s]);
    uint64_t best = ~0ull;
    for (int r = 0; r < reps; r++) {
        uint64_t t0 = now_ns();
        uint64_t hh = 0;
        for (int s = 0; s < NSEC; s++) hh ^= mesh(grid[s]);
        uint64_t dt = now_ns() - t0;
        if (hh != total) return 1;
        if (dt < best) best = dt;
    }
    printf("RESULT lang=c hash=%llu best_ns_per_section=%llu\n", (unsigned long long)total, (unsigned long long)(best / NSEC));
    return 0;
}

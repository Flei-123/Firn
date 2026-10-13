// tools/stack_e/host.c -- C side of the way-E probe: dlopen the Firn .so, call every export, check the results.
#include <dlfcn.h>
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef struct { float x, y, z, w; } Vtx;
typedef struct { float x, y; } V2;
static int fails = 0, checks = 0;
#define CHECK(c, name) do { checks++; if (!(c)) { fails++; printf("FAIL %s\n", name); } } while (0)
static int64_t cb_sq(int64_t x) { return x * x; }

int main(int argc, char **argv) {
    void *h = dlopen(argv[1], RTLD_NOW | RTLD_LOCAL);
    if (!h) { printf("dlopen failed: %s\n", dlerror()); return 2; }
#define SYM(n, ret, args) ret (*n)args = (ret (*)args)dlsym(h, #n); if (!n) { printf("missing symbol %s\n", #n); return 3; }
    SYM(se_abi, uint64_t, (void))
    SYM(se_add_i32, int32_t, (int32_t, int32_t))
    SYM(se_axpy_f32, float, (float, float, float))
    SYM(se_len_f64, double, (double, double))
    SYM(se_vtx_scale, void, (Vtx *, float))
    SYM(se_vtx_dot, float, (Vtx *, Vtx *))
    SYM(se_v2_add, V2, (V2, V2))
    SYM(se_call_cb, int64_t, (int64_t (*)(int64_t), int64_t))
    SYM(se_count_inc, uint64_t, (void))
    SYM(se_fma4_f32, void, (float *, float *, float *, float *, size_t))
    SYM(se_mix_span, void, (uint32_t *, size_t, uint32_t, uint32_t))
    SYM(se_blend_span, void, (uint32_t *, size_t, uint32_t, uint32_t))

    CHECK(se_abi() == 1, "abi");
    CHECK(se_add_i32(-5, 12) == 7, "add_i32");
    CHECK(se_axpy_f32(2.0f, 3.5f, 1.0f) == 8.0f, "axpy_f32");
    CHECK(se_len_f64(3.0, 4.0) == 25.0, "len_f64");
    Vtx a = {1, 2, 3, 4}, b = {0.5f, 1, 2, 3};
    CHECK(se_vtx_dot(&a, &b) == 0.5f + 2 + 6 + 12, "vtx_dot");
    se_vtx_scale(&a, 2.0f);
    CHECK(a.x == 2 && a.y == 4 && a.z == 6 && a.w == 8, "vtx_scale");
    V2 r = se_v2_add((V2){1, 2}, (V2){10, 20});
    CHECK(r.x == 11 && r.y == 22, "v2_add");
    CHECK(se_call_cb(cb_sq, 10) == 285, "callback");
    CHECK(se_count_inc() == 1 && se_count_inc() == 2, "global counter");

    float x[8], y[8], z[8], o[8], ref[8];
    for (int i = 0; i < 8; i++) { x[i] = i + 0.5f; y[i] = 2 - i; z[i] = i * 0.25f; ref[i] = x[i] * y[i] + z[i]; }
    se_fma4_f32(o, x, y, z, 8);
    CHECK(memcmp(o, ref, sizeof ref) == 0, "fma4_f32");

    uint32_t px[37], px2[37];
    srand(7);
    int okmix = 1, okblend = 1;
    for (uint32_t f = 0; f <= 256; f += 5) {
        for (int i = 0; i < 37; i++) px[i] = px2[i] = ((uint32_t)rand() << 8) ^ (uint32_t)rand();
        uint32_t rgb = 0x00AA5533;
        se_mix_span(px, 37, rgb, f);
        for (int i = 0; i < 37; i++) {
            uint32_t p = px2[i], e = 0;
            for (int c = 0; c < 4; c++) {
                uint32_t cc = (p >> (8 * c)) & 255, k = c == 3 ? 255 : (rgb >> (8 * c)) & 255;
                e |= (((cc * (256 - f) + k * f) >> 8) & 255) << (8 * c);
            }
            if (px[i] != e) okmix = 0;
        }
    }
    for (uint32_t al = 0; al <= 255; al += 7) {
        for (int i = 0; i < 37; i++) px[i] = px2[i] = ((uint32_t)rand() << 8) ^ (uint32_t)rand();
        uint32_t rgb = 0x00AA5533;
        se_blend_span(px, 37, rgb, al);
        for (int i = 0; i < 37; i++) {
            uint32_t p = px2[i], e = 0;
            for (int c = 0; c < 4; c++) {
                uint32_t cc = (p >> (8 * c)) & 255, k = c == 3 ? 255 : (rgb >> (8 * c)) & 255;
                e |= (((cc * (255 - al) + k * al) / 255) & 255) << (8 * c);
            }
            if (px[i] != e) okblend = 0;
        }
    }
    CHECK(okmix, "mix_span vs C");
    CHECK(okblend, "blend_span vs C");
    printf("C host: %d checks, %d failures\n", checks, fails);
    return fails ? 1 : 0;
}

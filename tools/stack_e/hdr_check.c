// tools/stack_e/hdr_check.c -- the header written by `firnc --header` must compile and carry the exact prototypes.
#include "lib_e.h"

_Static_assert(sizeof(Vtx) == 16, "Vtx layout");
_Static_assert(sizeof(V2) == 8, "V2 layout");

static uint64_t (*p_abi)(void) = se_abi;
static int32_t (*p_add)(int32_t, int32_t) = se_add_i32;
static float (*p_axpy)(float, float, float) = se_axpy_f32;
static double (*p_len)(double, double) = se_len_f64;
static void (*p_scale)(Vtx *, float) = se_vtx_scale;
static float (*p_dot)(Vtx *, Vtx *) = se_vtx_dot;
static V2 (*p_v2)(V2, V2) = se_v2_add;
static int64_t (*p_cb)(uint64_t, int64_t) = se_call_cb;
static void (*p_mix)(uint64_t, size_t, uint32_t, uint32_t) = se_mix_span;

int main(void) { return (p_abi && p_add && p_axpy && p_len && p_scale && p_dot && p_v2 && p_cb && p_mix) ? 0 : 1; }

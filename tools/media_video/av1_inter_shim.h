#ifndef AV1_INTER_SHIM_H
#define AV1_INTER_SHIM_H
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <limits.h>
#include <math.h>
#define ATTRIBUTE_PACKED
#define UENUM1BYTE(x)
#define DECLARE_ALIGNED(n, typ, val) typ val
#define AOM_FORCE_INLINE inline
#define AOM_INLINE inline
#define FILTER_BITS 7
#define SUBPEL_BITS 4
#define SUBPEL_MASK 15
#define SUBPEL_SHIFTS 16
#define SUBPEL_TAPS 8
#define SCALE_SUBPEL_BITS 10
#define SCALE_SUBPEL_SHIFTS 1024
#define SCALE_SUBPEL_MASK 1023
#define SCALE_EXTRA_BITS 6
#define SCALE_EXTRA_OFF 32
#define DIST_PRECISION_BITS 4
#define MAX_SB_SIZE 128
#define COMPOUND_ROUND1_BITS 7
#define AOM_BLEND_A64_ROUND_BITS 6
#define AOM_BLEND_A64_MAX_ALPHA 64
#define DIFF_FACTOR 16
#define ROUND_POWER_OF_TWO(value, n) (((value) + (((1 << (n)) >> 1))) >> (n))
#define ROUND_POWER_OF_TWO_SIGNED(value, n) (((value) < 0) ? -ROUND_POWER_OF_TWO(-(value), (n)) : ROUND_POWER_OF_TWO((value), (n)))
#define AOMMIN(x, y) (((x) < (y)) ? (x) : (y))
#define AOMMAX(x, y) (((x) > (y)) ? (x) : (y))
#define AOMABS(x) abs(x)
#define AOM_BLEND_A64(a, v0, v1) ROUND_POWER_OF_TWO((a) * (v0) + (AOM_BLEND_A64_MAX_ALPHA - (a)) * (v1), AOM_BLEND_A64_ROUND_BITS)
typedef uint16_t CONV_BUF_TYPE;
typedef struct ConvolveParams {
  int do_average;
  CONV_BUF_TYPE *dst;
  int dst_stride;
  int round_0;
  int round_1;
  int plane;
  int is_compound;
  int use_dist_wtd_comp_avg;
  int fwd_offset;
  int bck_offset;
} ConvolveParams;
typedef int16_t InterpKernel[SUBPEL_TAPS];
static inline int clamp(int value, int low, int high) { return value < low ? low : (value > high ? high : value); }
static inline int64_t clamp64(int64_t value, int64_t low, int64_t high) { return value < low ? low : (value > high ? high : value); }
static inline uint16_t clip_pixel_highbd(int val, int bd) {
  switch (bd) {
    case 8: return (uint16_t)clamp(val, 0, 255);
    case 10: return (uint16_t)clamp(val, 0, 1023);
    default: return (uint16_t)clamp(val, 0, 4095);
  }
}
static inline int get_msb(unsigned int n) { return 31 - __builtin_clz(n); }
static inline int get_msb64(uint64_t n) { return 63 - __builtin_clzll(n); }
#endif
#ifndef AV1_INTER_SHIM_EXTRA
#define AV1_INTER_SHIM_EXTRA
#define CONVERT_TO_SHORTPTR(x) ((uint16_t *)(((uintptr_t)(x)) << 1))
#define CONVERT_TO_BYTEPTR(x) ((uint8_t *)(((uintptr_t)(x)) >> 1))
#define ROUND_POWER_OF_TWO_64(value, n) (((value) + ((((int64_t)1 << (n)) >> 1))) >> (n))
#define ROUND_POWER_OF_TWO_SIGNED_64(value, n) (((value) < 0) ? -ROUND_POWER_OF_TWO_64(-(value), (n)) : ROUND_POWER_OF_TWO_64((value), (n)))
#define WARPEDMODEL_PREC_BITS 16
#define WARPEDMODEL_TRANS_CLAMP (128 << WARPEDMODEL_PREC_BITS)
#define WARPEDMODEL_NONDIAGAFFINE_CLAMP (1 << (WARPEDMODEL_PREC_BITS - 3))
#define WARPEDPIXEL_PREC_BITS 6
#define WARPEDPIXEL_PREC_SHIFTS (1 << WARPEDPIXEL_PREC_BITS)
#define WARP_PARAM_REDUCE_BITS 6
#define WARPEDDIFF_PREC_BITS (WARPEDMODEL_PREC_BITS - WARPEDPIXEL_PREC_BITS)
#define MAX_SB_SIZE_LOG2 7
#define LEAST_SQUARES_SAMPLES_MAX_BITS 3
#define IMPLIES(a, b) (!(a) || (b))
#define MI_SIZE 4
static inline int negative_to_zero(int value) { return value & ~(value >> (sizeof(value) * 8 - 1)); }
#define CONFIG_AV1_HIGHBITDEPTH 1
#define AOM_BLEND_AVG(v0, v1) ROUND_POWER_OF_TWO((v0) + (v1), 1)
static inline uint8_t clip_pixel(int val) { return (uint8_t)clamp(val, 0, 255); }
typedef int16_t WarpedFilterCoeff;
typedef enum { IDENTITY = 0, TRANSLATION = 1, ROTZOOM = 2, AFFINE = 3 } TransformationType;
typedef struct { TransformationType wmtype; int32_t wmmat[8]; int16_t alpha, beta, gamma, delta; int8_t invalid; } WarpedMotionParams;
typedef enum { DIFFWTD_38 = 0, DIFFWTD_38_INV = 1 } DIFFWTD_MASK_TYPE;
static const int quant_dist_weight[4][2] = { { 2, 3 }, { 2, 5 }, { 2, 7 }, { 1, 31 } };
static const int quant_dist_lookup_table[4][2] = { { 9, 7 }, { 11, 5 }, { 12, 4 }, { 13, 3 } };
#endif

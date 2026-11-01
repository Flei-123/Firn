#ifndef AV1_IPRED_SHIM_H
#define AV1_IPRED_SHIM_H
#include <assert.h>
#include <math.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define CONFIG_AV1_HIGHBITDEPTH 1
#define CONFIG_REALTIME_ONLY 0
#define CONFIG_AV1_DECODER 1
#define DECLARE_ALIGNED(n, typ, val) typ val
#define MAX_TX_SIZE 64
#define FILTER_INTRA_SCALE_BITS 4
#define ROUND_POWER_OF_TWO(value, n) (((value) + (((1 << (n)) >> 1))) >> (n))
#define ROUND_POWER_OF_TWO_SIGNED(value, n) \
  (((value) < 0) ? -ROUND_POWER_OF_TWO(-(value), (n)) : ROUND_POWER_OF_TWO((value), (n)))
#define AOMMIN(x, y) (((x) < (y)) ? (x) : (y))
#define AOMMAX(x, y) (((x) > (y)) ? (x) : (y))
#define OD_MAXI(a, b) ((a) > (b) ? (a) : (b))
#define CONVERT_TO_SHORTPTR(x) ((uint16_t *)(x))
#define NELEMENTS(x) ((int)(sizeof(x) / sizeof((x)[0])))
#define CFL_BUF_LINE 32
#define CFL_BUF_SQUARE (CFL_BUF_LINE * CFL_BUF_LINE)
#define av1_highbd_dr_prediction_z1 av1_highbd_dr_prediction_z1_c
#define av1_highbd_dr_prediction_z2 av1_highbd_dr_prediction_z2_c
#define av1_highbd_dr_prediction_z3 av1_highbd_dr_prediction_z3_c
#define av1_highbd_filter_intra_edge av1_highbd_filter_intra_edge_c
#define av1_highbd_upsample_intra_edge av1_highbd_upsample_intra_edge_c

typedef enum {
  DC_PRED, V_PRED, H_PRED, D45_PRED, D135_PRED, D113_PRED, D157_PRED, D203_PRED, D67_PRED,
  SMOOTH_PRED, SMOOTH_V_PRED, SMOOTH_H_PRED, PAETH_PRED, INTRA_MODES
} PREDICTION_MODE;

typedef enum {
  FILTER_DC_PRED, FILTER_V_PRED, FILTER_H_PRED, FILTER_D157_PRED, FILTER_PAETH_PRED, FILTER_INTRA_MODES
} FILTER_INTRA_MODE;

typedef enum {
  TX_4X4, TX_8X8, TX_16X16, TX_32X32, TX_64X64, TX_4X8, TX_8X4, TX_8X16, TX_16X8, TX_16X32,
  TX_32X16, TX_32X64, TX_64X32, TX_4X16, TX_16X4, TX_8X32, TX_32X8, TX_16X64, TX_64X16, TX_SIZES_ALL
} TX_SIZE;

static const int tx_size_wide[TX_SIZES_ALL] = { 4, 8, 16, 32, 64, 4, 8, 8, 16, 16, 32, 32, 64, 4, 16, 8, 32, 16, 64 };
static const int tx_size_high[TX_SIZES_ALL] = { 4, 8, 16, 32, 64, 8, 4, 16, 8, 32, 16, 64, 32, 16, 4, 32, 8, 64, 16 };
static const int mode_to_angle_map[INTRA_MODES] = { 0, 90, 180, 45, 135, 113, 157, 203, 67, 0, 0, 0, 0 };

typedef void (*intra_pred_fn)(uint8_t *dst, ptrdiff_t stride, const uint8_t *above, const uint8_t *left);

typedef struct {
  uint16_t recon_buf_q3[CFL_BUF_SQUARE];
  int buf_width;
  int buf_height;
} CFL_CTX;

static inline int av1_is_directional_mode(PREDICTION_MODE mode) { return mode >= V_PRED && mode <= D67_PRED; }
static inline int clamp(int value, int low, int high) { return value < low ? low : (value > high ? high : value); }
static inline uint16_t clip_pixel_highbd(int val, int bd) {
  switch (bd) {
    case 8: return (uint16_t)clamp(val, 0, 255);
    case 10: return (uint16_t)clamp(val, 0, 1023);
    default: return (uint16_t)clamp(val, 0, 4095);
  }
}
static inline void aom_memset16(void *dest, int val, size_t length) {
  uint16_t *d = (uint16_t *)dest;
  for (size_t i = 0; i < length; i++) d[i] = (uint16_t)val;
}
#endif

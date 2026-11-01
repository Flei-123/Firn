typedef struct MV { int16_t row, col; } MV;
typedef union int_mv { uint32_t as_int; MV as_mv; } int_mv;
typedef struct MB_MODE_INFO { int bsize; int8_t ref_frame[2]; int_mv mv[2]; } MB_MODE_INFO;
typedef struct TileInfo { int unused; } TileInfo;
typedef struct MACROBLOCKD { MB_MODE_INFO **mi; int mi_stride; int mi_row, mi_col; int width, height; int up_available, left_available; TileInfo tile; } MACROBLOCKD;
typedef struct MiParams { int mi_rows, mi_cols; } MiParams;
typedef struct AV1_COMMON { MiParams mi_params; } AV1_COMMON;
typedef struct POSITION { int row, col; } POSITION;
#define NONE_FRAME (-1)
#define LEAST_SQUARES_SAMPLES_MAX 8
#define GET_MV_SUBPEL(x) ((x) * 8)
static int g_tr_ok;
static int has_top_right(const AV1_COMMON *cm, const MACROBLOCKD *xd, int mi_row, int mi_col, int bs) { return g_tr_ok; }
static int is_inside(const TileInfo *tile, int mi_col, int mi_row, const POSITION *mi_pos) { return 1; }

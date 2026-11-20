#ifndef GLUE_H
#define GLUE_H
typedef struct pt { int x; int y; } pt;
#define ADD3(a, b, c) ((a) + (b) + (c))
#define MAXI(a, b) ((a) > (b) ? (a) : (b))
static inline int twice(int x) { return x * 2; }
static inline int dot(const pt *a, const pt *b) { return a->x * b->x + a->y * b->y; }
#endif

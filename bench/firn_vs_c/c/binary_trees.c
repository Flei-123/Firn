// Binary-trees allocation benchmark, max depth 16 (stretch tree 17), min depth 4, step 4.
// malloc/free per node; the Firn side uses the idiomatic GC variant (gc class, no manual free).
// Firn counterpart: bench/firn_vs_c/firn/binary_trees.fi
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef uint8_t u8;
typedef uint32_t u32;
typedef uint64_t u64;

static inline u64 lcg(u64 s) { return s * 6364136223846793005ULL + 1442695040888963407ULL; }

typedef struct Tree { struct Tree *left, *right; } Tree;

static Tree *build(u32 d) {
    Tree *t = (Tree *)malloc(sizeof(Tree));
    if (d == 0) {
        t->left = NULL;
        t->right = NULL;
        return t;
    }
    Tree *l = build(d - 1);
    Tree *r = build(d - 1);
    t->left = l;
    t->right = r;
    return t;
}

static u64 check(const Tree *t) {
    if (t->left == NULL) return 1;
    return 1 + check(t->left) + check(t->right);
}

static void release(Tree *t) {
    if (t->left != NULL) {
        release(t->left);
        release(t->right);
    }
    free(t);
}

int main(void) {
    u32 min_depth = 4, max_depth = 16;
    u64 total = 0;
    Tree *stretch = build(max_depth + 1);
    total += check(stretch);
    release(stretch);
    Tree *long_lived = build(max_depth);
    for (u32 d = min_depth; d <= max_depth; d += 4) {
        u64 iters = 1ULL << (max_depth - d + min_depth);
        u64 chk = 0;
        for (u64 i = 0; i < iters; i++) {
            Tree *t = build(d);
            chk += check(t);
            release(t);
        }
        total = total * 1000003ULL + chk;
    }
    total = total * 1000003ULL + check(long_lived);
    release(long_lived);
    printf("%llu\n", (unsigned long long)total);
    return 0;
}

// Scalar SHA-256 (own implementation, no libc) over 16 MiB of LCG data.
// Output: first 8 digest bytes as a big-endian u64.
// Firn counterpart: bench/firn_vs_c/firn/sha256.fi
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef uint8_t u8;
typedef uint32_t u32;
typedef uint64_t u64;

static inline u64 lcg(u64 s) { return s * 6364136223846793005ULL + 1442695040888963407ULL; }

static const u32 K[64] = {
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2
};
static u32 H[8];
static u32 W[64];

static inline u32 rotr(u32 x, u32 n) { return (x >> n) | (x << (32 - n)); }

/* processes one 64-byte block at p + off */
static void block(const u8 *p, size_t off) {
    size_t t = 0;
    while (t < 16) {
        size_t o = off + t * 4;
        W[t] = ((u32)p[o] << 24) | ((u32)p[o + 1] << 16) | ((u32)p[o + 2] << 8) | (u32)p[o + 3];
        t++;
    }
    while (t < 64) {
        u32 w15 = W[t - 15];
        u32 w2 = W[t - 2];
        u32 s0 = rotr(w15, 7) ^ rotr(w15, 18) ^ (w15 >> 3);
        u32 s1 = rotr(w2, 17) ^ rotr(w2, 19) ^ (w2 >> 10);
        W[t] = W[t - 16] + s0 + W[t - 7] + s1;
        t++;
    }
    u32 a = H[0], b = H[1], c = H[2], d = H[3], e = H[4], f = H[5], g = H[6], h = H[7];
    for (t = 0; t < 64; t++) {
        u32 bs1 = rotr(e, 6) ^ rotr(e, 11) ^ rotr(e, 25);
        u32 ch = (e & f) ^ ((e ^ 0xFFFFFFFFu) & g);
        u32 t1 = h + bs1 + ch + K[t] + W[t];
        u32 bs0 = rotr(a, 2) ^ rotr(a, 13) ^ rotr(a, 22);
        u32 maj = (a & b) ^ (a & c) ^ (b & c);
        u32 t2 = bs0 + maj;
        h = g; g = f; f = e; e = d + t1; d = c; c = b; b = a; a = t1 + t2;
    }
    H[0] += a; H[1] += b; H[2] += c; H[3] += d; H[4] += e; H[5] += f; H[6] += g; H[7] += h;
}

int main(void) {
    H[0] = 0x6a09e667; H[1] = 0xbb67ae85; H[2] = 0x3c6ef372; H[3] = 0xa54ff53a;
    H[4] = 0x510e527f; H[5] = 0x9b05688c; H[6] = 0x1f83d9ab; H[7] = 0x5be0cd19;
    size_t n = 16777216;
    u8 *buf = (u8 *)calloc(n, 1);
    u64 s = 3;
    for (size_t i = 0; i < n; i++) {
        s = lcg(s);
        buf[i] = (u8)(s >> 56);
    }
    for (size_t off = 0; off < n; off += 64) block(buf, off);
    /* padding block: 0x80, zeros, 64-bit big-endian bit length (n is a multiple of 64) */
    u8 *pad = (u8 *)calloc(64, 1);
    pad[0] = 128;
    u64 bits = (u64)n * 8;
    for (size_t k = 0; k < 8; k++) pad[56 + k] = (u8)((bits >> ((7 - k) * 8)) & 255);
    block(pad, 0);
    printf("%llu\n", (unsigned long long)(((u64)H[0] << 32) | (u64)H[1]));
    return 0;
}

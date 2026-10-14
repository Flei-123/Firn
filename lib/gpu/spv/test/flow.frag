#version 450
// lib/gpu/spv/test/flow.frag -- control flow, function calls, local arrays and structs for tests/2922.
layout(location = 0) flat in int n;
layout(location = 1) in float x;
layout(location = 0) out int sum;
layout(location = 1) out int collatz;
layout(location = 2) out float fl;
layout(location = 3) out int arr;
layout(location = 4) out int sw;
layout(location = 5) out float st;

struct P { float a; int b; vec2 c; };

int tri(int k) {
    int s = 0;
    for (int i = 1; i <= k; i++) {
        if (i % 7 == 0) continue;
        s += i;
        if (s > 100000) break;
    }
    return s;
}

float poly(float t, float u) {
    return t * t - 2.0 * u + (t > u ? 1.0 : -1.0);
}

int steps(int v) {
    int c = 0;
    while (v != 1 && c < 1000) {
        v = (v & 1) == 1 ? 3 * v + 1 : v / 2;
        c++;
    }
    return c;
}

void main() {
    sum = tri(n);
    collatz = steps(n);
    fl = poly(x, 1.5) + poly(2.0 * x, x);
    int tmp[5];
    for (int i = 0; i < 5; i++) tmp[i] = i * i + n;
    arr = tmp[n % 5] + tmp[4];
    switch (n % 4) {
        case 0: sw = 10; break;
        case 1: sw = 20; break;
        case 3: sw = 40; break;
        default: sw = 99; break;
    }
    P p = P(x, n, vec2(x, 2.0));
    p.c.y += p.a;
    p.b *= 2;
    st = p.a + float(p.b) + p.c.x * p.c.y;
}

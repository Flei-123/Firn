// Benchmark: own quicksort (median-of-3, Hoare partition, insertion sort for
// small ranges) on 2,000,000 u32 values from an LCG.
// Checksum = sum of a[i] * (i + 1), wrapping u64.
// Counterpart: bench/firn_vs_c/firn/qsort.fi
#include <stdio.h>
#include <stdint.h>

#define N 2000000

static uint32_t A[N];

static void insertion(int64_t lo, int64_t hi) {
    for (int64_t i = lo + 1; i <= hi; i++) {
        uint32_t v = A[i];
        int64_t j = i - 1;
        while (j >= lo && A[j] > v) {
            A[j + 1] = A[j];
            j--;
        }
        A[j + 1] = v;
    }
}

static void quicksort(int64_t lo, int64_t hi) {
    while (hi - lo > 16) {
        int64_t mid = lo + (hi - lo) / 2;
        if (A[mid] < A[lo]) {
            uint32_t t = A[mid]; A[mid] = A[lo]; A[lo] = t;
        }
        if (A[hi] < A[lo]) {
            uint32_t t = A[hi]; A[hi] = A[lo]; A[lo] = t;
        }
        if (A[hi] < A[mid]) {
            uint32_t t = A[hi]; A[hi] = A[mid]; A[mid] = t;
        }
        uint32_t pivot = A[mid];
        int64_t i = lo;
        int64_t j = hi;
        while (i <= j) {
            while (A[i] < pivot) i++;
            while (A[j] > pivot) j--;
            if (i <= j) {
                uint32_t t = A[i]; A[i] = A[j]; A[j] = t;
                i++;
                j--;
            }
        }
        if (j - lo < hi - i) {
            quicksort(lo, j);
            lo = i;
        } else {
            quicksort(i, hi);
            hi = j;
        }
    }
    insertion(lo, hi);
}

int main(void) {
    uint32_t x = 12345;
    for (size_t i = 0; i < N; i++) {
        x = x * 1664525u + 1013904223u;
        A[i] = x;
    }
    quicksort(0, N - 1);
    uint64_t sum = 0;
    for (size_t k = 0; k < N; k++) {
        if (k > 0 && A[k - 1] > A[k]) return 1;
        sum += (uint64_t)A[k] * (uint64_t)(k + 1);
    }
    printf("%llu\n", (unsigned long long)sum);
    return 0;
}

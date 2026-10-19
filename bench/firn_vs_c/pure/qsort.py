import os
import sys

S = int(os.environ.get("PYN_SCALE", "1") or 1)
N = 2_000_000 // S
M64 = 0xFFFFFFFFFFFFFFFF
A = [0] * N


def insertion(lo, hi):
    for i in range(lo + 1, hi + 1):
        v = A[i]
        j = i - 1
        while j >= lo and A[j] > v:
            A[j + 1] = A[j]
            j -= 1
        A[j + 1] = v


def quicksort(lo, hi):
    while hi - lo > 16:
        mid = lo + (hi - lo) // 2
        if A[mid] < A[lo]:
            A[mid], A[lo] = A[lo], A[mid]
        if A[hi] < A[lo]:
            A[hi], A[lo] = A[lo], A[hi]
        if A[hi] < A[mid]:
            A[hi], A[mid] = A[mid], A[hi]
        pivot = A[mid]
        i = lo
        j = hi
        while i <= j:
            while A[i] < pivot:
                i += 1
            while A[j] > pivot:
                j -= 1
            if i <= j:
                A[i], A[j] = A[j], A[i]
                i += 1
                j -= 1
        if j - lo < hi - i:
            quicksort(lo, j)
            lo = i
        else:
            quicksort(i, hi)
            hi = j
    insertion(lo, hi)


x = 12345
for i in range(N):
    x = (x * 1664525 + 1013904223) & 0xFFFFFFFF
    A[i] = x
quicksort(0, N - 1)
total = 0
for k in range(N):
    if k > 0 and A[k - 1] > A[k]:
        sys.exit(1)
    total = (total + A[k] * (k + 1)) & M64
print(total)

/* SPDX-License-Identifier: GPL-2.0-only */
/* tools/extfn/host.c -- direction 2's driver: an ordinary C program that
 * calls a Firn function it never saw the source of. */
extern long add_one(long x);
int main(void) {
    long r = add_one(41);
    return (int)r;
}

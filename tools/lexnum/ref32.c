/* SPDX-License-Identifier: GPL-2.0-only */
/* tools/lexnum/ref32.c -- the yardstick from OUTSIDE for round 71.
 *
 * Reads one decimal literal per line and writes the BIT PATTERN of the
 * FLOAT that C `strtof` makes of it. glibc rounds correctly and rounds
 * DIRECTLY -- it does not take the detour through a double, which is
 * exactly what makes it the right yardstick here: Firn reads the correctly
 * rounded binary64 and narrows it once, and the claim that this gives the
 * same result is a theorem (Figueroa 1995) that wants measuring.
 *
 * It is a MEASURING INSTRUMENT, not a dependency: nothing in the compiler
 * and nothing in the standard library of Firn uses it.
 *
 * Build:  cc -O2 -o ref32 tools/lexnum/ref32.c
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* long enough for a subnormal written out in full (about 1080 digits) */
static char line[65536];

int main(void)
{
    while (fgets(line, sizeof(line), stdin) != NULL) {
        size_t n = strlen(line);
        while (n > 0 && (line[n - 1] == '\n' || line[n - 1] == '\r')) {
            line[--n] = '\0';
        }
        if (n == 0) {
            continue;
        }
        float value = strtof(line, NULL);
        unsigned int pattern;
        memcpy(&pattern, &value, sizeof(pattern));
        printf("%u\n", pattern);
    }
    return 0;
}

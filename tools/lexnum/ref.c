/* SPDX-License-Identifier: GPL-2.0-only */
/* tools/lexnum/ref.c -- the yardstick from OUTSIDE for round 65.
 *
 * Reads one decimal literal per line and writes the BIT PATTERN of the
 * double that C `strtod` makes of it. glibc rounds correctly (it computes
 * with big numbers when it has to), so this is an independent third opinion
 * next to the two lexers of the project -- written in another language,
 * with another algorithm, by other people.
 *
 * It is a MEASURING INSTRUMENT, not a dependency: nothing in the compiler
 * and nothing in the standard library of Firn uses it.
 *
 * Build:  cc -O2 -o ref tools/lexnum/ref.c
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
        double value = strtod(line, NULL);
        unsigned long long pattern;
        memcpy(&pattern, &value, sizeof(pattern));
        printf("%llu\n", pattern);
    }
    return 0;
}

/* SPDX-License-Identifier: MPL-2.0 */
/* tools/desktop/argv_dump.c -- the Microsoft C runtime's idea of a command line.
 * Writes the arguments it received (as the CRT's own argv parser split them, program name excluded),
 * each followed by a NUL octet, as UTF-8 into the file named by the environment variable OUT. */
#include <stdio.h>
#include <stdlib.h>
#include <windows.h>

int wmain(int argc, wchar_t **argv) {
    wchar_t *out = _wgetenv(L"OUT");
    if (!out) return 2;
    FILE *f = _wfopen(out, L"wb");
    if (!f) return 3;
    for (int i = 1; i < argc; i++) {
        char buf[16384];
        int n = WideCharToMultiByte(CP_UTF8, 0, argv[i], -1, buf, sizeof buf, NULL, NULL);
        if (n > 0) fwrite(buf, 1, n, f); /* n includes the terminating NUL */
        else fputc(0, f);
    }
    fclose(f);
    return 0;
}

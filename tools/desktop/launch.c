/* SPDX-License-Identifier: MPL-2.0 */
/* tools/desktop/launch.c -- what the shell does with a Run key value: CreateProcess(NULL, <value>).
 * launch.exe <command line as ONE argument>; exit code = the child's, 250 = could not start. */
#include <windows.h>
#include <stdlib.h>

int wmain(int argc, wchar_t **argv) {
    if (argc < 2) return 2;
    STARTUPINFOW si; PROCESS_INFORMATION pi;
    ZeroMemory(&si, sizeof si); si.cb = sizeof si;
    wchar_t *line = _wcsdup(argv[1]);
    if (!CreateProcessW(NULL, line, NULL, NULL, FALSE, 0, NULL, NULL, &si, &pi)) return 250;
    WaitForSingleObject(pi.hProcess, 30000);
    DWORD code = 251;
    GetExitCodeProcess(pi.hProcess, &code);
    return (int)code;
}

/* tools/aarch64/qemu_exec_probe.c -- can a guest start ANOTHER guest program?
 *
 * Under qemu-user a program is an aarch64 file that a x86-64 kernel cannot
 * run. `execve` of such a file only works when the host has `binfmt_misc`
 * set up for aarch64 (a machine with qemu-user-static installed from a
 * distribution package usually has; a plain `apt install qemu-user` does
 * not). Cases that start THEMSELVES as a child process (tests/2061 and
 * tests/2064, `std.process`) cannot run without it -- and that is a fact
 * about the runner, not about the code generator.
 *
 * This is the proof, written in C on purpose: no Firn, no firnc. The program
 * forks, the child `execve`s this very file with an argument, and the
 * parent reports whether the child ran. The output line names the fact
 * (`foreign-exec: yes|no`); the exit code is 0 for "yes". */
#include <stdio.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>

int main(int argc, char **argv) {
    if (argc > 1 && strcmp(argv[1], "child") == 0) return 0;
    pid_t p = fork();
    if (p < 0) { printf("foreign-exec: no (fork failed)\n"); return 2; }
    if (p == 0) {
        char *args[] = { argv[0], "child", 0 };
        execv(argv[0], args);
        _exit(77);
    }
    int st = 0;
    waitpid(p, &st, 0);
    int ok = WIFEXITED(st) && WEXITSTATUS(st) == 0;
    printf("foreign-exec: %s\n", ok ? "yes" : "no");
    return ok ? 0 : 1;
}

/* demos/kernel/user/user.ld -- round K1: the shape of a program on the
 * disk.
 *
 * The kernel's ELF loader (`demos/kernel/elf.fi`) does not accept just
 * any file. It demands three things of a segment, and this script is what
 * produces them:
 *
 *   1. EVERY PT_LOAD LIES IN PAGES OF ITS OWN. Two segments that share a
 *      page cannot have different rights -- the page table has one entry
 *      per page and not one per octet. A loader that quietly gives the
 *      shared page the union of both rights has silently made the data
 *      writable AND executable. The `. = ALIGN(4096)` between the
 *      segments is therefore not cosmetics, it is what makes W^X
 *      checkable, and the loader refuses a file without it.
 *   2. THE THREE KINDS OF PAGE ARE REALLY THREE SEGMENTS: code read and
 *      execute, constants read only, data read and write. `statics.rs`
 *      puts a `static` without `mut` in `.rodata`, a `static mut` with a
 *      value in `.data` and an all-zero `static mut` in `.bss` -- so the
 *      language already sorts them, and this script only has to keep them
 *      apart.
 *   3. THE PROGRAM LIES WHERE THE PROCESS HAS ROOM. 0x40100000 is
 *      `elf.IMAGE_BASE`: above the private data page (0x40000000) and the
 *      eight stack pages (up to 0x40009000), below the end of the private
 *      region (0x40200000). `tools/osum/run.sh` links one program at
 *      0x50000000 on purpose -- the loader has to refuse it.
 *
 * ENTRY is `_start` out of crt.s; the Firn function under it is named
 * with `--defsym=USER_ENTRY=...` at link time, because firnc0 and firnc1
 * spell the same function `_F0.u_start` and `_F1.u_start`.
 */

ENTRY(_start)

PHDRS
{
    /* FILEHDR PHDRS: the ELF header and the program headers travel INSIDE
     * the first segment. Without that they get a page of their own and
     * every program on the disk is 4096 octets of nothing bigger -- which
     * matters, because a file in this file system stops at 12 + 64 blocks
     * (38912 octets, `demos/kernel/fs.fi`) and the shell built by firnc1
     * was over it. It also means a program can read its own ELF header,
     * the way a program on any other system can. */
    text   PT_LOAD FILEHDR PHDRS FLAGS(5);   /* R + X */
    rodata PT_LOAD FLAGS(4);   /* R     */
    data   PT_LOAD FLAGS(6);   /* R + W */
}

SECTIONS
{
    . = 0x40100000 + SIZEOF_HEADERS;

    .text : {
        *(.text)
        *(.text.*)
    } :text

    . = ALIGN(4096);
    .rodata : {
        *(.rodata)
        *(.rodata.*)
    } :rodata

    . = ALIGN(4096);
    .data : {
        *(.data)
        *(.data.*)
    } :data
    .bss : {
        *(.bss)
        *(.bss.*)
        *(COMMON)
    } :data

    /DISCARD/ : {
        *(.comment)
        *(.note.*)
        *(.eh_frame)
    }
}

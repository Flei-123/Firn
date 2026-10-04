import re
d = '/root/firn-wt-db/lib/db/'

helpers = '''
// Local copies of the byte helpers: the compiler does not inline across modules.
#[inline]
fn b8(p: u64, off: usize) -> u64 {
    return (*((p + off as u64) as *mut u8)) as u64
}

#[inline]
fn b16(p: u64, off: usize) -> u64 {
    return ((*((p + off as u64) as *mut u8)) as u64) << 8 | ((*((p + (off + 1) as u64) as *mut u8)) as u64)
}

#[inline]
fn b32(p: u64, off: usize) -> u64 {
    return ((*((p + off as u64) as *mut u8)) as u64) << 24 | ((*((p + (off + 1) as u64) as *mut u8)) as u64) << 16
        | ((*((p + (off + 2) as u64) as *mut u8)) as u64) << 8 | ((*((p + (off + 3) as u64) as *mut u8)) as u64)
}

#[inline]
fn w8(p: u64, off: usize, v: u64) {
    *((p + off as u64) as *mut u8) = (v & 255) as u8
}

#[inline]
fn w16(p: u64, off: usize, v: u64) {
    *((p + off as u64) as *mut u8) = ((v >> 8) & 255) as u8
    *((p + (off + 1) as u64) as *mut u8) = (v & 255) as u8
}

#[inline]
fn w32(p: u64, off: usize, v: u64) {
    *((p + off as u64) as *mut u8) = ((v >> 24) & 255) as u8
    *((p + (off + 1) as u64) as *mut u8) = ((v >> 16) & 255) as u8
    *((p + (off + 2) as u64) as *mut u8) = ((v >> 8) & 255) as u8
    *((p + (off + 3) as u64) as *mut u8) = (v & 255) as u8
}
'''

b = open(d + 'btree.fi').read()
for a, c in [('util.rd8(', 'b8('), ('util.rd16(', 'b16('), ('util.rd32(', 'b32('), ('util.wr8(', 'w8('), ('util.wr16(', 'w16('), ('util.wr32(', 'w32(')]:
    b = b.replace(a, c)
b = b.replace('const MAX_DEPTH: usize = 20\n', 'const MAX_DEPTH: usize = 20\n' + helpers, 1)
b = b.replace("pager.pager_usable((*bt).pg)", "usable_of(bt)")
b = b.replace("// ------------------------------------------------------------ cell lists", '''#[inline]
fn usable_of(bt: *mut Bt) -> usize {
    return (*(*bt).pg).usable
}

// ------------------------------------------------------------ cell lists''', 1)
for fn in ['cl_len', 'cl_ptr', 'cl_cost', 'cell_off', 'max_local_of', 'min_local_of', 'cur_new']:
    b = re.sub(r'\nfn ' + fn + r'\(', '\n#[inline]\nfn ' + fn + '(', b, count=1)
open(d + 'btree.fi', 'w').write(b)

r = open(d + 'record.fi').read()
r = r.replace('util.rd64(', 'r64(')
r = r.replace('const MAX_COLUMNS: usize = 2000\n', '''const MAX_COLUMNS: usize = 2000

#[inline]
fn r64(p: u64, off: usize) -> u64 {
    var v: u64 = 0
    var i: usize = 0
    while i < 8 {
        v = (v << 8) | ((*((p + (off + i) as u64) as *mut u8)) as u64)
        i = i + 1
    }
    return v
}

#[inline]
fn r8(p: u64, i: usize) -> u64 {
    return (*((p + i as u64) as *mut u8)) as u64
}
''', 1)
r = r.replace("rt.ld8(p, 0) >= 128 as u8", "r8(p, 0) >= 128")
r = r.replace("x = (x << 8) | (rt.ld8(p, i) as u64)", "x = (x << 8) | r8(p, i)")
for fn in ['val_null', 'val_int', 'val_real', 'val_text', 'val_blob', 'serial_len', 'f64_of_bits', 'bits_of_f64']:
    r = re.sub(r'\nfn ' + fn + r'\(', '\n#[inline]\nfn ' + fn + '(', r, count=1)
open(d + 'record.fi', 'w').write(r)

p = open(d + 'pager.fi').read()
p = p.replace('util.rd32(', 'g32(').replace('util.wr32(', 'p32(').replace('util.rd8(', 'g8(').replace('util.rd16(', 'g16(')
p = p.replace('util.wr8(', 'p8(').replace('util.wr16(', 'p16(')
p = p.replace('const FR_DIRTY: u32 = 1\n', '''#[inline]
fn g8(p: u64, off: usize) -> u64 {
    return (*((p + off as u64) as *mut u8)) as u64
}

#[inline]
fn g16(p: u64, off: usize) -> u64 {
    return ((*((p + off as u64) as *mut u8)) as u64) << 8 | ((*((p + (off + 1) as u64) as *mut u8)) as u64)
}

#[inline]
fn g32(p: u64, off: usize) -> u64 {
    return ((*((p + off as u64) as *mut u8)) as u64) << 24 | ((*((p + (off + 1) as u64) as *mut u8)) as u64) << 16
        | ((*((p + (off + 2) as u64) as *mut u8)) as u64) << 8 | ((*((p + (off + 3) as u64) as *mut u8)) as u64)
}

#[inline]
fn p8(p: u64, off: usize, v: u64) {
    *((p + off as u64) as *mut u8) = (v & 255) as u8
}

#[inline]
fn p16(p: u64, off: usize, v: u64) {
    *((p + off as u64) as *mut u8) = ((v >> 8) & 255) as u8
    *((p + (off + 1) as u64) as *mut u8) = (v & 255) as u8
}

#[inline]
fn p32(p: u64, off: usize, v: u64) {
    *((p + off as u64) as *mut u8) = ((v >> 24) & 255) as u8
    *((p + (off + 1) as u64) as *mut u8) = ((v >> 16) & 255) as u8
    *((p + (off + 2) as u64) as *mut u8) = ((v >> 8) & 255) as u8
    *((p + (off + 3) as u64) as *mut u8) = (v & 255) as u8
}

// native-endian frame fields (frame i of a table at `base`: unit = 4 octets)
#[inline]
fn f32g(base: u64, i: usize) -> u32 {
    return *((base + (i * 4) as u64) as *mut u32)
}

#[inline]
fn f32s(base: u64, i: usize, v: u32) {
    *((base + (i * 4) as u64) as *mut u32) = v
}

#[inline]
fn f64g(base: u64, i: usize) -> u64 {
    return *((base + (i * 8) as u64) as *mut u64)
}

#[inline]
fn f64s(base: u64, i: usize, v: u64) {
    *((base + (i * 8) as u64) as *mut u64) = v
}

const FR_DIRTY: u32 = 1
''', 1)
p = p.replace('rt.ld32(', 'f32g(').replace('rt.st32(', 'f32s(').replace('rt.ld64(fa, 2)', 'f64g(fa, 2)').replace('rt.st64(fa, 2,', 'f64s(fa, 2,')
for fn in ['fr_addr', 'fr_data', 'frame_find', 'pager_usable', 'pager_page_size', 'pager_page_count', 'pager_next_op', 'pager_in_write']:
    p = re.sub(r'\nfn ' + fn + r'\(', '\n#[inline]\nfn ' + fn + '(', p, count=1)
open(d + 'pager.fi', 'w').write(p)
print('ok31')

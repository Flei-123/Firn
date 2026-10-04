# SPDX-License-Identifier: MPL-2.0
"""A SquashFS 4.0 writer in plain Python (zlib for the blocks): what an AppImage
holds. No mksquashfs.

Scope, on purpose small: directories, regular files, symbolic links; one
owner (root); no fragments (the tail of a file is a short block of its own),
no xattrs, no export table, metadata blocks stored uncompressed (the format
allows it per block), data blocks deflated when that is smaller. It makes
images that `unsquashfs`, the kernel and the AppImage runtime read.

Layout written (offsets are what the superblock holds):

    superblock (96) | data blocks | inode table | directory table | id table
                    | (zero padding to 4096, not counted in bytes_used)

Directories and their inodes are written children first (like mksquashfs),
the root last, so every directory entry already knows the inode reference of
its children and every directory inode already knows where its listing is.
"""

import struct
import zlib

from . import common

MAGIC = 0x73717368
BLOCK = 131072
BLOCK_LOG = 17
META = 8192

T_DIR, T_FILE, T_SYMLINK = 1, 2, 3
FLAGS = 0x0001 | 0x0010 | 0x0200      # uncompressed inodes, no fragments, no xattrs


class Node(object):
    def __init__(self, kind, mode, data=None):
        self.kind = kind          # 'd' 'f' 'l'
        self.mode = mode
        self.data = data          # bytes (file), str (link target)
        self.children = {}        # name -> Node (dirs)


def tree_from_entries(entries):
    """entries: (path, kind, mode, data) with kind 'd' 'f' 'l'. Parents are
    created as 0o755 directories when missing."""
    root = Node('d', 0o755)
    for path, kind, mode, data in entries:
        parts = [p for p in path.strip("/").split("/") if p]
        if not parts:
            continue
        cur = root
        for p in parts[:-1]:
            nxt = cur.children.get(p)
            if nxt is None:
                nxt = Node('d', 0o755)
                cur.children[p] = nxt
            elif nxt.kind != 'd':
                raise common.PackError("%s: %s is not a directory" % (path, p))
            cur = nxt
        name = parts[-1]
        if kind == 'd' and name in cur.children and cur.children[name].kind == 'd':
            cur.children[name].mode = mode
            continue
        cur.children[name] = Node(kind, mode, data)
    return root


def _meta_blocks(table):
    """Cut a table into metadata blocks (header u16 with bit 15 = stored)."""
    out = bytearray()
    for i in range(0, len(table), META):
        chunk = table[i:i + META]
        out += struct.pack("<H", len(chunk) | 0x8000)
        out += chunk
    return bytes(out)


def _ref(pos):
    """position in an uncompressed table -> (block start, offset in block)"""
    return (pos // META) * (META + 2), pos % META


def build(root, mtime=None, comp_level=9):
    mtime = common.epoch() if mtime is None else mtime
    data_out = bytearray()
    inodes = bytearray()
    dirtab = bytearray()
    counter = [0]
    data_base = 96

    def write_file_blocks(content):
        start = data_base + len(data_out)
        sizes = []
        for i in range(0, len(content), BLOCK):
            chunk = content[i:i + BLOCK]
            z = zlib.compress(chunk, comp_level)
            if len(z) < len(chunk):
                data_out.extend(z)
                sizes.append(len(z))
            else:
                data_out.extend(chunk)
                sizes.append(len(chunk) | (1 << 24))
        return start, sizes

    # --- number the inodes: children before parents, root last
    order = []

    def number(node):
        if node.kind == 'd':
            for name in sorted(node.children, key=lambda s: s.encode("utf-8")):
                number(node.children[name])
        counter[0] += 1
        node.ino = counter[0]
        order.append(node)

    number(root)

    def set_parents(node, parent):
        node.parent_ino = parent.ino if parent is not None else node.ino
        if node.kind == 'd':
            for c in node.children.values():
                set_parents(c, node)
    set_parents(root, None)

    def type_id(node):
        return {'d': T_DIR, 'f': T_FILE, 'l': T_SYMLINK}[node.kind]

    def hdr(node):
        return struct.pack("<HHHHII", type_id(node), node.mode & 0o7777, 0, 0, mtime, node.ino)

    def write(node):
        if node.kind == 'd':
            names = sorted(node.children, key=lambda s: s.encode("utf-8"))
            for n in names:
                write(node.children[n])
            # the listing
            list_pos = len(dirtab)
            entries = []
            for n in names:
                c = node.children[n]
                entries.append((n, c))
            i = 0
            listing = bytearray()
            while i < len(entries):
                # one header per run of entries in the same inode metablock (max 256)
                first_blk = _ref(entries[i][1].ref)[0]
                j = i
                while (j < len(entries) and j - i < 256 and _ref(entries[j][1].ref)[0] == first_blk):
                    j += 1
                base_ino = entries[i][1].ino
                listing += struct.pack("<III", j - i - 1, first_blk, base_ino)
                for n, c in entries[i:j]:
                    nb = n.encode("utf-8")
                    listing += struct.pack("<HhHH", _ref(c.ref)[1], c.ino - base_ino, type_id(c), len(nb) - 1)
                    listing += nb
                i = j
            dirtab.extend(listing)
            blk, off = _ref(list_pos)
            node.ref = len(inodes)
            inodes.extend(hdr(node))
            inodes.extend(struct.pack("<IIHHI", blk, len(node.children) + 2, len(listing) + 3, off,
                                      node.parent_ino))
        elif node.kind == 'f':
            content = node.data or b""
            start, sizes = write_file_blocks(content) if content else (0, [])
            node.ref = len(inodes)
            inodes.extend(hdr(node))
            inodes.extend(struct.pack("<IIII", start, 0xFFFFFFFF, 0, len(content)))
            for s in sizes:
                inodes.extend(struct.pack("<I", s))
        else:
            target = node.data.encode("utf-8")
            node.ref = len(inodes)
            inodes.extend(hdr(node))
            inodes.extend(struct.pack("<II", 1, len(target)))
            inodes.extend(target)

    write(root)
    inode_blob = _meta_blocks(bytes(inodes))
    dir_blob = _meta_blocks(bytes(dirtab))
    data_end = data_base + len(data_out)
    inode_start = data_end
    dir_start = inode_start + len(inode_blob)
    id_meta = _meta_blocks(struct.pack("<I", 0))
    id_blocks_start = dir_start + len(dir_blob)
    id_index_start = id_blocks_start + len(id_meta)
    id_index = struct.pack("<Q", id_blocks_start)
    total = id_index_start + len(id_index)
    rblk, roff = _ref(root.ref)
    NONE = 0xFFFFFFFFFFFFFFFF
    sb = struct.pack("<IIIIIHHHHHHQQQQQQQQ", MAGIC, counter[0], mtime, BLOCK, 0, 1, BLOCK_LOG, FLAGS,
                     1, 4, 0, (rblk << 16) | roff, total, id_index_start, NONE,
                     inode_start, dir_start, NONE, NONE)
    assert len(sb) == 96, len(sb)
    img = bytearray(sb) + data_out + inode_blob + dir_blob + id_meta + id_index
    if len(img) % 4096:
        img += b"\0" * (4096 - len(img) % 4096)
    return bytes(img)

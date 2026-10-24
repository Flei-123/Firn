import os
import struct


class Reader:
    def __init__(self, data, pos=0):
        self.data = data
        self.pos = pos

    def u8(self):
        value = self.data[self.pos]
        self.pos += 1
        return value

    def s8(self):
        value = self.u8()
        return value - 256 if value > 127 else value

    def u16(self):
        value = struct.unpack_from("<H", self.data, self.pos)[0]
        self.pos += 2
        return value

    def u32(self):
        value = struct.unpack_from("<I", self.data, self.pos)[0]
        self.pos += 4
        return value

    def u64(self):
        value = struct.unpack_from("<Q", self.data, self.pos)[0]
        self.pos += 8
        return value

    def uint(self, size):
        value = int.from_bytes(self.data[self.pos:self.pos + size], "little")
        self.pos += size
        return value

    def uleb(self):
        result = 0
        shift = 0
        while True:
            byte = self.u8()
            result |= (byte & 0x7F) << shift
            shift += 7
            if not byte & 0x80:
                return result

    def sleb(self):
        result = 0
        shift = 0
        while True:
            byte = self.u8()
            result |= (byte & 0x7F) << shift
            shift += 7
            if not byte & 0x80:
                if byte & 0x40:
                    result -= 1 << shift
                return result

    def cstr(self):
        end = self.data.index(b"\0", self.pos)
        text = self.data[self.pos:end].decode(errors="replace")
        self.pos = end + 1
        return text


def parse_unit(reader, offset):
    length = reader.u32()
    wide = length == 0xFFFFFFFF
    if wide:
        length = reader.u64()
    end = reader.pos + length
    version = reader.u16()
    if version < 2 or version > 4:
        raise ValueError(f"unsupported .debug_line version {version}")
    header_length = reader.u64() if wide else reader.u32()
    program = reader.pos + header_length
    min_inst = reader.u8()
    if version >= 4:
        reader.u8()
    reader.u8()
    line_base = reader.s8()
    line_range = reader.u8()
    opcode_base = reader.u8()
    lengths = [reader.u8() for _ in range(opcode_base - 1)]
    dirs = []
    while True:
        text = reader.cstr()
        if not text:
            break
        dirs.append(text)
    files = [None]
    while True:
        name = reader.cstr()
        if not name:
            break
        dir_index = reader.uleb()
        reader.uleb()
        reader.uleb()
        files.append(join_path(dirs, dir_index, name))
    reader.pos = program
    rows = run_program(reader, end, files, min_inst, line_base, line_range, opcode_base, lengths)
    return {"offset": offset, "files": files, "rows": rows}, end


def join_path(dirs, dir_index, name):
    if os.path.isabs(name) or dir_index == 0 or dir_index > len(dirs):
        return os.path.normpath(name)
    return os.path.normpath(os.path.join(dirs[dir_index - 1], name))


def run_program(reader, end, files, min_inst, line_base, line_range, opcode_base, lengths):
    rows = []
    addr = 0
    file_index = 1
    line = 1

    def emit(stop):
        if 0 < file_index < len(files):
            rows.append((addr, files[file_index], line, stop))

    while reader.pos < end:
        opcode = reader.u8()
        if opcode >= opcode_base:
            adjusted = opcode - opcode_base
            addr += (adjusted // line_range) * min_inst
            line += line_base + adjusted % line_range
            emit(False)
        elif opcode == 0:
            size = reader.uleb()
            stop = reader.pos + size
            kind = reader.u8()
            if kind == 1:
                emit(True)
                addr = 0
                file_index = 1
                line = 1
            elif kind == 2:
                addr = reader.uint(size - 1)
            elif kind == 3:
                name = reader.cstr()
                dir_index = reader.uleb()
                files.append(join_path([], dir_index, name))
            reader.pos = stop
        elif opcode == 1:
            emit(False)
        elif opcode == 2:
            addr += reader.uleb() * min_inst
        elif opcode == 3:
            line += reader.sleb()
        elif opcode == 4:
            file_index = reader.uleb()
        elif opcode == 5:
            reader.uleb()
        elif opcode == 8:
            addr += ((255 - opcode_base) // line_range) * min_inst
        elif opcode == 9:
            addr += reader.u16()
        elif opcode == 12:
            reader.uleb()
        elif opcode in (6, 7, 10, 11):
            pass
        else:
            for _ in range(lengths[opcode - 1]):
                reader.uleb()
    return rows


def parse_line_section(data):
    units = []
    reader = Reader(data)
    while reader.pos < len(data):
        offset = reader.pos
        unit, end = parse_unit(reader, offset)
        units.append(unit)
        reader.pos = end
    return units


def ranges_of(unit):
    out = []
    rows = unit["rows"]
    for index, (addr, path, line, stop) in enumerate(rows):
        if stop or line == 0 or index + 1 >= len(rows):
            continue
        following = rows[index + 1][0]
        if following > addr:
            out.append((addr, following, path, line))
    return out

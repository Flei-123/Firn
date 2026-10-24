import struct


class Elf:
    def __init__(self, path):
        with open(path, "rb") as handle:
            self.data = handle.read()
        if self.data[:4] != b"\x7fELF" or self.data[4] != 2 or self.data[5] != 1:
            raise ValueError(f"{path}: not a 64-bit little-endian ELF file")
        shoff = struct.unpack_from("<Q", self.data, 0x28)[0]
        shentsize, shnum, shstrndx = struct.unpack_from("<HHH", self.data, 0x3A)
        headers = []
        for index in range(shnum):
            base = shoff + index * shentsize
            name, kind, flags, addr, offset, size = struct.unpack_from("<IIQQQQ", self.data, base)
            headers.append((name, kind, offset, size))
        names_offset = headers[shstrndx][2] if headers else 0
        self.sections = {}
        self.order = []
        for name, kind, offset, size in headers:
            end = self.data.index(b"\0", names_offset + name)
            label = self.data[names_offset + name:end].decode()
            self.sections[label] = (kind, offset, size)
            self.order.append(label)

    def section(self, name):
        entry = self.sections.get(name)
        if entry is None or entry[0] == 8:
            return b""
        return self.data[entry[1]:entry[1] + entry[2]]

    def function_symbols(self):
        symtab = self.section(".symtab")
        strtab = self.section(".strtab")
        if ".text" not in self.order:
            return []
        text_index = self.order.index(".text")
        out = []
        for base in range(0, len(symtab) - 23, 24):
            name, info, other, shndx, value, size = struct.unpack_from("<IBBHQQ", symtab, base)
            if shndx != text_index or info & 0xF > 2:
                continue
            end = strtab.index(b"\0", name)
            label = strtab[name:end].decode(errors="replace")
            if label:
                out.append((value, label))
        return sorted(set(out))

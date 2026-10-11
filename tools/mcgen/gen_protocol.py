#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""gen_protocol.py -- minecraft-data protocol.json -> a Firn module.

usage: gen_protocol.py <minecraft-data-data-dir> <version> <out.fi>
       gen_protocol.py --all <minecraft-data-data-dir> <lib/mc dir> <version,version,...|all>
       e.g. gen_protocol.py --all .../minecraft-data/data lib/mc all

Version = data (GDD 8.6): nothing about packet numbers or field layouts is typed
by hand. A new game version is this script run with another version string.

WHAT IS GENERATED (see the header of the produced file, too):

  * constants <STATE>_<DIR>_<NAME> (packet ids), PROTOCOL_VERSION, ST_*, DIR_*;
  * packet_name(state, dir, id) -> u64  : pointer to a NUL terminated name or 0;
  * packet_schema(state, dir, id) -> u64: handle for schema.walk or 0;
  * ONE blob (a `const str` literal) that holds, for every packet, a compact
    "schema program": a bytecode that describes the field layout of the packet
    body. The runtime walker (lib/mc/mc_schema.fi) interprets it.

WHY A STRING LITERAL. Measured (see tools/mcclient/README or the round report):
a 60 KB `const str` compiles in ~0.15 s, a function that fills an array word by
word is an order of magnitude slower in the compiler. The blob only contains
octets below 0x80 (the string literal must be valid UTF-8), so every operand is
a "6 bit group" varint: byte = (value & 0x3f) | (0x40 if more groups follow),
LOW group first. Fixed width fields (call offsets, table entries) are groups
of 7 bit/6 bit as documented below.

BYTECODE (must match lib/mc/mc_schema.fi, which holds the same table):

  u(x)      varint, 6 data bits per octet (bit 6 = more), low group first
  sv(x)     zigzag + u
  name      u(len) then len octets
  reg1      u(register + 1), 0 = "value not stored"
  block     u(len) then len octets of ops (so the walker can skip it)

  Every "type op" below is one op; a FIELD is  O_FIELD name reg1 <type op>.

  0  END         (not emitted inside programs; reserved)
  1  FIELD       name reg1 <type op>      (empty name = anonymous field)
  2  SCALAR      kind reg1                (kind: see K_* below)
  3  VOID
  4  CONTAINER   block-of-FIELDs
  5  BITFIELD    nbytes nsub { size signed reg1 name }*
  6  BITFLAGS    kind nflags { bit reg1 name }*
  7  STRING      kind reg1                (kind = type of the length prefix)
  8  BUFFER      ckind carg               (ckind: K_* prefix | CK_CONST carg=n
                                           | CK_REG carg=register)
  9  RESTBUF
  10 ARRAY       ckind carg minsz block-with-ONE-type-op
  11 OPTION      block-with-ONE-type-op
  12 SWITCH      reg ncases { key64 block }* defblock    (key = u(), 64 bit)
  13 CALL        4 fixed groups: zigzag(target - end_of_operand), 6 bits each
  14 TOPBIT      block-with-ONE-type-op
  15 EMETA       endval block-with-ONE-type-op
  16 NBT         mode (0 named, 1 anonymous, 2 optional named, 3 optional anon)
  17 REGHOLDER   basename othername block-with-ONE-type-op   (registryEntryHolder:
                 varint n; n != 0 -> registry id n-1, n == 0 -> the inline type)
  18 REGSET      basename othername minsz block(base type) block(element type)
                 (registryEntryHolderSet: varint n; n == 0 -> the base type (a tag
                 name), n != 0 -> n-1 elements of the element type)
  20 RCALL       same operand as CALL, used where a type refers back to itself
                 (a slot component that holds a slot, 1.20.5+); the walker saves and
                 restores its register file around the call
  19 LPVEC3      (low precision Vec3 of 1.21.9+: u8 a; a == 0 -> done (zero vector);
                 else u8, u32 and, if bit 2 of a is set, a varint)

  K_ (scalar kinds): 0 u8, 1 i8, 2 u16, 3 i16, 4 u32, 5 i32, 6 u64, 7 i64,
  8 f32, 9 f64, 10 bool, 11 varint, 12 varlong, 13 uuid (16 octets).

BLOB LAYOUT:
  [0..8)   magic "FSC1" + format byte + 3 zero octets
  [8..)    subroutines (shared named types: slot, particle, ...)
           packet programs
           packet names (NUL terminated)
           tables, one per (state, dir): for id 0..max: 6 octets, 7 bits each,
           name offset (3 octets, big endian) and program offset (3 octets);
           offset 0 = no such packet.

switch keys are resolved at GENERATION time: "true"/"false" -> 1/0, decimal
numbers as they are, mapper names back to their numeric id, string keys to the
64 bit FNV-1a hash of the text (the walker stores the same hash for a string
field that a switch compares against). compareTo paths ("flags/x", "../action/y",
"$compareTo") are resolved to a register number, never at run time.
"""
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import gen_registry  # noqa: E402
import gen_entity  # noqa: E402

FORMAT = 2

K = dict(u8=0, i8=1, u16=2, i16=3, u32=4, i32=5, u64=6, i64=7, f32=8, f64=9,
         bool=10, varint=11, varlong=12, UUID=13)
K_VARINT = 11
CK_CONST = 32
CK_REG = 33

(O_END, O_FIELD, O_SCALAR, O_VOID, O_CONTAINER, O_BITFIELD, O_BITFLAGS,
 O_STRING, O_BUFFER, O_RESTBUF, O_ARRAY, O_OPTION, O_SWITCH, O_CALL, O_TOPBIT,
 O_EMETA, O_NBT, O_REGHOLDER, O_REGSET, O_LPVEC3, O_RCALL) = range(21)

SCALAR_SIZE = dict(u8=1, i8=1, u16=2, i16=2, u32=4, i32=4, u64=8, i64=8, f32=4,
                   f64=8, bool=1, varint=1, varlong=1, UUID=16)

STATES = [("handshaking", "HANDSHAKE"), ("status", "STATUS"), ("login", "LOGIN"),
          ("play", "PLAY"), ("configuration", "CONFIG")]
DIRS = [("toServer", "C2S"), ("toClient", "S2C")]

REG_MAX = 128


def fnv64(text):
    h = 0xcbf29ce484222325
    for b in text.encode("utf-8"):
        h ^= b
        h = (h * 0x100000001b3) & 0xFFFFFFFFFFFFFFFF
    return h


# ----------------------------------------------------------------- emitter
class Frag:
    """A piece of bytecode plus the CALL fixups inside it."""

    def __init__(self):
        self.buf = bytearray()
        self.fix = []  # (offset of the 4 octet operand, subroutine key)

    def u(self, x):
        assert x >= 0
        while True:
            g = x & 0x3F
            x >>= 6
            if x:
                self.buf.append(g | 0x40)
            else:
                self.buf.append(g)
                return

    def op(self, o):
        self.buf.append(o)

    def name(self, s):
        raw = s.encode("utf-8")
        assert all(c < 0x80 for c in raw), "non ASCII name " + s
        self.u(len(raw))
        self.buf += raw

    def sub(self, other):
        base = len(self.buf)
        self.buf += other.buf
        self.fix += [(o + base, k) for (o, k) in other.fix]

    def block(self, other):
        self.u(len(other.buf))
        self.sub(other)

    def call(self, key):
        self.fix.append((len(self.buf), key))
        self.buf += b"\0\0\0\0"


# ---------------------------------------------------------------------- IR
class Var:
    """A value some later switch/array/buffer may refer to."""

    def __init__(self, kind="other", rev=None):
        self.kind = kind  # int | bool | str | map | other
        self.rev = rev  # mapper: name -> numeric id
        self.used = False
        self.reg = None


class Scope:
    def __init__(self, parent):
        self.parent = parent
        self.names = {}  # name -> Var | dict(subname -> Var)


class Gen:
    def __init__(self, proto):
        self.types = proto["types"]
        self.local = {}
        self.subs = {}  # key -> IR (None while being built)
        self.sub_order = []
        self.vars = []
        self.stack = []  # recursion guard for subroutine construction
        self.warnings = []  # data inconsistencies that were worked around

    # ---- type lookup
    def lookup(self, name):
        if name in self.local:
            return self.local[name]
        return self.types[name]

    # ---- the builder: type -> IR tuple
    def build(self, t, scope, arg):
        """Returns (ir, var_kind_info). `arg` is the Var a "$compareTo" means."""
        if isinstance(t, str):
            return self.build_named(t, None, scope, arg)
        head, a = t[0], t[1]
        if head == "container":
            return self.build_container(a, scope, arg)
        if head == "switch":
            return self.build_switch(a, scope, arg), Var()
        if head == "array":
            return self.build_array(a, scope, arg), Var()
        if head == "option":
            body, _ = self.build(a, scope, arg)
            return ("option", body), Var()
        if head == "mapper":
            kind = a["type"]
            rev = {}
            for k, v in a["mappings"].items():
                rev[v] = int(k, 0)
            return ("scalar", K[kind]), Var("map", rev)
        if head == "bitfield":
            return self.build_bitfield(a), "bits"
        if head == "bitflags":
            return self.build_bitflags(a), "bits"
        if head == "pstring":
            if "countType" not in a:
                raise ValueError("pstring without countType: %r" % (a,))
            return ("string", K[a["countType"]]), Var("str")
        if head == "buffer":
            if "countType" in a:
                return ("buffer", K[a["countType"]], 0), Var()
            return self.count_ref("buffer", a["count"], scope), Var()
        if head in ("topBitSetTerminatedArray",):
            body, _ = self.build(a["type"], scope, arg)
            return ("topbit", body), Var()
        if head == "entityMetadataLoop":
            body, _ = self.build(a["type"], scope, arg)
            return ("emeta", int(a["endVal"]), body), Var()
        if head == "registryEntryHolder":
            ot = a["otherwise"]
            body, _ = self.build(ot["type"], scope, arg)
            return ("regholder", a["baseName"], ot["name"], body), Var()
        if head == "registryEntryHolderSet":
            # varint n; n == 0: `base` (e.g. a tag name); else n-1 elements of `otherwise`
            base, _ = self.build(a["base"]["type"], scope, arg)
            elem, _ = self.build(a["otherwise"]["type"], scope, arg)
            return ("regset", a["base"]["name"], a["otherwise"]["name"], min_size(elem, self), base, elem), Var()
        if head in self.types or head in self.local:
            # a parametrized named type: ["particleData", {"compareTo": "particleId"}]
            return self.build_named(head, a, scope, arg)
        raise ValueError("unsupported type head %r" % (head,))

    def count_ref(self, what, count, scope):
        if isinstance(count, int):
            return (what, CK_CONST, count)
        var = self.resolve_path(scope, count)
        var.used = True
        return (what, CK_REG, var)

    def build_named(self, name, params, scope, arg):
        native_scalars = ("varint", "varlong", "u8", "u16", "u32", "u64", "i8", "i16",
                          "i32", "i64", "bool", "f32", "f64")
        if name in native_scalars:
            return ("scalar", K[name]), Var("bool" if name == "bool" else "int")
        if name == "UUID":
            return ("scalar", K["UUID"]), Var()
        if name == "void":
            return ("void",), Var()
        if name == "restBuffer":
            return ("restbuf",), Var()
        if name == "lpVec3":
            return ("lpvec3",), Var()
        if name in ("nbt", "optionalNbt", "anonymousNbt", "anonOptionalNbt"):
            mode = dict(nbt=0, anonymousNbt=1, optionalNbt=2, anonOptionalNbt=3)[name]
            return ("nbt", mode), Var()
        d = self.lookup(name)
        if d == "native":
            raise ValueError("native type %r is not handled" % name)
        if isinstance(d, str):  # an alias (optvarint -> varint)
            return self.build_named(d, params, scope, arg)
        head = d[0]
        if head in ("pstring", "buffer", "mapper", "bitfield", "bitflags",
                    "registryEntryHolder", "registryEntryHolderSet"):
            return self.build(d, scope, arg)  # small: inline
        # composite: a shared subroutine. A "$compareTo" parameter makes an instance
        # per argument variable.
        argvar = None
        if params and "compareTo" in params:
            argvar = self.resolve_path(scope, params["compareTo"])
            argvar.used = True
        key = (name, id(argvar) if argvar is not None else 0)
        if key in self.subs and self.subs[key] is None:
            # a type that refers back to itself (still under construction): the walker
            # recurses (depth-limited, registers saved/restored around the call)
            return ("rcall", key), Var()
        if key not in self.subs:
            self.subs[key] = None
            self.stack.append(name)
            try:
                ir, _ = self.build(d, Scope(None), argvar)
            except ValueError as e:
                if "[in " not in str(e):
                    raise ValueError("%s [in %s]" % (e, "/".join(self.stack))) from None
                raise
            self.stack.pop()
            self.subs[key] = ir
            self.sub_order.append(key)
        return ("call", key), Var()

    def build_container(self, fields, scope, arg):
        sc = Scope(scope)
        items = []
        for f in fields:
            name = f.get("name")
            ir, info = self.build(f["type"], sc, arg)
            if f.get("anon"):
                if info == "bits":
                    for sn, sv in ir_subvars(ir).items():
                        sc.names[sn] = sv
                elif getattr(info, "scope_names", None) is not None:
                    # protodef semantics: the fields of an anonymous container are
                    # visible to the siblings that follow it (1.12.2 entity metadata:
                    # {key, type} then a switch on "type")
                    for sn, sv in info.scope_names.items():
                        sc.names[sn] = sv
                else:
                    pass  # an anonymous non-bitfield: nothing can refer to its names
                items.append(("field", "", None, ir))
                continue
            var = None
            if info == "bits":
                sc.names[name] = ir_subvars(ir)
            else:
                var = info
                sc.names[name] = var
                self.vars.append(var)
            items.append(("field", name, var, ir))
        cv = Var()
        cv.scope_names = sc.names
        return ("container", items), cv

    def build_bitfield(self, a):
        subs = []
        total = 0
        for f in a:
            total += f["size"]
            v = Var("int")
            self.vars.append(v)
            subs.append((f["name"], f["size"], 1 if f.get("signed") else 0, v))
        assert total % 8 == 0 and total <= 64, "bitfield size"
        return ("bitfield", total // 8, subs)

    def build_bitflags(self, a):
        flags = a["flags"]
        if not isinstance(flags, list):
            raise ValueError("bitflags with a dict are not used by 1.20.1")
        subs = []
        for bit, n in enumerate(flags):
            v = Var("bool")
            self.vars.append(v)
            subs.append((n, bit, v))
        return ("bitflags", K[a["type"]], subs)

    def build_array(self, a, scope, arg):
        body, _ = self.build(a["type"], scope, arg)
        minsz = min_size(body, self)
        if "countType" in a:
            return ("array", K[a["countType"]], 0, minsz, body)
        ref = self.count_ref("array", a["count"], scope)
        return ("array", ref[1], ref[2], minsz, body)

    def resolve_path(self, scope, path):
        parts = path.split("/")
        sc = scope
        i = 0
        while i < len(parts) and parts[i] == "..":
            sc = sc.parent
            i += 1
            if sc is None:
                raise ValueError("compareTo %r climbs above the root" % path)
        if i == 0 and parts[0] not in sc.names:
            # protodef compiles a nested container into a closure of its parent, so a
            # name that is not a field of the current container is looked up in the
            # enclosing containers (1.21.11: DebugSubscriptionUpdate.default.payload
            # switches on the `type` of the outer container)
            up = sc.parent
            while up is not None and parts[0] not in up.names:
                up = up.parent
            if up is not None:
                sc = up
        cur = sc.names
        node = None
        for j in range(i, len(parts)):
            # a name is looked up in the CURRENT container only (protodef semantics)
            if parts[j] not in cur:
                raise ValueError("compareTo %r: %r not found" % (path, parts[j]))
            node = cur[parts[j]]
            if isinstance(node, dict):
                cur = node
            elif j != len(parts) - 1:
                raise ValueError("compareTo %r: %r is not a container" % (path, parts[j]))
        if isinstance(node, dict):
            raise ValueError("compareTo %r names a bitfield, not a field of it" % path)
        return node

    def build_switch(self, a, scope, arg):
        cmp = a["compareTo"]
        if cmp.startswith("$"):
            if arg is None:
                raise ValueError("$compareTo without an argument")
            var = arg
        else:
            var = self.resolve_path(scope, cmp)
        var.used = True
        cases = []
        for key, t in a["fields"].items():
            if var.kind == "map" and key not in var.rev and key not in ("true", "false"):
                # minecraft-data lists a case whose key the mapper never produces (a data
                # inconsistency, e.g. a recipe serializer or data component that is only in
                # one of the two tables): protodef can never select it, so it is dropped,
                # and reported in the generator log.
                self.warnings.append("switch case %r is not a mapper output: dropped" % key)
                continue
            body, _ = self.build(t, scope, arg)
            cases.append((key_value(key, var), body))
        dflt = ("void",)
        if "default" in a:
            dflt, _ = self.build(a["default"], scope, arg)
        return ("switch", var, cases, dflt)


def ir_subvars(ir):
    if ir[0] == "bitfield":
        return {n: v for (n, _s, _g, v) in ir[2]}
    return {n: v for (n, _b, v) in ir[2]}


def key_value(key, var):
    if key == "true":
        return 1
    if key == "false":
        return 0
    if var.kind == "map":
        if key not in var.rev:
            raise ValueError("switch key %r is not a mapper output" % key)
        return var.rev[key]
    if var.kind == "str":
        return fnv64(key)
    try:
        return int(key, 0) & 0xFFFFFFFFFFFFFFFF
    except ValueError:
        raise ValueError("switch key %r on a %s variable" % (key, var.kind))


def min_size(ir, gen):
    k = ir[0]
    if k == "scalar":
        return {0: 1, 1: 1, 2: 2, 3: 2, 4: 4, 5: 4, 6: 8, 7: 8, 8: 4, 9: 8, 10: 1,
                11: 1, 12: 1, 13: 16}[ir[1]]
    if k in ("void", "restbuf"):
        return 0
    if k == "container":
        return sum(min_size(i[3], gen) for i in ir[1])
    if k == "bitfield":
        return ir[1]
    if k == "bitflags":
        return min_size(("scalar", ir[1]), gen)
    if k == "string":
        return min_size(("scalar", ir[1]), gen)
    if k == "buffer":
        base = min_size(("scalar", ir[1]), gen) if ir[1] < CK_CONST else 0
        return base + (ir[2] if ir[1] == CK_CONST else 0)
    if k == "array":
        base = min_size(("scalar", ir[1]), gen) if ir[1] < CK_CONST else 0
        return base
    if k == "option":
        return 1
    if k == "switch":
        return min([min_size(b, gen) for _k, b in ir[2]] + [min_size(ir[3], gen)])
    if k == "call":
        sub = gen.subs.get(ir[1])
        return min_size(sub, gen) if sub is not None else 0
    if k == "topbit":
        return min_size(ir[1], gen)
    if k == "emeta":
        return 1
    if k == "nbt":
        return {0: 3, 1: 1, 2: 1, 3: 1}[ir[1]]
    if k in ("regholder", "regset", "lpvec3"):
        return 1
    if k == "rcall":
        return 0
    raise ValueError(k)


# ----------------------------------------------------------- IR -> bytecode
def assign_regs(gen):
    n = 0
    for v in gen.vars:
        if v.used and v.reg is None:
            v.reg = n
            n += 1
    assert n <= REG_MAX, "too many registers: %d" % n
    return n


def reg1(v):
    return 0 if (v is None or v.reg is None) else v.reg + 1


def enc(ir):
    f = Frag()
    k = ir[0]
    if k == "scalar":
        f.op(O_SCALAR); f.u(ir[1]); f.u(0)
    elif k == "void":
        f.op(O_VOID)
    elif k == "container":
        f.op(O_CONTAINER)
        body = Frag()
        for (_t, name, var, sub) in ir[1]:
            body.op(O_FIELD); body.name(name); body.u(reg1(var)); body.sub(enc(sub))
        f.block(body)
    elif k == "bitfield":
        f.op(O_BITFIELD); f.u(ir[1]); f.u(len(ir[2]))
        for (n, size, signed, v) in ir[2]:
            f.u(size); f.u(signed); f.u(reg1(v)); f.name(n)
    elif k == "bitflags":
        f.op(O_BITFLAGS); f.u(ir[1]); f.u(len(ir[2]))
        for (n, bit, v) in ir[2]:
            f.u(bit); f.u(reg1(v)); f.name(n)
    elif k == "string":
        f.op(O_STRING); f.u(ir[1]); f.u(0)
    elif k == "buffer":
        f.op(O_BUFFER); f.u(ir[1]); f.u(carg(ir[1], ir[2]))
    elif k == "restbuf":
        f.op(O_RESTBUF)
    elif k == "array":
        f.op(O_ARRAY); f.u(ir[1]); f.u(carg(ir[1], ir[2])); f.u(ir[3]); f.block(enc(ir[4]))
    elif k == "option":
        f.op(O_OPTION); f.block(enc(ir[1]))
    elif k == "switch":
        f.op(O_SWITCH); f.u(ir[1].reg); f.u(len(ir[2]))
        for key, body in ir[2]:
            f.u(key); f.block(enc(body))
        f.block(enc(ir[3]))
    elif k == "call":
        f.op(O_CALL); f.call(ir[1])
    elif k == "rcall":
        f.op(O_RCALL); f.call(ir[1])
    elif k == "topbit":
        f.op(O_TOPBIT); f.block(enc(ir[1]))
    elif k == "emeta":
        f.op(O_EMETA); f.u(ir[1]); f.block(enc(ir[2]))
    elif k == "nbt":
        f.op(O_NBT); f.u(ir[1])
    elif k == "regholder":
        f.op(O_REGHOLDER); f.name(ir[1]); f.name(ir[2]); f.block(enc(ir[3]))
    elif k == "regset":
        f.op(O_REGSET); f.name(ir[1]); f.name(ir[2]); f.u(ir[3]); f.block(enc(ir[4])); f.block(enc(ir[5]))
    elif k == "lpvec3":
        f.op(O_LPVEC3)
    else:
        raise ValueError(k)
    return f


def carg(ck, a):
    if ck == CK_REG:
        return a.reg
    return a


# ---------------------------------------------------------------- main part
def snake_const(s):
    out = []
    for ch in s:
        out.append(ch.upper() if ch.isalnum() else "_")
    return "".join(out)


HERE = os.path.dirname(os.path.abspath(__file__))


def version_table():
    return json.load(open(os.path.join(HERE, "versions.json")))["versions"]


def load_protocol(data_dir, version):
    paths = json.load(open(os.path.join(data_dir, "dataPaths.json")))["pc"]
    vt = version_table()
    if version in vt:
        version_data = vt[version]["data"]
    else:
        version_data = version
    if version_data not in paths:
        raise SystemExit("unknown version " + version)
    version = version_data
    proto_dir = paths[version]["protocol"]
    ver_dir = paths[version]["version"]
    proto = json.load(open(os.path.join(data_dir, proto_dir, "protocol.json")))
    ver = json.load(open(os.path.join(data_dir, ver_dir, "version.json")))
    return proto, ver["version"], proto_dir


def lit_string(blob):
    """The blob as the body of a Firn string literal (octets < 0x80 only)."""
    parts = []
    for b in blob:
        assert b < 0x80
        c = chr(b)
        if c == '"':
            parts.append('\\"')
        elif c == "\\":
            parts.append("\\\\")
        elif 0x20 <= b < 0x7F and c != "{":
            parts.append(c)
        else:
            parts.append("\\x%02x" % b)
    return "".join(parts)


def generate(data_dir, version, out_path):
    proto, pver, proto_dir = load_protocol(data_dir, version)
    vt = version_table()
    vinfo = vt.get(version, {"flags": [], "chunk_format": "CF_NONE"})
    gen = Gen(proto)

    # collect packets: (state_idx, dir_idx) -> {id: (name, ir)}
    packets = {}
    for si, (sname, _sc) in enumerate(STATES):
        for di, (dname, _dc) in enumerate(DIRS):
            if sname not in proto:
                packets[(si, di)] = {}
                continue
            d = proto[sname][dname]
            gen.local = d["types"]
            pk = d["types"]["packet"]
            mapper = pk[1][0]["type"][1]["mappings"]
            sw = pk[1][1]["type"][1]["fields"]
            table = {}
            for sid, pname in mapper.items():
                pid = int(sid, 0)
                tname = sw.get(pname)
                if tname is None:
                    raise SystemExit("packet %s has no body type" % pname)
                tdef = gen.lookup(tname)
                if isinstance(tdef, str):
                    ir, _ = gen.build_named(tname, None, Scope(None), None)
                else:
                    ir, _ = gen.build(tdef, Scope(None), None)
                table[pid] = (pname, ir)
            packets[(si, di)] = table
    gen.local = {}
    nregs = assign_regs(gen)

    # ---- assemble the blob
    blob = Frag()
    blob.buf += b"FSC1" + bytes([FORMAT, 0, 0, 0])
    sub_pos = {}
    for key in gen.sub_order:
        sub_pos[key] = len(blob.buf)
        blob.sub(enc(gen.subs[key]))
    prog_pos = {}
    for key in sorted(packets):
        for pid, (pname, ir) in sorted(packets[key].items()):
            prog_pos[(key, pid)] = len(blob.buf)
            blob.sub(enc(ir))
    name_pos = {}
    for key in sorted(packets):
        for pid, (pname, ir) in sorted(packets[key].items()):
            name_pos[(key, pid)] = len(blob.buf)
            blob.buf += pname.encode("ascii") + b"\0"
    tab_pos = {}
    for key in sorted(packets):
        ids = packets[key]
        n = (max(ids) + 1) if ids else 0
        tab_pos[key] = (len(blob.buf), n)
        for pid in range(n):
            if pid in ids:
                for v in (name_pos[(key, pid)], prog_pos[(key, pid)]):
                    assert v < (1 << 21)
                    blob.buf += bytes([(v >> 14) & 0x7F, (v >> 7) & 0x7F, v & 0x7F])
            else:
                blob.buf += bytes(6)
    # patch CALL operands: zigzag(target - end_of_operand), 4 groups of 6 bits
    for (off, key) in blob.fix:
        rel = sub_pos[key] - (off + 4)
        zz = (rel << 1) if rel >= 0 else ((-rel << 1) - 1)
        assert zz < (1 << 24)
        blob.buf[off:off + 4] = bytes([zz & 63, (zz >> 6) & 63, (zz >> 12) & 63, (zz >> 18) & 63])
    assert len(blob.buf) < (1 << 21)

    # ---- the Firn source
    sem = json.load(open(os.path.join(HERE, "semantic.json")))["sem"]
    modname = "mc_proto_v%d" % pver
    L = []
    w = L.append
    w("// SPDX-License-Identifier: MPL-2.0")
    w("// GENERATED by tools/mcgen/gen_protocol.py - do not edit")
    w("//")
    w("// lib/mc/v%d/%s.fi -- Minecraft Java %s, protocol %d." % (pver, modname, version, pver))
    w("//")
    w("// Source: minecraft-data (PrismarineJS, open source), %s/protocol.json, plus the hand written" % proto_dir)
    w("// facts of tools/mcgen/versions.json. Regenerate everything with")
    w("//   python3 tools/mcgen/gen_protocol.py --all <minecraft-data>/data lib/mc")
    w("//")
    w("// Contents: packet id constants (<STATE>_<DIR>_<NAME>), packet_name(),")
    w("// packet_schema(), version_make() and one blob of schema programs (%d octets, %d registers)." % (len(blob.buf), nregs))
    w("// The walker lib/mc/mc_schema.fi interprets the programs; their bytecode is")
    w("// documented at the top of tools/mcgen/gen_protocol.py. The blob is a `const str`")
    w("// because that compiles in a fraction of a second; every octet is below 0x80 so that")
    w("// the literal stays valid UTF-8.")
    w("")
    w("import std.rt")
    w("import mc.mc_version")
    w("import mc.mc_sem")
    w("import mc.v%d.mc_reg_v%d" % (pver, pver))
    w("")
    w("const PROTOCOL_VERSION: i32 = %d" % pver)
    w("const SCHEMA_FORMAT: i32 = %d" % FORMAT)
    w("const SCHEMA_REGS: i32 = %d" % nregs)
    w("")
    seen = set()
    count = 0
    for si, (sname, sc) in enumerate(STATES):
        for di, (dname, dc) in enumerate(DIRS):
            w("// ---- %s %s" % (sname, dname))
            for pid, (pname, _ir) in sorted(packets[(si, di)].items()):
                cname = "%s_%s_%s" % (sc, dc, snake_const(pname))
                assert cname not in seen, cname
                seen.add(cname)
                w("const %s: i32 = %d" % (cname, pid))
                count += 1
    w("")
    for (si, di), (pos, n) in sorted(tab_pos.items()):
        w("const TAB_%s_%s: usize = %d" % (STATES[si][1], DIRS[di][1], pos))
        w("const TAB_%s_%s_LEN: i32 = %d" % (STATES[si][1], DIRS[di][1], n))
    w("")
    w('const SCHEMA_BLOB: str = "%s"' % lit_string(blob.buf))
    w("")
    nk = len(STATES) * len(DIRS)
    w("""fn blob_base() -> u64 {
    return SCHEMA_BLOB.p as u64
}

// Offset of the 6 octet table entry of (state, dir, id), or 0.
fn table_entry(state: i32, dir: i32, id: i32) -> usize {
    if id < 0 {
        return 0
    }
    var off: usize = 0
    var len: i32 = 0
    var k: i32 = state * 2 + dir""")
    for si in range(len(STATES)):
        for di in range(2):
            w("    if k == %d {" % (si * 2 + di))
            w("        off = TAB_%s_%s" % (STATES[si][1], DIRS[di][1]))
            w("        len = TAB_%s_%s_LEN" % (STATES[si][1], DIRS[di][1]))
            w("    }")
    w("""    if id >= len {
        return 0
    }
    return off + (id as usize) * 6
}

fn table_field(entry: usize, which: usize) -> usize {
    let base: u64 = blob_base()
    let i: usize = entry + which * 3
    let a: usize = rt.ld8(base, i) as usize
    let b: usize = rt.ld8(base, i + 1) as usize
    let c: usize = rt.ld8(base, i + 2) as usize
    return (a << 14) | (b << 7) | c
}

// NUL terminated packet name (a pointer into the blob) or 0.
fn packet_name(state: i32, dir: i32, id: i32) -> u64 {
    let e: usize = table_entry(state, dir, id)
    if e == 0 {
        return 0
    }
    let o: usize = table_field(e, 0)
    if o == 0 {
        return 0
    }
    return blob_base() + o as u64
}

// Handle for mc_schema.walk: the address of the packet's program, or 0.
fn packet_schema(state: i32, dir: i32, id: i32) -> u64 {
    let e: usize = table_entry(state, dir, id)
    if e == 0 {
        return 0
    }
    let o: usize = table_field(e, 1)
    if o == 0 {
        return 0
    }
    return blob_base() + o as u64
}

// Highest packet id + 1 of a (state, dir) table: for loops over all packets.
fn packet_id_limit(state: i32, dir: i32) -> i32 {
    var len: i32 = 0
    var k: i32 = state * 2 + dir""")
    for si in range(len(STATES)):
        for di in range(2):
            w("    if k == %d {" % (si * 2 + di))
            w("        len = TAB_%s_%s_LEN" % (STATES[si][1], DIRS[di][1]))
            w("    }")
    w("    return len")
    w("}")
    w("")
    # ---- the Version object
    flags = vinfo["flags"]
    w("// The version object: protocol number, feature flags, chunk format, the semantic")
    w("// packet ids (mc_sem.fi) and the three lookup functions as function values.")
    w("fn version_make() -> mc_version.Version {")
    w("    var v: mc_version.Version = mc_version.version_empty()")
    w("    v.protocol = %d" % pver)
    w("    v.flags = %s" % (" | ".join("mc_version.%s" % f for f in flags) if flags else "0 as u64"))
    w("    v.chunk_format = mc_version.%s" % vinfo["chunk_format"])
    w('    v.name = VERSION_NAME.p as u64')
    w('    v.name_len = %d' % len(version))
    w("    v.id_limit = packet_id_limit")
    w("    v.name_of = packet_name")
    w("    v.schema_of = packet_schema")
    w("    v.registry_of = mc_reg_v%d.registry_make" % pver)
    states_by_name = {sname: si for si, (sname, _) in enumerate(STATES)}
    dirs_by_name = {dname: di for di, (dname, _) in enumerate(DIRS)}
    for e in sem:
        si = states_by_name[e["state"]]
        di = dirs_by_name[e["dir"]]
        found = -1
        for want in e["names"]:
            for pid, (pname, _ir) in packets[(si, di)].items():
                if pname == want:
                    found = pid
            if found >= 0:
                break
        w("    v.sem[mc_sem.SEM_%s] = %d" % (e["id"], found))
    w("    return v")
    w("}")
    w("")
    w('const VERSION_NAME: str = "%s"' % version)
    with open(out_path, "w") as fh:
        fh.write("\n".join(L) + "\n")
    uniq = sorted(set(gen.warnings))
    sys.stderr.write("gen_protocol: %s protocol %d: %d packets, %d subroutines, blob %d octets, %d registers%s\n" % (
        version, pver, count, len(gen.sub_order), len(blob.buf), nregs,
        (", %d data warnings (MCGEN_VERBOSE=1 lists them)" % len(uniq)) if uniq else ""))
    if os.environ.get("MCGEN_VERBOSE"):
        for m in uniq:
            sys.stderr.write("  warning: %s\n" % m)
    return pver


def write_sem(path):
    sem = json.load(open(os.path.join(HERE, "semantic.json")))["sem"]
    assert len(sem) <= 96
    out = ["// SPDX-License-Identifier: MPL-2.0",
           "// GENERATED by tools/mcgen/gen_protocol.py from tools/mcgen/semantic.json - do not edit",
           "//",
           "// lib/mc/mc_sem.fi -- the SEMANTIC packet ids. Engine code asks the version object",
           "// `v.sem[SEM_PLAY_S2C_MAP_CHUNK]` for the wire id; the value is -1 when the version",
           "// has no such packet. Nothing in the engine names a numeric packet id.",
           "",
           "export { SEM_COUNT,"]
    out.append("    " + ", ".join("SEM_" + e["id"] for e in sem) + " }")
    out.append("")
    out.append("const SEM_COUNT: usize = 96")
    for i, e in enumerate(sem):
        out.append("const SEM_%s: usize = %d" % (e["id"], i))
    with open(path, "w") as fh:
        fh.write("\n".join(out) + "\n")


def write_registry(path, entries):
    out = ["// SPDX-License-Identifier: MPL-2.0",
           "// GENERATED by tools/mcgen/gen_protocol.py - do not edit",
           "//",
           "// lib/mc/mc_versions.fi -- every protocol version compiled into this build. A program",
           "// that imports it can pick the version at run time from the protocol number",
           "// (status ping answers, or the command line). Importing it pulls all listed version",
           "// modules into the binary (about 60 KB of schema data each).",
           "",
           "import mc.mc_version"]
    for pver, vkey in entries:
        out.append("import mc.v%d.mc_proto_v%d" % (pver, pver))
    out.append("")
    out.append("export { version_by_protocol, version_count, version_protocol_at }")
    out.append("")
    out.append("fn version_count() -> usize { return %d }" % len(entries))
    out.append("")
    out.append("fn version_protocol_at(i: usize) -> i32 {")
    for k, (pver, vkey) in enumerate(entries):
        out.append("    if i == %d {" % k)
        out.append("        return %d" % pver)
        out.append("    }")
    out.append("    return 0 - 1")
    out.append("}")
    out.append("")
    out.append("// Fills `out` and returns true when the protocol number is known.")
    out.append("fn version_by_protocol(p: i32, out: *mut mc_version.Version) -> bool {")
    for pver, vkey in entries:
        out.append("    if p == %d {" % pver)
        out.append("        *out = mc_proto_v%d.version_make()" % pver)
        out.append("        return true")
        out.append("    }")
    out.append("    return false")
    out.append("}")
    with open(path, "w") as fh:
        fh.write("\n".join(out) + "\n")


def main():
    if len(sys.argv) == 5 and sys.argv[1] == "--all":
        data_dir, libdir = sys.argv[2], sys.argv[3]
        want = sys.argv[4].split(",") if sys.argv[4] != "all" else list(version_table().keys())
        os.makedirs(libdir, exist_ok=True)
        write_sem(os.path.join(libdir, "mc_sem.fi"))
        entries = []
        for vkey in want:
            proto, pver, _ = load_protocol(data_dir, vkey)
            d = os.path.join(libdir, "v%d" % pver)
            os.makedirs(d, exist_ok=True)
            generate(data_dir, vkey, os.path.join(d, "mc_proto_v%d.fi" % pver))
            gen_registry.generate(data_dir, vkey, os.path.join(d, "mc_reg_v%d.fi" % pver))
            entries.append((pver, vkey))
        write_registry(os.path.join(libdir, "mc_versions.fi"), entries)
        gen_entity.generate(data_dir, [e[1] for e in entries], load_protocol, os.path.join(libdir, "mc_entity_layout.fi"))
        return
    if len(sys.argv) != 4:
        raise SystemExit(__doc__)
    data_dir, version, out_path = sys.argv[1:4]
    generate(data_dir, version, out_path)


if __name__ == "__main__":
    main()

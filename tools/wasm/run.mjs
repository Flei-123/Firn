// SPDX-License-Identifier: MPL-2.0
// tools/wasm/run.mjs -- runs a Firn program compiled with
// `--target=wasm32-browser` under node, the way a terminal runs the
// native one: standard output, standard error, standard input, exit code.
//
// WHY THIS FILE EXISTS. A WebAssembly module cannot run by itself; some
// host has to instantiate it and answer its imports. In the browser that
// host is demos/webdemo/firn.js. For the byte-for-byte comparison with the
// native programs (tools/wasm/run.sh) the host has to be a PROCESS -- one
// whose output a shell can capture and whose exit code it can read. This is
// that host, and it answers the same six imports (`firn.write`, `read`,
// `exit`, `clock_ns`, `random`, `sleep_ns`) with the same meaning. No Firn
// program logic lives here: every line below is plumbing between the module
// and the operating system.
//
//     node tools/wasm/run.mjs prog.wasm < input > output; echo $?
import fs from 'node:fs';
import crypto from 'node:crypto';

const file = process.argv[2];
if (!file) {
    process.stderr.write('usage: node run.mjs prog.wasm\n');
    process.exit(2);
}

// `exit` unwinds the WebAssembly stack by throwing this; nothing else
// throws it, so a caught one is always a clean end.
class Exit {
    constructor(code) { this.code = code; }
}

let memory = null;
const bytes = (p, n) => new Uint8Array(memory.buffer, p >>> 0, n >>> 0);
const t0 = process.hrtime.bigint();
const sleeper = new Int32Array(new SharedArrayBuffer(4));

const firn = {
    // write(fd, buf, len) -> octets written or -errno
    write(fd, p, n) {
        if (fd !== 1 && fd !== 2) return -9; // EBADF
        let done = 0;
        const b = bytes(p, n);
        while (done < b.length) {
            try {
                done += fs.writeSync(fd, b, done, b.length - done);
            } catch (e) {
                if (e.code === 'EAGAIN') continue;
                return done > 0 ? done : -5; // EIO
            }
        }
        return done;
    },
    // read(fd, buf, len) -> octets read, 0 at the end, or -errno
    read(fd, p, n) {
        if (fd !== 0) return -9;
        for (;;) {
            try {
                return fs.readSync(0, bytes(p, n), 0, n, null);
            } catch (e) {
                if (e.code === 'EAGAIN') continue;
                if (e.code === 'EOF') return 0;
                return -5;
            }
        }
    },
    exit(code) {
        throw new Exit(code);
    },
    // clock_ns(clock) -> nanoseconds. CLOCK_REALTIME (0) is the wall clock;
    // every other clock (monotonic, process and thread CPU time) is the
    // monotonic clock of the host -- a page has no CPU time of its own.
    clock_ns(clk) {
        if (clk === 0) return BigInt(Date.now()) * 1000000n;
        return process.hrtime.bigint() - t0 + 1n;
    },
    random(p, n) {
        crypto.randomFillSync(bytes(p, n));
        return n;
    },
    sleep_ns(ns) {
        const ms = Number(ns / 1000000n);
        if (ms > 0) Atomics.wait(sleeper, 0, 0, ms);
        return 0;
    },
};

const facePackUrl = new URL('../../demos/webdemo/pdffonts.pack', import.meta.url);
function faceHost() {
    let pack = null;
    try {
        const bytes = fs.readFileSync(facePackUrl);
        const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
        const count = view.getUint32(0, true);
        const offs = [];
        for (let i = 0; i <= count; i++) offs.push(view.getUint32(4 + i * 4, true));
        pack = { count, offs, base: 4 + (count + 1) * 4, bytes };
    } catch (e) {
        pack = null;
    }
    const span = (i) => (pack && i < pack.count ? pack.offs[i + 1] - pack.offs[i] : 0);
    return {
        firn_web_face_size: (i) => span(i),
        firn_web_face_read: (i, p, n) => {
            if (span(i) !== n) return 0;
            const from = pack.base + pack.offs[i];
            new Uint8Array(memory.buffer, p, n).set(pack.bytes.subarray(from, from + n));
            return n;
        },
    };
}

let code = 0;
try {
    const { instance } = await WebAssembly.instantiate(fs.readFileSync(file), {
        firn,
        env: new Proxy(faceHost(), {
            get(target, name) {
                if (name in target) return target[name];
                return () => {
                    throw new Error(`the program imports env.${String(name)}, which this host does not provide`);
                };
            },
        }),
    });
    memory = instance.exports.memory;
    instance.exports._start();
} catch (e) {
    if (e instanceof Exit) {
        code = e.code;
    } else {
        process.stderr.write(`wasm: ${e && e.stack ? e.stack : e}\n`);
        code = 134;
    }
}
process.exitCode = code & 0xff;

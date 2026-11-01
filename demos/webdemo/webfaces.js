// SPDX-License-Identifier: MPL-2.0
'use strict';

(() => {
    const parse = (buf) => {
        const dv = new DataView(buf);
        const count = dv.getUint32(0, true);
        const offs = [];
        for (let i = 0; i <= count; i++) offs.push(dv.getUint32(4 + i * 4, true));
        const base = 4 + (count + 1) * 4;
        return { count, offs, base, bytes: new Uint8Array(buf) };
    };

    const loadSync = (url) => {
        const x = new XMLHttpRequest();
        x.open('GET', url, false);
        x.overrideMimeType('text/plain; charset=x-user-defined');
        x.send();
        if (x.status !== 200) return null;
        const t = x.responseText;
        const b = new Uint8Array(t.length);
        for (let i = 0; i < t.length; i++) b[i] = t.charCodeAt(i) & 255;
        return b.buffer;
    };

    window.firnWebFaces = (getMem, url) => {
        let pack = null;
        let failed = false;
        let asked = null;
        const counters = window.firnFaceStats = { sync: 0, async: 0, reads: 0 };
        const prefetch = () => {
            if (asked) return;
            asked = fetch(url).then((r) => (r.ok ? r.arrayBuffer() : null)).then((b) => {
                if (b && !pack) { pack = parse(b); counters.async++; }
            }).catch(() => {});
        };
        const ensure = () => {
            if (pack || failed) return;
            try {
                const b = loadSync(url);
                if (b) { pack = parse(b); counters.sync++; } else failed = true;
            } catch (e) { failed = true; }
        };
        window.firnFacesPrefetch = prefetch;
        return {
            firn_web_face_size(i) {
                ensure();
                if (!pack || i >= pack.count) return 0;
                return pack.offs[i + 1] - pack.offs[i];
            },
            firn_web_face_read(i, p, n) {
                ensure();
                if (!pack || i >= pack.count) return 0;
                const len = pack.offs[i + 1] - pack.offs[i];
                if (len !== n) return 0;
                const from = pack.base + pack.offs[i];
                new Uint8Array(getMem().buffer, p, n).set(pack.bytes.subarray(from, from + n));
                counters.reads++;
                return n;
            },
        };
    };
})();

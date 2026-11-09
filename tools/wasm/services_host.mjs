// SPDX-License-Identifier: MPL-2.0
import fs from 'node:fs';
import path from 'node:path';

const ANS_OK = 1;
const ANS_YES = 3;
const BTNS_YES_NO = 2;

export function servicesHost(memoryOf) {
    const utf8 = new TextDecoder();
    const enc = new TextEncoder();
    const view = (p, n) => new Uint8Array(memoryOf().buffer, p >>> 0, n >>> 0);
    const text = (p, n) => utf8.decode(view(p, n).slice());
    const store = new Map();
    return {
        firn_web_store_get(kp, kn, p, cap) {
            const v = store.get(text(kp, kn));
            if (!v) return 0;
            const b = enc.encode(v).subarray(0, cap);
            view(p, b.length).set(b);
            return b.length;
        },
        firn_web_store_set(kp, kn, p, n) {
            store.set(text(kp, kn), text(p, n));
        },
        firn_web_clip_write() {
            return 1;
        },
        firn_web_clip_read() {},
        firn_web_dialog_file() {
            return 1;
        },
        firn_web_dialog_dir() {
            return 1;
        },
        firn_web_dialog_message(kind, tp, tn, xp, xn, buttons) {
            return buttons === BTNS_YES_NO ? ANS_YES : ANS_OK;
        },
        firn_web_download(np, nn, mp, mn, p, n) {
            const dir = process.env.FIRN_SERVICES_OUT;
            if (dir) fs.writeFileSync(path.join(dir, path.basename(text(np, nn))), view(p, n));
            return 1;
        },
    };
}

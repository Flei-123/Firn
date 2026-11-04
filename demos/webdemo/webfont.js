// SPDX-License-Identifier: MPL-2.0
'use strict';

(() => {
    const FAMILY = 'system-ui, -apple-system, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif';
    const stub = () => ({
        firn_web_text_probe: () => 0,
        firn_web_text_advance: () => 0,
        firn_web_text_pair: () => 0,
        firn_web_text_vmetrics: () => 0,
        firn_web_text_glyph: () => -1,
    });

    window.firnWebFont = (getMem) => {
        let measure = null;
        let raster = null;
        try {
            measure = new OffscreenCanvas(8, 8).getContext('2d');
            raster = new OffscreenCanvas(8, 8).getContext('2d', { willReadFrequently: true });
        } catch (e) {
            try {
                measure = document.createElement('canvas').getContext('2d');
                raster = document.createElement('canvas').getContext('2d', { willReadFrequently: true });
            } catch (e2) { return stub(); }
        }
        if (!measure || !raster || typeof measure.measureText !== 'function') return stub();
        let measureFont = '';
        const setFont = (ctx, size, weight, slant) => {
            const f = (slant ? 'italic ' : '') + weight + ' ' + size + 'px ' + FAMILY;
            if (ctx === measure) {
                if (f !== measureFont) { ctx.font = f; measureFont = f; }
            } else ctx.font = f;
        };
        const text = (cp) => String.fromCodePoint(cp);
        const width = (s, size, weight) => { setFont(measure, size, weight, 0); return measure.measureText(s).width; };
        return {
            firn_web_text_probe: () => 1,
            firn_web_text_advance: (cp, size, weight) => width(text(cp), size, weight),
            firn_web_text_pair: (a, b, size, weight) => {
                const sa = text(a), sb = text(b);
                return width(sa + sb, size, weight) - width(sa, size, weight) - width(sb, size, weight);
            },
            firn_web_text_vmetrics: (size, weight, out) => {
                setFont(measure, size, weight, 0);
                const m = measure.measureText('Hg');
                const asc = m.fontBoundingBoxAscent !== undefined ? m.fontBoundingBoxAscent : size * 0.93;
                const dsc = m.fontBoundingBoxDescent !== undefined ? m.fontBoundingBoxDescent : size * 0.27;
                const dv = new DataView(getMem().buffer);
                dv.setFloat64(out, asc, true);
                dv.setFloat64(out + 8, dsc, true);
                return 1;
            },
            firn_web_text_glyph: (cp, size, weight, slant, qx, buf, cap, meta) => {
                setFont(raster, size, weight, slant);
                const s = text(cp);
                const m = raster.measureText(s);
                const pad = 2;
                const left = Math.floor(-m.actualBoundingBoxLeft) - pad;
                const right = Math.ceil(m.actualBoundingBoxRight) + 1 + pad;
                const top = Math.ceil(m.actualBoundingBoxAscent) + pad;
                const bottom = Math.ceil(m.actualBoundingBoxDescent) + pad;
                const w = right - left, h = top + bottom;
                const dv = new DataView(getMem().buffer);
                if (!(w > 0 && h > 0) || s.trim() === '') {
                    dv.setInt32(meta, 0, true); dv.setInt32(meta + 4, 0, true);
                    dv.setInt32(meta + 8, 0, true); dv.setInt32(meta + 12, 0, true);
                    return 0;
                }
                if (w * h > cap) return -1;
                const cv = raster.canvas;
                if (cv.width < w || cv.height < h) { cv.width = Math.max(cv.width, w); cv.height = Math.max(cv.height, h); setFont(raster, size, weight, slant); }
                raster.clearRect(0, 0, cv.width, cv.height);
                raster.fillStyle = '#fff';
                raster.textBaseline = 'alphabetic';
                raster.fillText(s, -left + qx / 4, top);
                const img = raster.getImageData(0, 0, w, h).data;
                const out = new Uint8Array(getMem().buffer, buf, w * h);
                for (let i = 0, n = w * h; i < n; i++) out[i] = img[i * 4 + 3];
                dv.setInt32(meta, w, true); dv.setInt32(meta + 4, h, true);
                dv.setInt32(meta + 8, left, true); dv.setInt32(meta + 12, top, true);
                return w * h;
            },
        };
    };
})();

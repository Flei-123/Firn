// SPDX-License-Identifier: MPL-2.0
'use strict';

(() => {
    const pull = (s, outs, n) => {
        let i = 0;
        if (!s.paused) {
            while (i < n && s.q.length) {
                const b = s.q[0];
                const take = Math.min(b.length / s.ch - s.off, n - i);
                for (let c = 0; c < outs.length; c++) {
                    const src = Math.min(c, s.ch - 1);
                    for (let k = 0; k < take; k++) outs[c][i + k] = b[(s.off + k) * s.ch + src];
                }
                i += take;
                s.off += take;
                s.consumed += take;
                if (s.off * s.ch >= b.length) { s.q.shift(); s.off = 0; }
            }
        }
        for (let c = 0; c < outs.length; c++) outs[c].fill(0, i);
    };

    const workletSource = `const pull = ${pull.toString()};
class FirnPcm extends AudioWorkletProcessor {
    constructor(o) {
        super();
        this.s = { q: [], off: 0, ch: o.processorOptions.ch, consumed: 0, paused: false, epoch: 0 };
        this.sent = -1;
        this.port.onmessage = (m) => {
            const d = m.data, s = this.s;
            if (d.pcm) s.q.push(d.pcm);
            else if (d.pause !== undefined) s.paused = d.pause;
            else if (d.flush !== undefined) { s.q = []; s.off = 0; s.consumed = 0; s.epoch = d.flush; this.sent = -1; }
        };
    }
    process(inputs, outputs) {
        const s = this.s;
        pull(s, outputs[0], outputs[0][0].length);
        if (s.consumed !== this.sent) { this.sent = s.consumed; this.port.postMessage({ used: s.consumed, epoch: s.epoch }); }
        return true;
    }
}
registerProcessor('firn-pcm', FirnPcm);`;

    const NONE = 0, WAITING = 1, RUNNING = 2;
    const MODE_NONE = 0, MODE_WORKLET = 1, MODE_SCRIPT = 2;

    window.firnWebAudio = (getMemory, preferScript) => {
        const streams = new Map();
        window.firnAudioStreams = streams;
        let seq = 0;
        const bytes = (p, n) => new Uint8Array(getMemory().buffer, p >>> 0, n >>> 0);

        const unlock = () => {
            for (const s of streams.values()) if (s.ctx.state === 'suspended') s.ctx.resume().catch(() => {});
        };
        for (const name of ['pointerdown', 'keydown', 'touchend', 'click'])
            document.addEventListener(name, unlock, true);

        const latencyFrames = (s) => Math.round(((s.ctx.baseLatency || 0) + (s.ctx.outputLatency || 0)) * s.rate);

        const refresh = (s) => {
            const raw = s.consumed >= s.pushed ? s.pushed : Math.max(0, s.consumed - latencyFrames(s));
            if (raw > s.played) s.played = raw;
        };

        const tap = (s, node) => {
            s.analyser = s.ctx.createAnalyser();
            node.connect(s.analyser);
        };

        const startScript = (s) => {
            const node = s.ctx.createScriptProcessor(2048, 0, s.ch);
            node.onaudioprocess = (e) => {
                const outs = [];
                for (let c = 0; c < e.outputBuffer.numberOfChannels; c++) outs.push(e.outputBuffer.getChannelData(c));
                pull(s, outs, e.outputBuffer.length);
            };
            node.connect(s.ctx.destination);
            tap(s, node);
            s.node = node;
            s.mode = MODE_SCRIPT;
        };

        const startWorklet = async (s) => {
            const url = URL.createObjectURL(new Blob([workletSource], { type: 'text/javascript' }));
            try {
                await s.ctx.audioWorklet.addModule(url);
            } finally {
                URL.revokeObjectURL(url);
            }
            if (s.closed) return;
            const node = new AudioWorkletNode(s.ctx, 'firn-pcm', {
                numberOfInputs: 0, numberOfOutputs: 1, outputChannelCount: [s.ch], processorOptions: { ch: s.ch },
            });
            node.port.onmessage = (m) => { if (m.data.epoch === s.epoch) s.consumed = m.data.used; };
            node.connect(s.ctx.destination);
            tap(s, node);
            s.node = node;
            s.mode = MODE_WORKLET;
            if (s.paused) node.port.postMessage({ pause: true });
            for (const b of s.pre) node.port.postMessage({ pcm: b }, [b.buffer]);
            s.pre = [];
        };

        const open = (rate, ch) => {
            const Ctx = window.AudioContext || window.webkitAudioContext;
            if (!Ctx || (ch !== 1 && ch !== 2)) return 0;
            let ctx;
            try { ctx = new Ctx({ sampleRate: rate, latencyHint: 'interactive' }); } catch (e) { return 0; }
            if (ctx.sampleRate !== rate) { ctx.close().catch(() => {}); return 0; }
            const s = { ctx, rate, ch, cap: rate >> 1, q: [], pre: [], off: 0, pushed: 0, consumed: 0, played: 0,
                epoch: 0, paused: false, closed: false, node: null, mode: MODE_NONE };
            const id = ++seq;
            streams.set(id, s);
            ctx.resume().catch(() => {});
            if (!preferScript && ctx.audioWorklet) startWorklet(s).catch(() => { if (!s.closed) startScript(s); });
            else startScript(s);
            return id;
        };

        const get = (id) => streams.get(id >>> 0);

        return {
            firn_web_audio_open: open,
            firn_web_audio_room(id) {
                const s = get(id);
                if (!s) return 0;
                refresh(s);
                return Math.max(0, s.cap - (s.pushed - s.played));
            },
            firn_web_audio_push(id, p, frames) {
                const s = get(id);
                if (!s) return 0;
                refresh(s);
                const n = Math.min(frames >>> 0, Math.max(0, s.cap - (s.pushed - s.played)));
                if (n === 0) return 0;
                const i16 = new Int16Array(bytes(p, n * s.ch * 2).slice().buffer);
                const f = new Float32Array(i16.length);
                for (let k = 0; k < i16.length; k++) f[k] = i16[k] / 32768;
                s.pushed += n;
                if (s.mode === MODE_WORKLET) s.node.port.postMessage({ pcm: f }, [f.buffer]);
                else if (s.mode === MODE_NONE && s.ctx.audioWorklet && !preferScript) s.pre.push(f);
                else s.q.push(f);
                return n;
            },
            firn_web_audio_queued(id) {
                const s = get(id);
                if (!s) return 0;
                refresh(s);
                return Math.max(0, s.pushed - s.played);
            },
            firn_web_audio_pause(id, on) {
                const s = get(id);
                if (!s) return 0;
                s.paused = !!on;
                if (s.mode === MODE_WORKLET) s.node.port.postMessage({ pause: s.paused });
                return 1;
            },
            firn_web_audio_flush(id) {
                const s = get(id);
                if (!s) return 0;
                s.epoch++;
                s.q = [];
                s.pre = [];
                s.off = 0;
                s.pushed = s.consumed = s.played = 0;
                if (s.mode === MODE_WORKLET) s.node.port.postMessage({ flush: s.epoch });
                return 1;
            },
            firn_web_audio_close(id) {
                const s = get(id);
                if (!s) return;
                s.closed = true;
                streams.delete(id >>> 0);
                if (s.node) s.node.disconnect();
                s.ctx.close().catch(() => {});
            },
            firn_web_audio_state(id) {
                const s = get(id);
                if (!s || s.ctx.state === 'closed') return NONE;
                return s.ctx.state === 'running' ? RUNNING : WAITING;
            },
            firn_web_audio_mode(id) { const s = get(id); return s ? s.mode : MODE_NONE; },
            firn_web_audio_played(id) {
                const s = get(id);
                if (!s) return 0;
                refresh(s);
                return s.played >>> 0;
            },
            firn_web_audio_latency(id) { const s = get(id); return s ? latencyFrames(s) : 0; },
        };
    };
})();

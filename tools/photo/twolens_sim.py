#!/usr/bin/env python3
"""r657 feasibility: two-lens fusion (main + ultrawide) for FirnCam, synthetic study.

Optics come from the Xiaomi 14T Pro camera dump (focal_x10 / equiv_x10 / raw):
  main id 6: f 6.6 mm, equiv 23.2 mm, raw 4096x3072 (50 MP binned 2x2)
  ultrawide id 3: f 1.9 mm, equiv 14.8 mm, raw 4000x3000
F-numbers (main 1.6, ultrawide 2.2) are public spec-sheet values (assumption).

Model: one synthetic scene on a fine "truth" grid; each camera = lens blur (Gaussian)
+ pixel aperture + sampling + Poisson/read noise.  Fusion = Wiener merge on the
main grid with a per-pixel parallax gate.  Metrics: PSNR vs the ideal main-grid image,
slanted-edge MTF50 in cycles/degree.

usage: twolens_sim.py [--quick]
"""
import sys
import numpy as np
from scipy import ndimage, optimize

DIAG_FF = 43.267
F_MAIN, EQ_MAIN, RAW_MAIN = 6.6, 23.2, (4096, 3072)
F_UW, EQ_UW, RAW_UW = 1.9, 14.8, (4000, 3000)
FN_MAIN, FN_UW = 1.6, 2.2
READ_NOISE = 2.0
K = 4  # truth samples per main pixel


def pitch_um(f, eq, raw):
    diag = DIAG_FF / (eq / f)
    return diag / np.hypot(*raw) * 1000.0


P_MAIN = pitch_um(F_MAIN, EQ_MAIN, RAW_MAIN)
P_UW = pitch_um(F_UW, EQ_UW, RAW_UW)
PPD_MAIN = F_MAIN / (P_MAIN / 1000.0) * np.pi / 180.0  # pixels per degree
PPD_UW = F_UW / (P_UW / 1000.0) * np.pi / 180.0
PPD_FULL = PPD_MAIN * 2.0  # 50 MP mode
PPD_T = PPD_MAIN * K
R_UW = PPD_UW / PPD_MAIN
# photons per pixel ~ (pitch / fnumber)^2
PHOTON_RATIO_UW = ((P_UW / FN_UW) / (P_MAIN / FN_MAIN)) ** 2


def analytic():
    out = []
    out.append("pitch main(binned) %.3f um, uw %.3f um" % (P_MAIN, P_UW))
    out.append("px/deg main binned %.1f, main 50MP %.1f, uw %.1f (uw/main = %.3f)" % (PPD_MAIN, PPD_FULL, PPD_UW, R_UW))
    out.append("nyquist cyc/deg main %.1f, main50 %.1f, uw %.1f" % (PPD_MAIN / 2, PPD_FULL / 2, PPD_UW / 2))
    wm = P_MAIN * RAW_MAIN[0] / 1000.0
    for z in (1, 2, 3, 5):
        half = np.arctan(wm / 2.0 / F_MAIN / z)
        fov = 2 * np.degrees(half)
        uw_px = 2 * F_UW * np.tan(half) / (P_UW / 1000.0)
        out.append("zoom %dx: HFOV %.1f deg; main binned %d px, main 50MP %d px, uw crop %d px" % (
            z, fov, RAW_MAIN[0] / z, 2 * RAW_MAIN[0] / z, uw_px))
    # light per solid angle ~ aperture area
    dm, du = F_MAIN / FN_MAIN, F_UW / FN_UW
    out.append("aperture area main %.2f mm2, uw %.2f mm2 -> photons per angle uw/main = %.3f" % (
        np.pi * dm * dm / 4, np.pi * du * du / 4, (du / dm) ** 2))
    out.append("photons per px uw/main = %.3f" % PHOTON_RATIO_UW)
    out.append("ideal SNR gain of adding uw photons: %.2f dB" % (10 * np.log10(1 + (du / dm) ** 2)))
    return out


def make_scene(rng, h, w, detail=1.0):
    """Texture 1/f plus hard-edged rectangles; mean ~1."""
    fy = np.fft.fftfreq(h)[:, None]
    fx = np.fft.fftfreq(w)[None, :]
    r = np.sqrt(fx * fx + fy * fy)
    r[0, 0] = 1.0
    spec = (r ** -1.3) * (np.random.default_rng(rng.integers(1 << 30)).standard_normal((h, w)) +
                          1j * np.random.default_rng(rng.integers(1 << 30)).standard_normal((h, w)))
    spec[0, 0] = 0
    tex = np.real(np.fft.ifft2(spec))
    tex = tex / tex.std() * 0.22 * detail
    img = 1.0 + tex
    for _ in range(40):
        ww, hh = rng.integers(10, 160), rng.integers(10, 160)
        x0, y0 = rng.integers(0, w - ww), rng.integers(0, h - hh)
        img[y0:y0 + hh, x0:x0 + ww] *= rng.uniform(0.5, 1.6)
    img = np.clip(img, 0.03, 4.0)
    return img / img.mean()


def capture(truth, ppd_cam, ncols, nrows, sigma_lens_deg, shift_deg, level, rng, noisy=True, shift_px_truth=(0.0, 0.0)):
    """Lens blur + pixel aperture + sampling + noise.  Returns normalised image (mean ~1) and clean image."""
    th, tw = truth.shape
    s_lens = sigma_lens_deg * PPD_T
    s_pix = (PPD_T / ppd_cam) / np.sqrt(12.0)
    s = float(np.hypot(s_lens, s_pix))
    bl = ndimage.gaussian_filter(truth, s, mode="reflect")
    step = PPD_T / ppd_cam
    ys = (np.arange(nrows) - (nrows - 1) / 2.0) * step + (th - 1) / 2.0 + shift_deg[1] * PPD_T + shift_px_truth[1]
    xs = (np.arange(ncols) - (ncols - 1) / 2.0) * step + (tw - 1) / 2.0 + shift_deg[0] * PPD_T + shift_px_truth[0]
    yy, xx = np.meshgrid(ys, xs, indexing="ij")
    clean = ndimage.map_coordinates(bl, [yy, xx], order=3, mode="nearest")
    if not noisy:
        return clean, clean
    e = rng.poisson(np.clip(clean, 0, None) * level).astype(np.float64) + rng.standard_normal(clean.shape) * READ_NOISE
    return e / level, clean


NM, NH = 256, 192  # main grid
NU_W, NU_H = 176, 136


def mtf_lens(f_cyc_px, sigma_px):
    return np.exp(-2 * (np.pi * sigma_px * f_cyc_px) ** 2)


def sinc(x):
    return np.sinc(x)


def wiener_estimate(M, sm2, U, su2, sig_m_px, sig_u_px, Sprior):
    """Wiener merge on the main grid.  Channels: M (full band) and U (band-limited, already registered, upsampled)."""
    h, w = M.shape
    fx = np.fft.fftfreq(w)[None, :]
    fy = np.fft.fftfreq(h)[:, None]
    f = np.sqrt(fx * fx + fy * fy)
    Hm = mtf_lens(f, sig_m_px)
    num = np.conj(Hm) * np.fft.fft2(M) / sm2
    den = np.abs(Hm) ** 2 / sm2
    if U is not None:
        fu = f / R_UW
        pix = np.where(sinc(f) > 1e-3, sinc(fu) / np.maximum(sinc(f), 1e-3), 0.0)
        Hu = mtf_lens(fu, sig_u_px) * pix * (fu < 0.5)
        Nu = su2 / (R_UW ** 2)
        num = num + np.conj(Hu) * np.fft.fft2(U) / Nu
        den = den + np.abs(Hu) ** 2 / Nu
    est = num / (den + 1.0 / Sprior)
    return np.real(np.fft.ifft2(est))


def radial_prior(ref):
    F = np.abs(np.fft.fft2(ref - ref.mean())) ** 2 / ref.size
    h, w = ref.shape
    fx = np.fft.fftfreq(w)[None, :]
    fy = np.fft.fftfreq(h)[:, None]
    f = np.sqrt(fx * fx + fy * fy)
    bins = np.minimum((f / 0.7071 * 64).astype(int), 63)
    prof = np.bincount(bins.ravel(), F.ravel()) / np.maximum(np.bincount(bins.ravel()), 1)
    prof = ndimage.gaussian_filter1d(prof, 1.0)
    S = prof[bins]
    S[0, 0] = max(prof[0], 1e-6)
    return np.maximum(S, 1e-9)


def noise_var_flat(level, ratio=1.0):
    """Per-pixel variance of the normalised image at mean 1: (shot + read) / level^2."""
    n = level * ratio
    return (n + READ_NOISE ** 2) / (n * n)


def register_uw(U, M):
    """Estimate sub-pixel shift (main px) of U (resampled to the main grid) w.r.t. M; return registered upsampled U."""
    cy, cx = (NU_H - 1) / 2.0, (NU_W - 1) / 2.0
    iy = (np.arange(NH) - (NH - 1) / 2.0) * R_UW + cy
    ix = (np.arange(NM) - (NM - 1) / 2.0) * R_UW + cx

    def resample(dx, dy):
        yy, xx = np.meshgrid(iy + dy * R_UW, ix + dx * R_UW, indexing="ij")
        return ndimage.map_coordinates(U, [yy, xx], order=3, mode="nearest")

    Ml = ndimage.gaussian_filter(M, 2.0)

    def cost(p):
        Ul = ndimage.gaussian_filter(resample(p[0], p[1]), 2.0)
        d = (Ml - Ul)[24:-24, 24:-24]
        return float(np.median(np.abs(d)))  # robust against a foreground layer

    best = None
    for x0 in (-6, -2, 0, 2, 6):
        r = optimize.minimize(cost, [x0, 0.0], method="Nelder-Mead", options={"xatol": 0.02, "fatol": 1e-7, "maxiter": 120})
        if best is None or r.fun < best.fun:
            best = r
    return resample(best.x[0], best.x[1]), best.x


def gate_map(M, Ureg, sig_res):
    r = ndimage.gaussian_filter(np.abs(ndimage.gaussian_filter(M, 2.0) - ndimage.gaussian_filter(Ureg, 2.0)), 3.0)
    return 1.0 / (1.0 + (r / (3.0 * sig_res)) ** 4)


def psnr(a, b, peak):
    c = 24
    d = (a - b)[c:-c, c:-c]
    return 10 * np.log10(peak * peak / np.mean(d * d))


def run_case(seed, level, disparity_main_px, sig_uw_px, shift_err_px=0.37, bursts=4):
    rng = np.random.default_rng(seed)
    th, tw = 1024, 1280
    bg = make_scene(rng, th, tw)
    fg = make_scene(rng, th, tw, detail=1.3) * 1.05
    mask = np.zeros((th, tw))
    mask[300:700, 420:760] = 1.0  # centre-left block, near object
    mask = ndimage.gaussian_filter(mask, 1.0)
    truth_m = bg * (1 - mask) + fg * mask
    d_t = disparity_main_px * K
    if disparity_main_px:
        fg_s = ndimage.shift(fg, (0, d_t), order=1, mode="nearest")
        mask_s = ndimage.shift(mask, (0, d_t), order=1, mode="nearest")
        truth_u = bg * (1 - mask_s) + fg_s * mask_s
    else:
        truth_u = truth_m
    sig_m_deg = 0.45 / PPD_MAIN
    sig_u_deg = sig_uw_px / PPD_UW
    # ideal reference on the main grid: pixel-integrated, no lens blur
    ref, _ = capture(truth_m, PPD_MAIN, NM, NH, 0.0, (0, 0), 1, rng, noisy=False)
    peak = ref.max()
    M1, _ = capture(truth_m, PPD_MAIN, NM, NH, sig_m_deg, (0, 0), level, rng)
    Mb = [M1] + [capture(truth_m, PPD_MAIN, NM, NH, sig_m_deg, (0, 0), level, rng)[0] for _ in range(bursts - 1)]
    lev_u = level * PHOTON_RATIO_UW
    # true global misalignment between the two cameras (main px), unknown to the pipeline
    sh = shift_err_px / PPD_MAIN
    U, _ = capture(truth_u, PPD_UW, NU_W, NU_H, sig_u_deg, (sh, -0.6 * sh), lev_u, rng)
    S = radial_prior(ref)
    sm2 = noise_var_flat(level)
    su2 = noise_var_flat(lev_u)
    # A: main alone (Wiener)
    A = wiener_estimate(M1, sm2, None, 0, 0.45, 0, S)
    # C: burst of N frames (ideal registration), Wiener with variance / N
    C = wiener_estimate(np.mean(Mb, axis=0), sm2 / bursts, None, 0, 0.45, 0, S)
    # B: fusion with registration + gate
    Ureg, shift_est = register_uw(U, M1)
    Fw = wiener_estimate(M1, sm2, Ureg, su2, 0.45, sig_uw_px, S)
    # residual std of low-passed difference (from noise only, known sensor model)
    fl = np.random.default_rng(1)
    flat_m = fl.poisson(level * np.ones((NH, NM))) / level + fl.standard_normal((NH, NM)) * READ_NOISE / level
    flat_u = fl.poisson(lev_u * np.ones((NH, NM))) / lev_u + fl.standard_normal((NH, NM)) * READ_NOISE / lev_u
    flat_u = ndimage.gaussian_filter(flat_u, R_UW ** -1 * 0.5)  # upsampling interpolation smooths noise
    sig_res = float(np.std(ndimage.gaussian_filter(flat_m - flat_u, 2.0)))
    g = gate_map(M1, Ureg, max(sig_res, 1e-4))
    B = g * Fw + (1 - g) * A
    # oracle shift (no registration error) upper bound for the plain fusion
    pa, pb, pc = psnr(A, ref, peak), psnr(B, ref, peak), psnr(C, ref, peak)
    pm = psnr(M1, ref, peak)
    return dict(raw=pm, A=pa, B=pb, C=pc, shift=shift_est, gate_mean=float(g[24:-24, 24:-24].mean()))


# ---- slanted edge MTF ----
def edge_scene(th, tw, angle_deg=5.0):
    yy, xx = np.mgrid[0:th, 0:tw]
    a = np.radians(angle_deg)
    d = (xx - tw / 2) * np.cos(a) + (yy - th / 2) * np.sin(a)
    return np.where(d > 0, 1.6, 0.4).astype(np.float64)  # contrast 4:1 -> mean 1.0


def mtf50_edge(img, ppd, angle_deg=5.0):
    h, w = img.shape
    yy, xx = np.mgrid[0:h, 0:w]
    a = np.radians(angle_deg)
    d = (xx - w / 2.0) * np.cos(a) + (yy - h / 2.0) * np.sin(a)
    sel = np.abs(d) < 14
    bins = np.round(d[sel] * 4).astype(int)
    bins -= bins.min()
    esf = np.bincount(bins, img[sel]) / np.maximum(np.bincount(bins), 1)
    lsf = np.diff(esf) * np.hanning(len(esf) - 1)
    mtf = np.abs(np.fft.rfft(lsf, 2048))
    mtf /= mtf[0]
    f = np.fft.rfftfreq(2048, d=0.25)  # cyc/px of this grid
    below = np.where(mtf < 0.5)[0]
    f50 = f[below[0]] if len(below) else f[-1]
    return f50 * ppd  # cycles per degree


def edge_case(seed, level, sig_uw_px):
    rng = np.random.default_rng(seed)
    th, tw = 1024, 1280
    sc = edge_scene(th, tw)
    sig_m = 0.45 / PPD_MAIN
    sig_u = sig_uw_px / PPD_UW
    M1, _ = capture(sc, PPD_MAIN, NM, NH, sig_m, (0, 0), level, rng)
    Mb = [M1] + [capture(sc, PPD_MAIN, NM, NH, sig_m, (0, 0), level, rng)[0] for _ in range(3)]
    lev_u = level * PHOTON_RATIO_UW
    U, _ = capture(sc, PPD_UW, NU_W, NU_H, sig_u, (0.37 / PPD_MAIN, -0.22 / PPD_MAIN), lev_u, rng)
    ref, _ = capture(sc, PPD_MAIN, NM, NH, 0.0, (0, 0), 1, rng, noisy=False)
    S = radial_prior(ref)
    sm2, su2 = noise_var_flat(level), noise_var_flat(lev_u)
    A = wiener_estimate(M1, sm2, None, 0, 0.45, 0, S)
    C = wiener_estimate(np.mean(Mb, axis=0), sm2 / 4, None, 0, 0.45, 0, S)
    Ureg, _ = register_uw(U, M1)
    B = wiener_estimate(M1, sm2, Ureg, su2, 0.45, sig_uw_px, S)
    # 50 MP mode: sensor at 2x pixel density, full-res samples, same lens, 4x less light per pixel
    NF = (NM * 2, NH * 2)
    F1, _ = capture(sc, PPD_FULL, NF[0], NF[1], sig_m, (0, 0), level / 4.0, rng)
    out = {"main_raw": mtf50_edge(M1, PPD_MAIN), "main_wiener": mtf50_edge(A, PPD_MAIN), "fusion": mtf50_edge(B, PPD_MAIN),
           "burst4": mtf50_edge(C, PPD_MAIN), "uw_alone": mtf50_edge(U, PPD_UW), "main_50mp_raw": mtf50_edge(F1, PPD_FULL)}
    return out


def main():
    quick = "--quick" in sys.argv
    for line in analytic():
        print(line)
    seeds = [11] if quick else [11, 12, 13]
    levels = [1000, 100, 10] if not quick else [100]
    disps = [0, 5, 20] if not quick else [0, 20]
    print("\n== PSNR vs ideal main-grid image (dB), mean over %d seeds, UW lens sigma 0.6 px ==" % len(seeds))
    print("level(e-/px)  disp(px)   raw     main    fused   d(fuse-main)  burst4  gate")
    for lv in levels:
        for dsp in disps:
            rs = [run_case(s, lv, dsp, 0.6) for s in seeds]
            m = {k: float(np.mean([r[k] for r in rs])) for k in ("raw", "A", "B", "C", "gate_mean")}
            print("%6d        %3d     %6.2f  %6.2f  %6.2f   %+6.2f       %6.2f  %.2f" % (
                lv, dsp, m["raw"], m["A"], m["B"], m["B"] - m["A"], m["C"], m["gate_mean"]))
    print("\n== sensitivity: UW lens sigma (px of own grid), level 100, disparity 0 ==")
    for sg in (0.45, 0.6, 0.9, 1.2):
        r = run_case(21, 100, 0, sg)
        print("uw_sigma %.2f  main %.2f  fused %.2f  d %+.2f" % (sg, r["A"], r["B"], r["B"] - r["A"]))
    print("\n== slanted-edge MTF50 (cycles/degree), level 1000 / 100 ==")
    for lv in ((1000,) if quick else (1000, 100)):
        e = edge_case(5, lv, 0.6)
        print("level %d: " % lv + "  ".join("%s %.1f" % (k, v) for k, v in e.items()))
    print("nyquist cyc/deg: main %.1f, uw %.1f, main50mp %.1f" % (PPD_MAIN / 2, PPD_UW / 2, PPD_FULL / 2))


if __name__ == "__main__":
    main()

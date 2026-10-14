# SPDX-License-Identifier: MPL-2.0
# tools/desktop/tscale.py -- shared helpers of the desktop checks (r580).
#   T(seconds)      a timeout scaled by the machine load (1.0 .. 4.0); override: FIRN_TIME_SCALE=<float>
#   free_display()  an X display number nobody uses (random start: parallel runs rarely collide)
#   scratch_dir()   a private scratch directory under $TMPDIR (or the system temp dir), created fresh
import os, random, tempfile


def _scale():
    v = os.environ.get("FIRN_TIME_SCALE")
    if v:
        try:
            return max(1.0, float(v))
        except ValueError:
            pass
    try:
        load = os.getloadavg()[0]
    except OSError:
        return 1.0
    cpus = os.cpu_count() or 1
    return max(1.0, min(4.0, load / max(1.0, cpus / 8.0)))


SCALE = _scale()


def T(seconds):
    return seconds * SCALE


def free_display(lo=250, hi=400):
    nums = list(range(lo, hi))
    random.shuffle(nums)
    for n in nums:
        if not os.path.exists("/tmp/.X11-unix/X%d" % n) and not os.path.exists("/tmp/.X%d-lock" % n):
            return n
    return None


def scratch_dir(prefix):
    # Under the repo (.test-work/, git-ignored): a cleaner of the system temp dir (heavy's per-run
    # $TMPDIR) can no longer pull it away mid-run. Falls back to the system temp dir.
    base = os.path.join(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))), ".test-work")
    try:
        os.makedirs(base, exist_ok=True)
        return tempfile.mkdtemp(prefix=prefix, dir=base)
    except OSError:
        return tempfile.mkdtemp(prefix=prefix)

import sys
import numpy as np
from PIL import Image

def make(w, h, seed):
    rng = np.random.default_rng(seed)
    y, x = np.mgrid[0:h, 0:w].astype(np.float64)
    r = 127 + 90 * np.sin(x / 37.0 + y / 91.0)
    g = 127 + 90 * np.cos(y / 53.0 - x / 131.0)
    b = 127 + 60 * np.sin((x + y) / 71.0)
    img = np.stack([r, g, b], axis=2)
    tex = rng.integers(0, 40, size=(h, w, 1)).astype(np.float64)
    mask = ((x // 160 + y // 120) % 3 == 0)[..., None]
    img = np.where(mask, img * 0.5 + tex * 3, img)
    edge = ((x * 0.7 + y) % 200 < 6)[..., None]
    img = np.where(edge, 255 - img, img)
    return Image.fromarray(np.clip(img, 0, 255).astype(np.uint8), "RGB")

out, w, h = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
make(w, h, 7).save(out)

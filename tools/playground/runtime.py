import os
import shutil

import config

RUNTIME_DIR = os.environ.get("PLAYGROUND_RUNTIME", "/var/tmp/firn-playground-runtime")
STAMP_FILE = "stamp"


def tree_stamp():
    compiler = os.stat(config.FIRNC)
    newest = max(
        (os.lstat(os.path.join(base, name)).st_mtime_ns for base, _, names in os.walk(config.FIRNLIB) for name in names),
        default=0,
    )
    return f"{compiler.st_size}:{compiler.st_mtime_ns}:{newest}"


def stage():
    stamp = tree_stamp()
    stamp_path = os.path.join(RUNTIME_DIR, STAMP_FILE)
    if os.path.exists(stamp_path) and open(stamp_path).read() == stamp:
        return RUNTIME_DIR
    shutil.rmtree(RUNTIME_DIR, ignore_errors=True)
    os.makedirs(os.path.join(RUNTIME_DIR, "bin"), mode=0o755)
    shutil.copy2(config.FIRNC, os.path.join(RUNTIME_DIR, "bin", "firnc"))
    shutil.copytree(config.FIRNLIB, os.path.join(RUNTIME_DIR, "lib"), symlinks=True)
    for base, dirs, names in os.walk(RUNTIME_DIR):
        for entry in dirs:
            path = os.path.join(base, entry)
            if not os.path.islink(path):
                os.chmod(path, 0o755)
        for entry in names:
            path = os.path.join(base, entry)
            if not os.path.islink(path):
                os.chmod(path, 0o755 if base.endswith("bin") else 0o644)
    os.chmod(RUNTIME_DIR, 0o755)
    with open(stamp_path, "w") as handle:
        handle.write(stamp)
    return RUNTIME_DIR

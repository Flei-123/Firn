import os

ROOT = os.path.realpath(os.path.join(os.path.dirname(__file__), "..", ".."))
HERE = os.path.dirname(os.path.realpath(__file__))

HOST = os.environ.get("PLAYGROUND_HOST", "127.0.0.1")
PORT = int(os.environ.get("PLAYGROUND_PORT", "8123"))
FIRNC = os.environ.get("PLAYGROUND_FIRNC", os.path.join(ROOT, "compiler", "target", "release", "firnc"))
FIRNLIB = os.environ.get("PLAYGROUND_FIRNLIB", os.path.join(ROOT, "lib"))

COMPILE_TIMEOUT_S = float(os.environ.get("PLAYGROUND_COMPILE_TIMEOUT", "20"))
RUN_TIMEOUT_S = float(os.environ.get("PLAYGROUND_RUN_TIMEOUT", "10"))
MAX_SOURCE_BYTES = 64 * 1024
MAX_OUTPUT_BYTES = 64 * 1024
MAX_PARALLEL_JOBS = int(os.environ.get("PLAYGROUND_JOBS", "4"))
MAX_PARALLEL_CHECKS = int(os.environ.get("PLAYGROUND_CHECKS", "4"))
CHECK_TIMEOUT_S = float(os.environ.get("PLAYGROUND_CHECK_TIMEOUT", "8"))

COMPILE_ADDRESS_SPACE = 4 << 30
RUN_ADDRESS_SPACE = 512 << 20
RUN_FILE_SIZE = 1 << 20
MAX_PROCESSES = 96
SANDBOX_UID = 65534

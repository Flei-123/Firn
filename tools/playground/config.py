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
MAX_SESSIONS = int(os.environ.get("PLAYGROUND_SESSIONS", "4"))
SESSION_TOTAL_TIMEOUT_S = float(os.environ.get("PLAYGROUND_SESSION_TIMEOUT", "60"))
SESSION_IDLE_TIMEOUT_S = float(os.environ.get("PLAYGROUND_SESSION_IDLE", "30"))
SESSION_ABANDON_S = float(os.environ.get("PLAYGROUND_SESSION_ABANDON", "8"))
SESSION_KEEP_S = 60
SESSION_CPU_S = 20
SESSION_HEARTBEAT_S = float(os.environ.get("PLAYGROUND_HEARTBEAT", "5"))
MAX_STDIN_BYTES = 16 * 1024
MAX_STDIN_REQUEST_BYTES = 8 * 1024

COMPILE_ADDRESS_SPACE = 4 << 30
RUN_ADDRESS_SPACE = 512 << 20
RUN_FILE_SIZE = 1 << 20
MAX_PROCESSES = 96
SANDBOX_UID = 65534
MAX_SESSIONS_PER_CLIENT = int(os.environ.get("PLAYGROUND_SESSIONS_PER_CLIENT", "2"))

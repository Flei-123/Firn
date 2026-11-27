import json
import os
import socket
import subprocess
import sys
import urllib.error
import urllib.request

HERE = os.path.dirname(os.path.realpath(__file__))


def free_port():
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        return probe.getsockname()[1]


def call(port, path, payload=None, raw=None):
    data = raw if raw is not None else (json.dumps(payload).encode() if payload is not None else None)
    try:
        with urllib.request.urlopen(urllib.request.Request(f"http://127.0.0.1:{port}{path}", data=data), timeout=60) as reply:
            return reply.status, reply.read()
    except urllib.error.HTTPError as failure:
        return failure.code, failure.read()


SESSION_ENV = {
    "PLAYGROUND_SESSION_TIMEOUT": "6", "PLAYGROUND_SESSION_IDLE": "3", "PLAYGROUND_SESSION_ABANDON": "1",
    "PLAYGROUND_HEARTBEAT": "0.3",
}


def start_server(port, runtime_dir, run_timeout_s):
    env = dict(os.environ, PLAYGROUND_PORT=str(port), PLAYGROUND_RUN_TIMEOUT=str(run_timeout_s), PLAYGROUND_RUNTIME=runtime_dir, **SESSION_ENV)
    process = subprocess.Popen([sys.executable, os.path.join(HERE, "server.py")], env=env, cwd=HERE, stdout=subprocess.PIPE)
    process.stdout.readline()
    return process

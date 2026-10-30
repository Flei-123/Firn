import os, signal, struct, subprocess, sys, time, glob, argparse

p = argparse.ArgumentParser()
p.add_argument("--jar", required=True)
p.add_argument("--java", required=True)
p.add_argument("--out", required=True)
p.add_argument("--port", type=int, default=25777)
p.add_argument("--compression", default="")
p.add_argument("--bot", default="")
p.add_argument("--chunks", type=int, default=256)
p.add_argument("--seed", default="424242")
p.add_argument("--bot-only", action="store_true")
p.add_argument("--player-name", default="SaveBot")
a = p.parse_args()

os.makedirs(a.out, exist_ok=True)
open(os.path.join(a.out, "eula.txt"), "w").write("eula=true\n")
props = {
    "level-seed": a.seed, "online-mode": "false", "server-port": str(a.port),
    "view-distance": "4", "simulation-distance": "4", "spawn-protection": "0",
    "max-tick-time": "-1", "enable-command-block": "true", "motd": "savetest",
    "spawn-monsters": "false", "spawn-animals": "true", "sync-chunk-writes": "false",
}
if a.compression:
    props["region-file-compression"] = a.compression
open(os.path.join(a.out, "server.properties"), "w").write("".join(f"{k}={v}\n" for k, v in props.items()))
logp = os.path.join(a.out, "server.out")
log = open(logp, "wb")
srv = subprocess.Popen([a.java, "-Xmx3G", "-jar", a.jar, "--nogui"], cwd=a.out, stdin=subprocess.PIPE, stdout=log, stderr=subprocess.STDOUT)

def send(cmd):
    srv.stdin.write((cmd + "\n").encode()); srv.stdin.flush()

def wait_for(text, timeout):
    end = time.time() + timeout
    while time.time() < end:
        if srv.poll() is not None:
            return False
        if text.encode() in open(logp, "rb").read():
            return True
        time.sleep(0.5)
    return False

def count_chunks():
    n = 0
    for f in glob.glob(os.path.join(a.out, "world", "**", "region", "r.*.mca"), recursive=True):
        h = open(f, "rb").read(4096)
        if len(h) == 4096:
            n += sum(1 for i in range(1024) if h[i * 4:i * 4 + 4] != b"\0\0\0\0")
    return n

if not wait_for("Done (", 300):
    print("server did not start", file=sys.stderr); srv.kill(); sys.exit(2)

if a.bot:
    bot = subprocess.Popen(["node", a.bot, "127.0.0.1", str(a.port), a.player_name], env=dict(os.environ, NODE_PATH=os.environ.get("NODE_PATH", "/root/firn/tools/mcserver/node_modules")),
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    if wait_for(a.player_name + " joined the game", 60):
        time.sleep(1)
        send("tp " + a.player_name + " 100.5 90.0 -50.5")
        time.sleep(3)
    else:
        print("bot did not join", file=sys.stderr)
    bot.terminate()
    time.sleep(2)

side = 16
if not a.bot_only:
    send("forceload add -128 -128 127 127")
end = time.time() + (0 if a.bot_only else 600)
while time.time() < end:
    time.sleep(8)
    send("save-all flush")
    time.sleep(4)
    if count_chunks() >= a.chunks:
        break
send("save-all flush")
wait_for("Saved the game", 120)
time.sleep(2)
send("stop")
try:
    srv.wait(120)
except subprocess.TimeoutExpired:
    srv.kill()
print("chunks", count_chunks())

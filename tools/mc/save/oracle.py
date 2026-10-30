import argparse, os, re, subprocess, sys, time

p = argparse.ArgumentParser()
p.add_argument("--jar", required=True)
p.add_argument("--java", required=True)
p.add_argument("--dir", required=True)
p.add_argument("--port", type=int, required=True)
p.add_argument("--picks", required=True)
p.add_argument("--out", required=True)
p.add_argument("--bot", default="")
p.add_argument("--player", default="SaveBot")
p.add_argument("--wrap", default="")
a = p.parse_args()

open(os.path.join(a.dir, "eula.txt"), "w").write("eula=true\n")
props = {"online-mode": "false", "server-port": str(a.port), "view-distance": "4", "simulation-distance": "4",
         "max-tick-time": "-1", "spawn-protection": "0", "spawn-monsters": "false"}
open(os.path.join(a.dir, "server.properties"), "w").write("".join("%s=%s\n" % kv for kv in props.items()))
logp = os.path.join(a.dir, "server.out")
cmd = ([a.wrap] if a.wrap else []) + [a.java, "-Xmx3G", "-jar", a.jar, "--nogui"]
srv = subprocess.Popen(cmd, cwd=a.dir, stdin=subprocess.PIPE, stdout=open(logp, "wb"), stderr=subprocess.STDOUT)


def send(c):
    srv.stdin.write((c + "\n").encode()); srv.stdin.flush()


def log():
    return open(logp, "rb").read().decode("utf8", "replace")


def wait_for(text, timeout, start=0):
    end = time.time() + timeout
    while time.time() < end:
        if srv.poll() is not None:
            return False
        if text in log()[start:]:
            return True
        time.sleep(0.5)
    return False


if not wait_for("Done (", 300):
    print("server did not start", file=sys.stderr)
    srv.kill(); sys.exit(2)

picks = [l.split(" ", 3) for l in open(a.picks).read().splitlines() if l.strip()]
expect = []
cmds = []
for i, (x, y, z, name) in enumerate(picks):
    if i % 10 == 9 and name != "minecraft:barrier":
        cmds.append("execute if block %s %s %s minecraft:barrier" % (x, y, z)); expect.append("failed")
    else:
        cmds.append("execute if block %s %s %s %s" % (x, y, z, name)); expect.append("passed")
send("forceload add -128 -128 127 127")
time.sleep(10)
start = len(log())
for c in cmds:
    send(c)
send("say ORACLE-END")
wait_for("ORACLE-END", 120, start)
seen = re.findall(r"Test (passed|failed)", log()[start:])
ok = seen == expect
res = ["picks=%d answers=%d match=%s" % (len(cmds), len(seen), ok)]
if not ok:
    bad = [i for i, (e, s) in enumerate(zip(expect, seen)) if e != s][:5]
    res.append("first_mismatch=%s" % bad)

if a.bot:
    env = dict(os.environ, NODE_PATH=os.environ.get("NODE_PATH", "/root/firn/tools/mcserver/node_modules"))
    bot = subprocess.Popen(["node", a.bot, "127.0.0.1", str(a.port), a.player], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    if wait_for(a.player + " joined the game", 60):
        time.sleep(2)
        s2 = len(log())
        send("data get entity %s Pos" % a.player)
        wait_for("entity data", 20, s2)
        m = re.search(r"\[(-?[\d.]+)d, (-?[\d.]+)d, (-?[\d.]+)d\]", log()[s2:])
        res.append("player_pos=%s" % (",".join(m.groups()) if m else "none"))
    else:
        res.append("player_pos=nojoin")
    bot.terminate()
    time.sleep(1)

send("stop")
try:
    srv.wait(120)
except subprocess.TimeoutExpired:
    srv.kill()
bad_log = [l for l in log().splitlines() if re.search(r"(WARN|ERROR)", l) and re.search(r"(?i)chunk|region|anvil|nbt|corrupt|compress|\.mca", l)]
res.append("log_chunk_problems=%d" % len(bad_log))
for l in bad_log[:3]:
    res.append("  " + l[:200])
open(a.out, "w").write("\n".join(res) + "\n")
print("\n".join(res))
sys.exit(0 if ok and not bad_log else 1)

#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/android/relay_test.py -- a stand-in relay for lib/plat/android/push.fi.
# Accepts connections on PORT, logs them, and sends each connected client one
# line per LINE argument (default: three test messages), SPACING seconds apart.
#   relay_test.py PORT [--spacing S] [--hold S] [LINE ...]
import socket, sys, time, threading
port = int(sys.argv[1]); args = sys.argv[2:]
spacing = 2.0; hold = 600.0
if "--spacing" in args:
    i = args.index("--spacing"); spacing = float(args[i + 1]); del args[i:i + 2]
if "--hold" in args:
    i = args.index("--hold"); hold = float(args[i + 1]); del args[i:i + 2]
lines = args or ["Hallo von Firn - Nachricht 1", "Umlaute: äöü ß", "Nachricht 3 ohne Firebase"]
srv = socket.socket(); srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
srv.bind(("0.0.0.0", port)); srv.listen(4)
print("relay listening on", port, flush=True)
def serve(c, a):
    print("connected", a, flush=True)
    try:
        for l in lines:
            time.sleep(spacing); c.sendall((l + "\n").encode()); print("sent", l, flush=True)
        time.sleep(hold)
    except OSError as e:
        print("client gone", e, flush=True)
    c.close()
while True:
    c, a = srv.accept(); threading.Thread(target=serve, args=(c, a), daemon=True).start()

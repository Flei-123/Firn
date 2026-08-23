# The regression limits of tools/bench82 — and why they sit where they sit

A limit here is **not** a target. It is a trip wire against a later round
quietly giving the speed back. This is a shared virtual machine; the same
binary measured between 429 and 894 MiB/s for SHA-256 depending on what else
was running, so a limit set at the measured value would fire on the
neighbours and would be taken out again within a round or two.

**The rule: a minimum sits at roughly HALF of what was measured, a maximum
at roughly one and a half times.** That catches a factor of two — which is
what a lost optimisation looks like — and not the noise.

| file | measured (round 87, load average 9) | limit | why |
|---|---:|---:|---|
| `minquota_sha256.txt` | 919.9 MiB/s | 400 | unchanged since round 82 — this round did not touch the crypto path |
| `minquota_aes_cbc.txt` | 580.6 MiB/s | 300 | unchanged |
| `minquota_aes_dec.txt` | 685.3 MiB/s | 320 | unchanged |
| `minquota_cfb8.txt` | 25.7 MiB/s | 14 | unchanged |
| `minquota_deflate.txt` | 11.5 MiB/s | 6.5 | unchanged — round 87's DEFLATE work shows on real text, not on this generated corpus (docs/ROUND87.md §2) |
| `maxquota_self_ms.txt` | **3,323 ms** | **5,000** | **round 87: was 40,000.** The self compile took 5,615 ms before this round, so a limit of 5,000 catches a return to that state, and it still leaves 50 % headroom over what was measured. |

Raising a limit that the round did not move would only add flakiness without
protecting anything new, so the crypto and DEFLATE limits stay where round 82
put them. The one number this round really moved is the self compile, and
that is the one whose limit moved with it.

`tools/bench87` carries two limits of its own, and one of them is not a time
at all:

| file | measured | limit | why |
|---|---:|---:|---|
| `tools/bench87/minquota_json_float.txt` | 15.1 MiB/s | 7.0 | half of it, like the others |
| `tools/bench87/maxsize_deflate6.txt` | 150,357 octets | 150,357 | **exact.** The compression ratio is not a measurement, it is a number the program computes; it does not fluctuate, so its limit does not have to. One octet more and the run fails. |

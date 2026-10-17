#!/usr/bin/env bash
# MANUAL / OPTIONAL interop test: lib/fleitec_login (Firn) against the real Node service fleitec-id.
# Needs node + curl + the fleitec-id repo. Binds 127.0.0.1:8470 only, fake backend, temp data, no real accounts.
#   tests/manual/fleitec_login_interop.sh   (exit 0 = all checks passed)
set -u
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SVC="${FLEITEC_ID_DIR:-/root/jarvis/projects/u_DiS4in7esMF1/fleitec-id}"
FIRNC="${FIRNC:-$ROOT/compiler/target/release/firnc}"
[ -x "$FIRNC" ] || FIRNC=/root/jarvis/projects/u_DiS4in7esMF1/firn/compiler/target/release/firnc
export FIRNLIB="$ROOT/lib"
I=http://127.0.0.1:8470
D=$(mktemp -d); PID=""
trap '[ -n "$PID" ] && kill $PID 2>/dev/null; kill $CO_PID 2>/dev/null; rm -rf "$D"' EXIT
fail=0; chk() { if [ "$2" = "$3" ]; then echo "ok   $1"; else echo "FAIL $1 (got '$2' want '$3')"; fail=$((fail+1)); fi; }
( cd "$SVC" && exec node bin/fleitec-id.js --fake --init-key --port 8470 --client-redirect http://127.0.0.1:8765/callback ) > "$D/svc.log" 2>&1 &
PID=$!
for i in $(seq 1 30); do curl -sf $I/.well-known/openid-configuration >/dev/null && break; sleep 0.3; done
"$FIRNC" -o "$D/interop" "$ROOT/tests/manual/fleitec_login_interop.fi" || { echo "FAIL compile"; exit 1; }
coproc FP { "$D/interop"; }
CO_PID=$FP_PID
rd() { local k; IFS= read -r -u "${FP[0]}" line; echo "$line"; }
get_hdr() { sed -n "s/^[Ll]ocation: *//p" "$1" | tr -d '\r'; }
field() { sed -n "s/.*name=\"$1\" value=\"\([^\"]*\)\".*/\1/p" | head -1 | sed 's/&amp;/\&/g; s/&#39;/'"'"'/g; s/&quot;/"/g'; }
# ---- the browser: log in as justin, approve, stop at the callback
browse() {
  local url="$1" jar="$D/jar" n=0 loc
  while [ $n -lt 8 ]; do
    n=$((n+1))
    curl -s -o "$D/body" -D "$D/hdr" -b "$jar" -c "$jar" "$url"
    loc=$(get_hdr "$D/hdr")
    if [ -n "$loc" ]; then case "$loc" in http://127.0.0.1:8765/callback*) echo "$loc"; return;; /*) url="$I$loc";; *) url="$loc";; esac; continue; fi
    if grep -q 'action="/oidc/login"' "$D/body"; then
      csrf=$(field csrf < "$D/body"); ret=$(field return < "$D/body")
      curl -s -o /dev/null -D "$D/hdr" -b "$jar" -c "$jar" -H "Origin: $I" --data-urlencode "csrf=$csrf" --data-urlencode "return=$ret" --data-urlencode username=justin --data-urlencode password=test-password-1 $I/oidc/login
      loc=$(get_hdr "$D/hdr"); case "$loc" in /*) url="$I$loc";; *) url="$loc";; esac; continue
    fi
    if grep -q 'action="/oidc/consent"' "$D/body"; then
      csrf=$(field csrf < "$D/body"); ret=$(field return < "$D/body")
      curl -s -o /dev/null -D "$D/hdr" -b "$jar" -c "$jar" -H "Origin: $I" --data-urlencode "csrf=$csrf" --data-urlencode "return=$ret" --data-urlencode decision=allow $I/oidc/consent
      loc=$(get_hdr "$D/hdr"); case "$loc" in /*) url="$I$loc";; *) url="$loc";; esac; continue
    fi
    echo "STUCK: $(head -c 200 "$D/body")"; return
  done
}
l=$(rd); chk discovery "$l" "DISCOVER ok"
l=$(rd); url=${l#URL }; red=$(browse "$url"); echo "     callback: ${red:0:60}..."
echo "$red" >&"${FP[1]}"
l=$(rd); chk code_flow "$l" "CODEFLOW ok "
l=$(rd); echo "     $l"; case "$l" in "SUB "*" idp=jarvis") chk sub_claims ok ok;; *) chk sub_claims "$l" "SUB .. idp=jarvis";; esac
l=$(rd); chk refresh "$l" "REFRESH ok"
l=$(rd); chk rotation "$l" "ROTATED yes"
l=$(rd); chk reuse_refused "$l" "REUSE need_login"
l=$(rd); chk family_dead "$l" "OWNER_AFTER_REUSE need_login"
l=$(rd); chk device_start "$l" "DEVICE_START ok"
l=$(rd); code=${l#USER_CODE }
# ---- the phone: log in, enter the code, allow
jar="$D/jar2"
curl -s -o "$D/body" -c "$jar" -b "$jar" "$I/oidc/login?return=%2Foidc%2Fdevice"; csrf=$(field csrf < "$D/body")
curl -s -o /dev/null -c "$jar" -b "$jar" -H "Origin: $I" --data-urlencode "csrf=$csrf" --data-urlencode "return=/oidc/device" --data-urlencode username=justin --data-urlencode password=test-password-1 $I/oidc/login
curl -s -o "$D/body" -c "$jar" -b "$jar" "$I/oidc/device"; csrf=$(field csrf < "$D/body")
curl -s -o "$D/body" -D "$D/hdr" -c "$jar" -b "$jar" -H "Origin: $I" --data-urlencode "csrf=$csrf" --data-urlencode "user_code=$code" $I/oidc/device
conf=$(get_hdr "$D/hdr"); [ -z "$conf" ] && { csrf=$(field csrf < "$D/body"); }
curl -s -o "$D/body" -c "$jar" -b "$jar" "$I/oidc/device/confirm?user_code=$code"; csrf=$(field csrf < "$D/body")
curl -s -o /dev/null -c "$jar" -b "$jar" -H "Origin: $I" --data-urlencode "csrf=$csrf" --data-urlencode "user_code=$code" --data-urlencode decision=allow $I/oidc/device/decision
sleep 6
echo go >&"${FP[1]}"
l=$(rd); chk device_poll "$l" "DEVICE_POLL ok "
l=$(rd); echo "     $l"; case "$l" in DEVICE_SUB*) chk device_sub ok ok;; *) chk device_sub "$l" "DEVICE_SUB ..";; esac
wait $CO_PID 2>/dev/null
echo "failures: $fail"; [ $fail -eq 0 ]

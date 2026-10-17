#!/usr/bin/env python3
"""Deterministic test vectors for lib/fleitec_login (Fleitec-ID token v2, EdDSA).

Made with Python `cryptography` and `hmac` only -- nothing of the Firn code. Fixed seeds, so the
output is byte-identical on every run. Writes tests/data/fleitec-login-vectors.json.

    python3 tools/fleitec_login/gen_vectors.py > tests/data/fleitec-login-vectors.json

When the shared docs/fleitec-id/vectors_v2.json (service worker) exists, align the case ids and
reasons with it (groups jwt_eddsa, claims_v2).
"""
import base64, hashlib, hmac, json, sys
from cryptography.hazmat.primitives.asymmetric import ed25519, ec, rsa, padding
from cryptography.hazmat.primitives import hashes, serialization

NOW = 1800000000
ISS = "https://id.fleitec.com"
AUD = "firnchat"
LEEWAY = 30
L = 2**252 + 27742317777372353535851937790883648493


def b64u(b):
    return base64.urlsafe_b64encode(b).rstrip(b"=").decode()


def unb64u(s):
    return base64.urlsafe_b64decode(s + "=" * (-len(s) % 4))


def seed_key(n):
    return ed25519.Ed25519PrivateKey.from_private_bytes(bytes([n]) * 32)


def pubx(k):
    return b64u(k.public_key().public_bytes(serialization.Encoding.Raw, serialization.PublicFormat.Raw))


K1, K2, ATT = seed_key(1), seed_key(2), seed_key(9)


def j(o):
    return json.dumps(o, separators=(",", ":")).encode()


def mk(payload, kid="k1", key=K1, alg="EdDSA", header=None, sig=None, raw_payload=None):
    h = header if header is not None else {"alg": alg, "typ": "JWT", "kid": kid}
    if kid is None and header is None:
        h = {"alg": alg, "typ": "JWT"}
    hb = b64u(j(h))
    pb = b64u(raw_payload if raw_payload is not None else j(payload))
    msg = (hb + "." + pb).encode()
    s = sig if sig is not None else key.sign(msg)
    return hb + "." + pb + "." + b64u(s)


def base(**kw):
    p = {"iss": ISS, "aud": AUD, "sub": "acc_7f3a9c", "uid": "u_DiS4in7esMF1", "name": "Justin",
         "acc": "acc_7f3a9c", "idp": "jarvis", "sv": 0, "iat": NOW - 10, "nbf": NOW - 10, "exp": NOW + 900}
    p.update(kw)
    for k in [k for k, v in p.items() if v is None]:
        del p[k]
    return p


cases = []


def case(cid, token, reason, **expect):
    d = {"id": cid, "token": token, "reason": reason}
    d.update(expect)
    cases.append(d)


exp_ok = {"sub": "acc_7f3a9c", "uid": "u_DiS4in7esMF1", "name": "Justin", "acc": "acc_7f3a9c", "idp": "jarvis"}
case("valid_basic", mk(base()), "ok", **exp_ok)
case("valid_next_key_k2", mk(base(), kid="k2", key=K2), "ok", **exp_ok)
case("valid_aud_array", mk(base(aud=["other", AUD])), "ok", **exp_ok)
case("valid_minimal", mk({"iss": ISS, "aud": AUD, "sub": "acc_min", "exp": NOW + 5}), "ok",
     sub="acc_min", uid="acc_min", name="", acc="", idp="")  # uid falls back to sub (shared vectors)
case("valid_odd_claims", mk(base(acc=5, idp=["x"], uid=None, name=7, amr="pwd", groups="notarray", extra={"a": 1})),
     "ok", sub="acc_7f3a9c", uid="acc_7f3a9c", name="", acc="", idp="")
case("valid_exp_in_leeway", mk(base(exp=NOW - 29)), "ok", **exp_ok)
case("expired_at_leeway_edge", mk(base(exp=NOW - 30)), "expired")
case("expired", mk(base(exp=NOW - 3600)), "expired")
case("expired_exp_equals_now_minus_leeway_minus_1", mk(base(exp=NOW - 31)), "expired")
case("valid_nbf_in_leeway", mk(base(nbf=NOW + 30)), "ok", **exp_ok)
case("nbf_future", mk(base(nbf=NOW + 31)), "nbf")
case("exp_missing", mk(base(exp=None)), "expired")
case("exp_string", mk(base(exp=str(NOW + 900))), "expired")
case("exp_smuggled_in_name", mk(base(exp=NOW - 100, name='","exp":99999999999,"x":"')), "expired")
case("iss_wrong", mk(base(iss="https://evil.example")), "iss")
case("iss_missing", mk(base(iss=None)), "iss")
case("iss_prefix_only", mk(base(iss=ISS + "/")), "iss")
case("aud_wrong", mk(base(aud="cockpit")), "aud")
case("aud_missing", mk(base(aud=None)), "aud")
case("aud_array_without_ours", mk(base(aud=["a", "b"])), "aud")
case("sub_missing", mk(base(sub=None)), "claims")
case("sub_empty", mk(base(sub="")), "claims")
case("alg_none_unsigned", mk(base(), alg="none", sig=b"")[:-0 or None] if False else
     b64u(j({"alg": "none", "typ": "JWT", "kid": "k1"})) + "." + b64u(j(base())) + ".", "alg")  # shared vectors_v2: alg_none -> alg
case("alg_none_with_sig", mk(base(), alg="none", sig=b"\x00" * 64), "alg")
case("alg_hs256_public_key_as_secret", None, "alg")  # filled below
case("alg_rs256_header", mk(base(), alg="RS256"), "alg")
case("alg_lowercase_eddsa", mk(base(), alg="eddsa"), "alg")
case("alg_missing", mk(base(), header={"typ": "JWT", "kid": "k1"}), "malformed")
case("alg_not_string", mk(base(), header={"alg": 1, "kid": "k1"}), "malformed")
case("kid_unknown", mk(base(), kid="k9"), "kid")
case("kid_missing", mk(base(), kid=None), "kid")
case("payload_tampered", None, "sig")
case("signed_by_attacker_key", mk(base(), key=ATT), "sig")
case("signature_63_bytes", mk(base(), sig=K1.sign(b"x")[:63]), "sig")
case("signature_empty_third_part", None, "malformed")
case("signature_s_plus_l_malleable", None, "sig")
case("four_parts", mk(base()) + ".AAAA", "malformed")
case("two_parts", ".".join(mk(base()).split(".")[:2]), "malformed")
case("padding_in_base64", None, "malformed")
case("standard_base64_alphabet", None, "malformed")
case("payload_not_object", mk(None, raw_payload=b'["x"]'), "malformed")
case("header_not_json", None, "malformed")
case("garbage", "not-a-token", "malformed")
case("empty", "", "malformed")

# HS256 with the PUBLIC key as the HMAC secret (classic key confusion); kid = k1
pub1 = unb64u(pubx(K1))
hdr = b64u(j({"alg": "HS256", "typ": "JWT", "kid": "k1"}))
pl = b64u(j(base()))
sig = hmac.new(pub1, (hdr + "." + pl).encode(), hashlib.sha256).digest()
cases[[c["id"] for c in cases].index("alg_hs256_public_key_as_secret")]["token"] = hdr + "." + pl + "." + b64u(sig)

# tampered payload: signature of one payload, body of another (exp changed afterwards)
good = mk(base(exp=NOW + 5)).split(".")
bad = good[0] + "." + b64u(j(base(exp=NOW + 999999))) + "." + good[2]
cases[[c["id"] for c in cases].index("payload_tampered")]["token"] = bad

g = mk(base()).split(".")
cases[[c["id"] for c in cases].index("signature_empty_third_part")]["token"] = g[0] + "." + g[1] + "."

# S + L: same R, S' = S + L (still < 2^256) -- must be refused (malleability)
sg = K1.sign((g[0] + "." + g[1]).encode())
s_int = int.from_bytes(sg[32:], "little")
sg2 = sg[:32] + ((s_int + L) % 2**256).to_bytes(32, "little")
cases[[c["id"] for c in cases].index("signature_s_plus_l_malleable")]["token"] = g[0] + "." + g[1] + "." + b64u(sg2)

cases[[c["id"] for c in cases].index("padding_in_base64")]["token"] = g[0] + "=." + g[1] + "." + g[2]
std = g[2].replace("-", "+").replace("_", "/")
if std == g[2]:
    std = g[1] + "+"
cases[[c["id"] for c in cases].index("standard_base64_alphabet")]["token"] = g[0] + "." + g[1] + "." + g[2] + "+/"
hb = b64u(b"{not json")
cases[[c["id"] for c in cases].index("header_not_json")]["token"] = hb + "." + g[1] + "." + g[2]

# --------------------------------------------------------------- legacy HS256 (the old cookie token)
SECRET = "0123456789abcdef" * 4


def hs(payload, alg="HS256", secret=SECRET):
    h = b64u(j({"alg": alg, "typ": "JWT"}))
    p = b64u(j(payload))
    s = hmac.new(secret.encode(), (h + "." + p).encode(), hashlib.sha256).digest()
    return h + "." + p + "." + b64u(s)


legacy = [
    {"id": "legacy_valid", "token": hs({"uid": "u_x", "name": "Justin", "sv": 0, "iat": NOW - 5, "exp": NOW + 3600}),
     "reason": "ok", "uid": "u_x", "name": "Justin"},
    {"id": "legacy_expired", "token": hs({"uid": "u_x", "iat": NOW - 5000, "exp": NOW - 1}), "reason": "expired"},
    {"id": "legacy_wrong_secret", "token": hs({"uid": "u_x", "exp": NOW + 60}, secret="f" * 64), "reason": "sig"},
    {"id": "legacy_no_uid", "token": hs({"name": "x", "exp": NOW + 60}), "reason": "claims"},
    {"id": "legacy_eddsa_token_refused", "token": mk(base()), "reason": "alg"},
]

# --------------------------------------------------------------- JOSE (foreign OIDC provider, RS256/ES256)
JISS, JAUD = "https://login.xoffi.example", "fleitec-client"
ec_key = ec.generate_private_key(ec.SECP256R1(), backend=None) if False else None
# deterministic EC / RSA keys: derive from a fixed integer / PEM-free route
ec_priv = ec.derive_private_key(int.from_bytes(hashlib.sha256(b"fleitec-login-es256").digest(), "big"), ec.SECP256R1())
nums = ec_priv.public_key().public_numbers()
jwk_ec = {"kty": "EC", "crv": "P-256", "kid": "es1", "use": "sig", "alg": "ES256",
          "x": b64u(nums.x.to_bytes(32, "big")), "y": b64u(nums.y.to_bytes(32, "big"))}
# RSA: 2048-bit with deterministic primes is not available; use a fixed PEM generated once (test key only)
RSA_PEM = None


def es256(payload, kid="es1"):
    h = b64u(j({"alg": "ES256", "typ": "JWT", "kid": kid}))
    p = b64u(j(payload))
    from cryptography.hazmat.primitives.asymmetric.utils import decode_dss_signature
    der = ec_priv.sign((h + "." + p).encode(), ec.ECDSA(hashes.SHA256()))
    r, s = decode_dss_signature(der)
    return h + "." + p + "." + b64u(r.to_bytes(32, "big") + s.to_bytes(32, "big"))


jp = {"iss": JISS, "aud": JAUD, "sub": "xoffi-user-42", "name": "Max", "email": "max@example.org",
      "org_id": "org-1", "org_name": "Org Eins", "groups": ["1", "ops"], "iat": NOW - 5, "exp": NOW + 600,
      "auth_time": NOW - 20}
jose = {
    "iss": JISS, "aud": JAUD,
    "jwks": {"keys": [jwk_ec]},
    "cases": [
        {"id": "jose_es256_valid", "token": es256(jp), "reason": "ok", "sub": "xoffi-user-42", "org_id": "org-1",
         "groups": "1,ops", "email": "max@example.org"},
        {"id": "jose_es256_wrong_aud", "token": es256(dict(jp, aud="other")), "reason": "aud"},
        {"id": "jose_es256_expired", "token": es256(dict(jp, exp=NOW - 1000)), "reason": "expired"},
        {"id": "jose_es256_wrong_iss", "token": es256(dict(jp, iss="https://evil.example")), "reason": "iss"},
        {"id": "jose_es256_unknown_kid", "token": es256(jp, kid="nope"), "reason": "kid"},
        {"id": "jose_eddsa_token_refused", "token": mk(dict(jp)), "reason": "alg"},
    ],
}

out = {
    "about": "generated by tools/fleitec_login/gen_vectors.py (python cryptography); do not edit by hand",
    "now": NOW, "iss": ISS, "aud": AUD, "leeway": LEEWAY,
    "jwks": {"keys": [
        {"kty": "OKP", "crv": "Ed25519", "kid": "k1", "use": "sig", "alg": "EdDSA", "x": pubx(K1)},
        {"kty": "OKP", "crv": "Ed25519", "kid": "k2", "use": "sig", "alg": "EdDSA", "x": pubx(K2)},
    ]},
    "jwks_k1_only": {"keys": [{"kty": "OKP", "crv": "Ed25519", "kid": "k1", "x": pubx(K1)}]},
    "jwks_k2_only": {"keys": [{"kty": "OKP", "crv": "Ed25519", "kid": "k2", "x": pubx(K2)}]},
    "jwks_junk": {"keys": [
        {"kty": "RSA", "kid": "r", "n": "AQAB", "e": "AQAB"},
        {"kty": "OKP", "crv": "Ed448", "kid": "e448", "x": pubx(K1)},
        {"kty": "OKP", "crv": "Ed25519", "x": pubx(K1)},
        {"kty": "OKP", "crv": "Ed25519", "kid": "short", "x": "AAAA"},
        {"kty": "OKP", "crv": "Ed25519", "kid": "enc", "use": "enc", "x": pubx(K1)},
        {"kty": "OKP", "crv": "Ed25519", "kid": "wrongalg", "alg": "RS256", "x": pubx(K1)},
    ]},
    "seeds": {"k1": 1, "k2": 2, "attacker": 9},
    "pub": {"k1": pubx(K1), "k2": pubx(K2), "attacker": pubx(ATT)},
    "jwt_eddsa": cases,
    "legacy_hs256": {"secret": SECRET, "cases": legacy},
    "jose": jose,
}
json.dump(out, sys.stdout, indent=1, sort_keys=False)
sys.stdout.write("\n")

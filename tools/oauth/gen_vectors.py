#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""tools/oauth/gen_vectors.py -- writes tests/data/oauth-vectors.json: JSON Web
Tokens signed by the `cryptography` package (RSA PKCS#1 v1.5 with SHA-256/384/
512, ECDSA P-256/P-384, HMAC) and the JWKS that holds the public keys, plus
hostile variants. tests/2150_oauth_jose.fi runs lib/auth/jose.fi against it.

The keys are made fresh every time this script runs (the file is committed, the
script is not run by test.sh); `now` is fixed so the tokens never "expire".
"""
import base64
import hashlib
import hmac
import json
import os
import sys

from cryptography.hazmat.primitives import hashes
from cryptography.hazmat.primitives.asymmetric import ec, padding, rsa, utils

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, '..', '..', 'tests', 'data', 'oauth-vectors.json')

NOW = 1800000000


def b64(b):
    return base64.urlsafe_b64encode(b).rstrip(b'=').decode()


def j(o):
    return json.dumps(o, separators=(',', ':')).encode()


def i2b(n, length=None):
    length = length or (n.bit_length() + 7) // 8
    return n.to_bytes(length, 'big')


rsa_key = rsa.generate_private_key(public_exponent=65537, key_size=2048)
rsa_small = rsa.generate_private_key(public_exponent=65537, key_size=512)
ec256 = ec.generate_private_key(ec.SECP256R1())
ec384 = ec.generate_private_key(ec.SECP384R1())
rsa_pub = rsa_key.public_key().public_numbers()
e256 = ec256.public_key().public_numbers()
e384 = ec384.public_key().public_numbers()

SECRET = b'0123456789abcdef0123456789abcdef-shared-secret'

jwks = {'keys': [
    {'kty': 'RSA', 'kid': 'rsa1', 'use': 'sig', 'n': b64(i2b(rsa_pub.n)), 'e': b64(i2b(rsa_pub.e))},
    {'kty': 'EC', 'kid': 'ec256', 'crv': 'P-256', 'use': 'sig', 'x': b64(i2b(e256.x, 32)), 'y': b64(i2b(e256.y, 32))},
    {'kty': 'EC', 'kid': 'ec384', 'crv': 'P-384', 'x': b64(i2b(e384.x, 48)), 'y': b64(i2b(e384.y, 48))},
    # these must be skipped by jwks_parse:
    {'kty': 'RSA', 'kid': 'enc1', 'use': 'enc', 'n': b64(i2b(rsa_pub.n)), 'e': b64(i2b(rsa_pub.e))},
    {'kty': 'RSA', 'kid': 'tiny', 'use': 'sig', 'n': b64(i2b(rsa_small.public_key().public_numbers().n)), 'e': 'AQAB'},
    {'kty': 'oct', 'kid': 'oct1', 'k': b64(SECRET)},
    {'kty': 'EC', 'kid': 'badlen', 'crv': 'P-256', 'x': b64(b'\x01' * 31), 'y': b64(b'\x02' * 32)},
    {'kty': 'OKP', 'kid': 'ed', 'crv': 'Ed25519', 'x': b64(b'\x03' * 32)},
]}
EXPECTED_KEYS = 3

# a JWKS in which the RSA key names its algorithm
jwks_alg = {'keys': [dict(jwks['keys'][0], kid='rsa384', alg='RS384')]}


def sign(alg, signing_input, key=None):
    data = signing_input.encode()
    if alg == 'RS256':
        return rsa_key.sign(data, padding.PKCS1v15(), hashes.SHA256())
    if alg == 'RS384':
        return rsa_key.sign(data, padding.PKCS1v15(), hashes.SHA384())
    if alg == 'RS512':
        return rsa_key.sign(data, padding.PKCS1v15(), hashes.SHA512())
    if alg == 'PS256':
        return rsa_key.sign(data, padding.PSS(padding.MGF1(hashes.SHA256()), 32), hashes.SHA256())
    if alg == 'ES256':
        der = ec256.sign(data, ec.ECDSA(hashes.SHA256()))
        r, s = utils.decode_dss_signature(der)
        return i2b(r, 32) + i2b(s, 32)
    if alg == 'ES384':
        der = ec384.sign(data, ec.ECDSA(hashes.SHA384()))
        r, s = utils.decode_dss_signature(der)
        return i2b(r, 48) + i2b(s, 48)
    if alg == 'HS256':
        return hmac.new(key or SECRET, data, hashlib.sha256).digest()
    if alg == 'none':
        return b''
    raise SystemExit(alg)


def token(alg, claims, kid='AUTO', header_extra=None, key=None, raw_header=None):
    kids = {'RS256': 'rsa1', 'RS384': 'rsa1', 'RS512': 'rsa1', 'PS256': 'rsa1',
            'ES256': 'ec256', 'ES384': 'ec384', 'HS256': None, 'none': None}
    h = {'alg': alg, 'typ': 'JWT'}
    k = kids[alg] if kid == 'AUTO' else kid
    if k:
        h['kid'] = k
    if header_extra:
        h.update(header_extra)
    hb = raw_header if raw_header is not None else j(h)
    si = b64(hb) + '.' + b64(j(claims) if not isinstance(claims, bytes) else claims)
    return si + '.' + b64(sign(alg, si, key))


base = {'iss': 'https://idp.example/', 'aud': 'client-1', 'sub': 'user-42', 'name': 'Zoë',
        'iat': NOW - 100, 'nbf': NOW - 100, 'exp': NOW + 3600, 'nonce': 'n-0S6_WzA2Mj'}
POL = {'iss': 'https://idp.example/', 'aud': 'client-1'}


def case(name, tok, expect, pol=None, **extra):
    c = {'name': name, 'token': tok, 'expect': expect, 'policy': dict(POL, **(pol or {}))}
    c.update(extra)
    return c


cases = []
for alg in ('RS256', 'RS384', 'RS512', 'ES256', 'ES384'):
    cases.append(case(alg.lower() + '_ok', token(alg, base), 'ok', claim_sub='user-42', claim_name='Zoë'))
cases.append(case('hs256_ok', token('HS256', base), 'ok', pol={'algs': ['HS256']}, with_secret=True))
cases.append(case('hs256_not_allowed_by_default', token('HS256', base), 'alg', with_secret=True))
cases.append(case('hs256_wrong_secret', token('HS256', base, key=b'x' * 40), 'sig', pol={'algs': ['HS256']}, with_secret=True))
cases.append(case('rs256_no_kid_one_rsa_key', token('RS256', base, kid=None), 'ok'))
cases.append(case('es256_no_kid_one_p256_key', token('ES256', base, kid=None), 'ok'))
cases.append(case('unknown_kid', token('RS256', base, kid='nope'), 'nokey'))
cases.append(case('kid_of_the_wrong_kind', token('RS256', base, kid='ec256'), 'nokey'))
# tampering
t = token('RS256', base)
h, p, s = t.split('.')
p2 = b64(j(dict(base, sub='admin')))
cases.append(case('payload_replaced', h + '.' + p2 + '.' + s, 'sig'))
sb = bytearray(base64.urlsafe_b64decode(s + '=' * (-len(s) % 4)))
sb[10] ^= 1
cases.append(case('signature_bit_flipped', h + '.' + p + '.' + b64(bytes(sb)), 'sig'))
cases.append(case('signature_truncated', h + '.' + p + '.' + b64(bytes(sb[:-1])), 'sig'))
cases.append(case('signature_empty', h + '.' + p + '.', 'sig'))
# the algorithm attacks
cases.append(case('alg_none', token('none', base, kid=None), 'alg'))
cases.append(case('alg_none_with_signature_part', token('none', base, kid=None, raw_header=j({'alg': 'none'})) + 'AAAA', 'alg'))
cases.append(case('alg_None_capital', token('RS256', base, raw_header=j({'alg': 'None', 'kid': 'rsa1'})), 'alg'))
cases.append(case('ps256_not_supported', token('PS256', base), 'alg'))
# RS256 key used as HMAC secret (the confusion attack)
pem_n = i2b(rsa_pub.n)
cases.append(case('rs256_to_hs256_confusion', token('HS256', base, kid='rsa1', key=pem_n), 'nokey', pol={'algs': ['HS256', 'RS256']}))
cases.append(case('rs256_to_hs256_confusion_default', token('HS256', base, kid='rsa1', key=pem_n), 'alg'))
cases.append(case('key_names_other_alg', token('RS256', base, kid='rsa384'), 'alg', jwks='alg'))
# time
cases.append(case('expired', token('RS256', dict(base, exp=NOW - 61)), 'expired'))
cases.append(case('expired_within_leeway', token('RS256', dict(base, exp=NOW - 30)), 'ok'))
cases.append(case('exp_equals_now_minus_leeway', token('RS256', dict(base, exp=NOW - 60)), 'expired'))
cases.append(case('expired_no_leeway', token('RS256', dict(base, exp=NOW - 1)), 'expired', pol={'leeway': 0}))
cases.append(case('exp_missing', token('RS256', {k: v for k, v in base.items() if k != 'exp'}), 'expired'))
cases.append(case('nbf_in_the_future', token('RS256', dict(base, nbf=NOW + 600)), 'nbf'))
cases.append(case('iat_in_the_future', token('RS256', dict(base, iat=NOW + 600)), 'nbf'))
cases.append(case('nbf_within_leeway', token('RS256', dict(base, nbf=NOW + 30)), 'ok'))
# claims
cases.append(case('wrong_issuer', token('RS256', dict(base, iss='https://evil.example/')), 'iss'))
cases.append(case('issuer_missing', token('RS256', {k: v for k, v in base.items() if k != 'iss'}), 'iss'))
cases.append(case('issuer_prefix_only', token('RS256', dict(base, iss='https://idp.example')), 'iss'))
cases.append(case('wrong_audience', token('RS256', dict(base, aud='client-2')), 'aud'))
cases.append(case('audience_array_hit', token('RS256', dict(base, aud=['x', 'client-1', 'y'])), 'ok'))
cases.append(case('audience_array_miss', token('RS256', dict(base, aud=['x', 'y'])), 'aud'))
cases.append(case('audience_missing', token('RS256', {k: v for k, v in base.items() if k != 'aud'}), 'aud'))
cases.append(case('nonce_ok', token('RS256', base), 'ok', pol={'nonce': 'n-0S6_WzA2Mj'}))
cases.append(case('nonce_wrong', token('RS256', base), 'nonce', pol={'nonce': 'other'}))
cases.append(case('nonce_missing_in_token', token('RS256', {k: v for k, v in base.items() if k != 'nonce'}), 'nonce', pol={'nonce': 'n-0S6_WzA2Mj'}))
cases.append(case('no_demands_at_all', token('RS256', dict(base, iss='whatever', aud='whoever')), 'ok', pol={'iss': '', 'aud': ''}))
cases.append(case('claim_text_cannot_smuggle_exp', token('RS256', {'iss': POL['iss'], 'aud': 'client-1', 'name': '"exp":99999999999', 'exp': NOW - 1000}), 'expired'))
# shape
good = token('RS256', base)
cases.append(case('two_parts', '.'.join(good.split('.')[:2]), 'malformed'))
cases.append(case('four_parts', good + '.AAAA', 'malformed'))
cases.append(case('empty', '', 'malformed'))
cases.append(case('header_not_base64', '!!!.' + p + '.' + s, 'malformed'))
cases.append(case('padding_in_part', h + '=.' + p + '.' + s, 'malformed'))
cases.append(case('standard_alphabet_plus', h + '.' + p.replace('-', '+').replace('_', '/') + 'xx+/.' + s, 'malformed'))
cases.append(case('header_not_json', token('RS256', base, raw_header=b'not json'), 'malformed'))
cases.append(case('header_not_object', token('RS256', base, raw_header=b'[1,2]'), 'malformed'))
cases.append(case('header_without_alg', token('RS256', base, raw_header=j({'kid': 'rsa1'})), 'malformed'))
cases.append(case('payload_not_object', token('RS256', b'[1,2,3]'), 'malformed'))
cases.append(case('payload_not_json', token('RS256', b'{"a":'), 'malformed'))

vec = {'now': NOW, 'secret': SECRET.decode(), 'expected_keys': EXPECTED_KEYS,
       'jwks': jwks, 'jwks_alg': jwks_alg, 'cases': cases,
       'jwks_bad': ['not json', '{}', '{"keys":5}', '[]']}
with open(OUT, 'w') as f:
    json.dump(vec, f, indent=1, ensure_ascii=False)
print('wrote', os.path.normpath(OUT), len(cases), 'cases')

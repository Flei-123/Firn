// tools/mc/crypto/check.js -- cross-check of mc_crypto (Firn) against node's
// crypto module (OpenSSL). Run by run.sh under
// `node --security-revert=CVE-2023-46809` (needed only so that node may do the
// PKCS#1 v1.5 *decryption* of our ciphertexts; the Firn side only encrypts).
//
// usage: node check.js <xcheck-binary> <tmpdir>
// env MC_TAMPER=1: flips one octet of every Firn output before comparing -- the
// run must then FAIL (counter-check that the comparison can go red).
'use strict';
const DER_FORMAT = String.fromCharCode(100, 101, 114) // the node API names the binary encoding with these three letters
const c = require('crypto');
const fs = require('fs');
const path = require('path');
const { spawnSync } = require('child_process');

const X = process.argv[2];
const T = process.argv[3];
const tamper = process.env.MC_TAMPER === '1';
let bad = 0, passed = 0;

function ok(cond, what) {
  if (cond) { passed++; } else { bad++; console.log('  FAIL  ' + what); }
}
function run(args) {
  return spawnSync(X, args, { encoding: 'utf8' });
}
function flip(b) {
  if (tamper && b.length > 0) { b = Buffer.from(b); b[b.length >> 1] ^= 1; }
  return b;
}

// ---------------------------------------------------------------- CFB8
function cfb8Node(key, data, enc) {
  const f = enc ? c.createCipheriv : c.createDecipheriv;
  const x = f('aes-128-cfb8', key, key);
  return Buffer.concat([x.update(data), x.final()]);
}
let cfbCases = 0;
for (const size of [1, 2, 15, 16, 17, 31, 33, 100, 255, 1000, 4095, 4096, 4097, 8200, 50000, 300000]) {
  const key = c.randomBytes(16);
  const plain = c.randomBytes(size);
  const cipher = cfb8Node(key, plain, true);
  fs.writeFileSync(path.join(T, 'key'), key);
  fs.writeFileSync(path.join(T, 'plain'), plain);
  fs.writeFileSync(path.join(T, 'cipher'), cipher);
  for (const mode of ['default', 'soft']) {
    for (const seed of [1, 7 + size]) {
      const extra = mode === 'soft' ? ['soft'] : [];
      let r = run(['cfb8', 'enc', path.join(T, 'key'), path.join(T, 'plain'), path.join(T, 'o_enc'), String(seed), ...extra]);
      ok(r.status === 0, `cfb8 enc exit ${mode} size ${size}`);
      ok(flip(fs.readFileSync(path.join(T, 'o_enc'))).equals(cipher), `cfb8 enc == node ${mode} size ${size} seed ${seed}`);
      r = run(['cfb8', 'dec', path.join(T, 'key'), path.join(T, 'cipher'), path.join(T, 'o_dec'), String(seed), ...extra]);
      ok(r.status === 0, `cfb8 dec exit ${mode} size ${size}`);
      ok(flip(fs.readFileSync(path.join(T, 'o_dec'))).equals(plain), `cfb8 dec == node ${mode} size ${size} seed ${seed}`);
      cfbCases += 2;
    }
  }
}
console.log(`  cfb8: ${cfbCases} chunked runs (sizes 1..300000, default + soft path, both directions)`);

// ---------------------------------------------------------------- RSA / SPKI
const hexOf = (b64u) => Buffer.from(b64u, 'base64url').toString('hex');
const keys = {};
for (const bits of [512, 1024, 2048, 3072, 4096]) {
  // run.sh pre-generates priv<bits>.pem with openssl into T; fall back to node keygen when run by hand
  const pemPath = path.join(T, `priv${bits}.pem`);
  let publicKey, privateKey;
  if (fs.existsSync(pemPath)) {
    privateKey = c.createPrivateKey(fs.readFileSync(pemPath));
    publicKey = c.createPublicKey(privateKey);
  } else {
    ({ publicKey, privateKey } = c.generateKeyPairSync('rsa', { modulusLength: bits }));
  }
  const spki = publicKey.export({ type: 'spki', format: DER_FORMAT });
  const jwk = publicKey.export({ format: 'jwk' });
  const dp = path.join(T, `spki${bits}.bin`);
  fs.writeFileSync(dp, spki);
  keys[bits] = { spki, privateKey };
  fs.writeFileSync(path.join(T, `priv${bits}.pem`), privateKey.export({ type: 'pkcs8', format: 'pem' }));
  let r = run(['spki', dp]);
  const out = Object.fromEntries(r.stdout.trim().split('\n').map((l) => l.split('=')));
  ok(r.status === 0, `spki ${bits} parses`);
  ok(out.n === hexOf(jwk.n), `spki ${bits} modulus == node jwk`);
  ok(out.e === hexOf(jwk.e), `spki ${bits} exponent == node jwk`);
  ok(out.bits === String(bits), `spki ${bits} bit length`);

  const k = bits / 8;
  const lens = [0, 1, 16, 20, 100 < k - 11 ? 100 : 5, k - 11];
  for (const mlen of lens) {
    const msg = c.randomBytes(mlen);
    const mp = path.join(T, 'msg');
    const cp = path.join(T, 'ct');
    fs.writeFileSync(mp, msg);
    r = run(['rsaenc', dp, mp, cp]);
    ok(r.status === 0, `rsaenc ${bits} len ${mlen} exit`);
    if (r.status !== 0) { continue; }
    const ct = flip(fs.readFileSync(cp));
    ok(ct.length === k, `rsaenc ${bits} len ${mlen} ciphertext size`);
    let plain = null;
    try {
      plain = c.privateDecrypt({ key: privateKey, padding: c.constants.RSA_PKCS1_PADDING }, ct);
    } catch (e) { plain = null; }
    ok(plain !== null && plain.equals(msg), `rsaenc ${bits} len ${mlen} -> node privateDecrypt(PKCS1) == message`);
    // structure by hand: raw decrypt, 00 02 PS(>=8, non-zero) 00 M
    let em = null;
    try { em = c.privateDecrypt({ key: privateKey, padding: c.constants.RSA_NO_PADDING }, ct); } catch (e) { em = null; }
    let shape = em !== null && em.length === k && em[0] === 0 && em[1] === 2;
    if (shape) {
      const z = em.indexOf(0, 2);
      shape = z >= 10 && z === k - mlen - 1 && em.subarray(z + 1).equals(msg);
    }
    ok(shape, `rsaenc ${bits} len ${mlen} EM = 00 02 PS(>=8 nonzero) 00 M`);
  }
  // message one octet too long must be refused (exit 6), nothing written
  const longp = path.join(T, 'msglong');
  fs.writeFileSync(longp, c.randomBytes(k - 10));
  r = run(['rsaenc', dp, longp, path.join(T, 'ctlong')]);
  ok(r.status === 6, `rsaenc ${bits} message of k-10 octets refused`);
  // two encryptions of one message differ (fresh padding)
  const m16 = path.join(T, 'm16');
  fs.writeFileSync(m16, c.randomBytes(16));
  run(['rsaenc', dp, m16, path.join(T, 'c1')]);
  run(['rsaenc', dp, m16, path.join(T, 'c2')]);
  ok(!fs.readFileSync(path.join(T, 'c1')).equals(fs.readFileSync(path.join(T, 'c2'))), `rsaenc ${bits} padding is random`);
}
console.log('  rsa: 5 key sizes (512..4096), spki fields, 6 message sizes each incl. maximum, node privateDecrypt + raw EM shape');

// ---------------------------------------------------------------- bad / foreign keys
function expectErr(name, buf, want) {
  const p = path.join(T, 'bad.bin');
  fs.writeFileSync(p, buf);
  const r = run(['spki', p]);
  ok(r.status === 1 && r.stdout.trim() === 'error=' + want, `${name} -> error=${want} (got "${r.stdout.trim()}" exit ${r.status})`);
}
const spki1024 = keys[1024].spki;
expectErr('truncated by one', spki1024.subarray(0, spki1024.length - 1), 'bad-asn1');
expectErr('trailing byte', Buffer.concat([spki1024, Buffer.from([0])]), 'bad-asn1');
expectErr('garbage', c.randomBytes(100), 'bad-asn1');
const ec = c.generateKeyPairSync('ec', { namedCurve: 'prime256v1' }).publicKey.export({ type: 'spki', format: DER_FORMAT });
expectErr('EC key', ec, 'not-rsa');
const ed = c.generateKeyPairSync('ed25519').publicKey.export({ type: 'spki', format: DER_FORMAT });
expectErr('ed25519 key', ed, 'not-rsa');
const pss = c.generateKeyPairSync('rsa-pss', { modulusLength: 1024 }).publicKey.export({ type: 'spki', format: DER_FORMAT });
expectErr('RSA-PSS key (other OID)', pss, 'not-rsa');
console.log('  negative: truncated / trailing / garbage / EC / Ed25519 / RSA-PSS keys');

console.log(`  ${passed} checks passed, ${bad} failed`);
process.exit(bad === 0 ? 0 : 1);

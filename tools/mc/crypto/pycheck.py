# tools/mc/crypto/pycheck.py -- second independent decryptor: python `cryptography`
# decrypts RSA PKCS#1 v1.5 ciphertexts produced by mc_crypto (xcheck rsaenc).
# usage: python3 -I pycheck.py <xcheck-binary> <tmpdir>   (tmpdir holds priv*.pem / spki*.der)
import os, subprocess, sys
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric import padding

xcheck, tmp = sys.argv[1], sys.argv[2]
bad = 0
n = 0
for bits in (1024, 2048):
    with open(os.path.join(tmp, f"priv{bits}.pem"), "rb") as f:
        key = serialization.load_pem_private_key(f.read(), password=None)
    spki = os.path.join(tmp, f"spki{bits}.der")
    for mlen in (16, 4, (bits // 8) - 11):
        msg = os.urandom(mlen)
        mp, cp = os.path.join(tmp, "pm"), os.path.join(tmp, "pc")
        with open(mp, "wb") as f:
            f.write(msg)
        subprocess.run([xcheck, "rsaenc", spki, mp, cp], check=True)
        with open(cp, "rb") as f:
            ct = f.read()
        n += 1
        try:
            if key.decrypt(ct, padding.PKCS1v15()) != msg:
                bad += 1
                print("  FAIL  python decrypt mismatch", bits, mlen)
        except Exception as e:
            bad += 1
            print("  FAIL  python decrypt error", bits, mlen, e)
print(f"  python: {n} ciphertexts decrypted, {bad} failures")
sys.exit(1 if bad else 0)

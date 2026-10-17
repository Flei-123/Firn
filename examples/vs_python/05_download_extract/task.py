import os, tarfile, tempfile, urllib.request, zipfile

base = "http://127.0.0.1:" + os.environ["VSPY_PORT"]
with tempfile.TemporaryDirectory() as tmp:
    for name, kind in (("pkg.zip", "zip"), ("pkg.tar.gz", "tgz")):
        path = os.path.join(tmp, name)
        urllib.request.urlretrieve(base + "/" + name, path)
        dest = os.path.join(tmp, kind)
        if kind == "zip":
            zipfile.ZipFile(path).extractall(dest)
        else:
            tarfile.open(path).extractall(dest)
        for root, dirs, files in sorted(os.walk(dest)):
            dirs.sort()
            for f in sorted(files):
                full = os.path.join(root, f)
                print(kind, os.path.relpath(full, dest), os.path.getsize(full))

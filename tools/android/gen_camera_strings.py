#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/android/gen_camera_strings.py -- the JNI names and log texts of lib/plat/android/camera.fi as ONE
# zero separated table plus offset constants (offsets by hand go wrong, same as gen_pick_strings.py).
# Edit the list, run: python3 tools/android/gen_camera_strings.py
import os
S = [
("S_TAG", "firn-cam"),
("S_PERM_CAMERA", "android.permission.CAMERA"),
("N_CHECK_PERM", "checkSelfPermission"),
("G_CHECK_PERM", "(Ljava/lang/String;)I"),
("N_REQ_PERM", "requestPermissions"),
("G_REQ_PERM", "([Ljava/lang/String;I)V"),
("C_STRING", "java/lang/String"),
("S_LIBCAM", "libcamera2ndk.so"),
("S_PHYS_FN", "ACaptureSessionPhysicalOutput_create"),
("L_OPEN_FAILED", "camera open failed"),
("L_OPENED", "camera opened"),
("X_OK", "ok"),
("X_PERMISSION", "camera permission missing"),
("X_NO_CAMERA", "no such camera"),
("X_OPEN", "camera could not be opened"),
("X_SESSION", "capture session failed"),
("X_STREAM", "stream setup failed"),
("X_TIMEOUT", "timeout, no frame"),
("X_UNSUPPORTED", "not supported on this device"),
("X_SPACE", "buffer too small"),
("X_STATE", "camera not ready"),
("C_VALUES", "android/content/ContentValues"),
("C_MEDIA", "android/provider/MediaStore$Images$Media"),
("N_INIT", "<init>"),
("G_V", "()V"),
("N_PUT", "put"),
("G_PUT_SS", "(Ljava/lang/String;Ljava/lang/String;)V"),
("S_DISPLAY", "_display_name"),
("S_MIME", "mime_type"),
("S_RELPATH", "relative_path"),
("S_PICTURES", "Pictures/FirnCam"),
("N_EXT", "EXTERNAL_CONTENT_URI"),
("G_URI", "Landroid/net/Uri;"),
("N_RESOLVER", "getContentResolver"),
("G_RESOLVER", "()Landroid/content/ContentResolver;"),
("N_INSERT", "insert"),
("G_INSERT", "(Landroid/net/Uri;Landroid/content/ContentValues;)Landroid/net/Uri;"),
("N_OPEN_FD", "openFileDescriptor"),
("G_OPEN_FD", "(Landroid/net/Uri;Ljava/lang/String;)Landroid/os/ParcelFileDescriptor;"),
("S_W", "w"),
("N_DETACH", "detachFd"),
("G_VI", "()I"),
]
txt = ""
offs = []
off = 0
for n, v in S:
    b = v.encode("utf-8")
    offs.append((n, off, v))
    txt += v.replace("\\", "\\\\").replace('"', '\\"') + "\\0"
    off += len(b) + 1
tab = "static TXT: [u8; %d] = \"%s\"\n\n" % (off, txt)
consts = "".join("const %s: usize = %d // %s\n" % (n, o, v) for n, o, v in offs)
P = os.path.join(os.path.dirname(os.path.abspath(__file__)), "../../lib/plat/android/camera.fi")
src = open(P).read()
a = src.index("// ---- BEGIN GENERATED STRINGS")
a = src.index("\n", a) + 1
b = src.index("// ---- END GENERATED STRINGS")
src = src[:a] + tab + consts + src[b:]
open(P, "w").write(src)
print("ok", off)

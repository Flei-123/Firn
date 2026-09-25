#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/android/gen_pick_strings.py -- the JNI names of lib/plat/android/pick.fi
# as ONE zero separated table plus offset constants (the same way as
# gen_push_strings.py: offsets by hand go wrong).
# Edit the list, run: python3 tools/android/gen_pick_strings.py
import os
S = [
("C_INTENT", "android/content/Intent"),
("C_MEDIA", "android/provider/MediaStore$Images$Media"),
("C_VALUES", "android/content/ContentValues"),
("C_BF", "android/graphics/BitmapFactory"),
("C_BFO", "android/graphics/BitmapFactory$Options"),
("C_BITMAP", "android/graphics/Bitmap"),
("C_FORMAT", "android/graphics/Bitmap$CompressFormat"),
("C_BAOS", "java/io/ByteArrayOutputStream"),
("C_EXIF", "android/media/ExifInterface"),
("C_MATRIX", "android/graphics/Matrix"),
("N_INIT", "<init>"),
("G_V", "()V"),
("G_S_V", "(Ljava/lang/String;)V"),
("G_IN_V", "(Ljava/io/InputStream;)V"),
("S_GET_CONTENT", "android.intent.action.GET_CONTENT"),
("S_CAPTURE", "android.media.action.IMAGE_CAPTURE"),
("S_IMAGES", "image/*"),
("S_OPENABLE", "android.intent.category.OPENABLE"),
("N_SET_TYPE", "setType"),
("N_ADD_CAT", "addCategory"),
("G_S_I", "(Ljava/lang/String;)Landroid/content/Intent;"),
("N_ADD_FLAGS", "addFlags"),
("G_I_I", "(I)Landroid/content/Intent;"),
("N_PUT_EXTRA", "putExtra"),
("G_PUT_PARC", "(Ljava/lang/String;Landroid/os/Parcelable;)Landroid/content/Intent;"),
("G_PUT_PARCS", "(Ljava/lang/String;[Landroid/os/Parcelable;)Landroid/content/Intent;"),
("S_OUTPUT", "output"),
("S_INITIAL", "android.intent.extra.INITIAL_INTENTS"),
("N_CHOOSER", "createChooser"),
("G_CHOOSER", "(Landroid/content/Intent;Ljava/lang/CharSequence;)Landroid/content/Intent;"),
("S_TITLE", "Foto senden"),
("N_START_RESULT", "startActivityForResult"),
("G_START_RESULT", "(Landroid/content/Intent;I)V"),
("N_START", "startActivity"),
("G_START", "(Landroid/content/Intent;)V"),
("N_SET_CLASS", "setClassName"),
("G_SET_CLASS", "(Ljava/lang/String;Ljava/lang/String;)Landroid/content/Intent;"),
("N_PKG", "getPackageName"),
("G_PKG", "()Ljava/lang/String;"),
("S_PICK_CLASS", "org.firn.FirnPick"),
("N_FINISH", "finish"),
("N_RESOLVER", "getContentResolver"),
("G_RESOLVER", "()Landroid/content/ContentResolver;"),
("N_PUT", "put"),
("G_PUT_SS", "(Ljava/lang/String;Ljava/lang/String;)V"),
("S_MIME", "mime_type"),
("S_JPEG_MIME", "image/jpeg"),
("N_EXT", "EXTERNAL_CONTENT_URI"),
("G_URI", "Landroid/net/Uri;"),
("N_INSERT", "insert"),
("G_INSERT", "(Landroid/net/Uri;Landroid/content/ContentValues;)Landroid/net/Uri;"),
("N_DELETE", "delete"),
("G_DELETE", "(Landroid/net/Uri;Ljava/lang/String;[Ljava/lang/String;)I"),
("N_GET_DATA", "getData"),
("G_GET_DATA", "()Landroid/net/Uri;"),
("N_OPEN_IN", "openInputStream"),
("G_OPEN_IN", "(Landroid/net/Uri;)Ljava/io/InputStream;"),
("N_CLOSE", "close"),
("N_DECODE_STREAM", "decodeStream"),
("G_DECODE_STREAM", "(Ljava/io/InputStream;Landroid/graphics/Rect;Landroid/graphics/BitmapFactory$Options;)Landroid/graphics/Bitmap;"),
("N_DECODE_BYTES", "decodeByteArray"),
("G_DECODE_BYTES", "([BIILandroid/graphics/BitmapFactory$Options;)Landroid/graphics/Bitmap;"),
("F_JUST", "inJustDecodeBounds"),
("G_Z", "Z"),
("F_SAMPLE", "inSampleSize"),
("G_I", "I"),
("F_OUTW", "outWidth"),
("F_OUTH", "outHeight"),
("N_ATTR", "getAttributeInt"),
("G_ATTR", "(Ljava/lang/String;I)I"),
("S_ORIENT", "Orientation"),
("N_ROTATE", "postRotate"),
("G_F_Z", "(F)Z"),
("N_CREATE", "createBitmap"),
("G_CREATE", "(Landroid/graphics/Bitmap;IIIILandroid/graphics/Matrix;Z)Landroid/graphics/Bitmap;"),
("G_CREATE5", "(Landroid/graphics/Bitmap;IIII)Landroid/graphics/Bitmap;"),
("N_SCALED", "createScaledBitmap"),
("G_SCALED", "(Landroid/graphics/Bitmap;IIZ)Landroid/graphics/Bitmap;"),
("N_WIDTH", "getWidth"),
("N_HEIGHT", "getHeight"),
("G_VI", "()I"),
("N_COMPRESS", "compress"),
("G_COMPRESS", "(Landroid/graphics/Bitmap$CompressFormat;ILjava/io/OutputStream;)Z"),
("N_JPEG", "JPEG"),
("G_FORMAT", "Landroid/graphics/Bitmap$CompressFormat;"),
("N_TOBYTES", "toByteArray"),
("G_TOBYTES", "()[B"),
("N_RECYCLE", "recycle"),
("N_COPYPX", "copyPixelsToBuffer"),
("G_COPYPX", "(Ljava/nio/Buffer;)V"),
("S_TAG", "firn-pick"),
("L_START", "picker started"),
("L_CAM", "camera offered"),
("L_GOT", "picture taken"),
("L_NONE", "no picture chosen"),
("L_FAIL", "picture could not be read"),
("L_OPEN", "picker failed to open"),
]
txt = ""
offs = []
off = 0
for n, v in S:
    b = v.encode("utf-8")
    offs.append((n, off, v))
    txt += "".join(c if ord(c) < 128 else "\\u{%x}" % ord(c) for c in v) + "\\0"
    off += len(b) + 1
tab = "static TXT: [u8; %d] = \"%s\"\n\n" % (off, txt)
consts = "".join("const %s: usize = %d   // %s\n" % (n, o, v) for n, o, v in offs)
P = os.path.join(os.path.dirname(os.path.abspath(__file__)), "../../lib/plat/android/pick.fi")
src = open(P).read()
a = src.index("// ---- BEGIN GENERATED STRINGS")
a = src.index("\n", a) + 1
b = src.index("// ---- END GENERATED STRINGS")
src = src[:a] + tab + consts + src[b:]
open(P, "w").write(src)
print("ok", off)

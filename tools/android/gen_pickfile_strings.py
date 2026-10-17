#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/android/gen_pickfile_strings.py -- the JNI names of lib/plat/android/pickfile.fi as ONE zero separated table plus offset
# constants (same way as gen_pick_strings.py). Edit the list, run: python3 tools/android/gen_pickfile_strings.py
import os
S = [
("C_INTENT", "android/content/Intent"),
("N_INIT", "<init>"),
("G_S_V", "(Ljava/lang/String;)V"),
("S_GET_CONTENT", "android.intent.action.GET_CONTENT"),
("S_ANY", "*/*"),
("S_OPENABLE", "android.intent.category.OPENABLE"),
("N_SET_TYPE", "setType"),
("N_ADD_CAT", "addCategory"),
("G_S_I", "(Ljava/lang/String;)Landroid/content/Intent;"),
("N_CHOOSER", "createChooser"),
("G_CHOOSER", "(Landroid/content/Intent;Ljava/lang/CharSequence;)Landroid/content/Intent;"),
("S_TITLE", "File"),
("N_START_RESULT", "startActivityForResult"),
("G_START_RESULT", "(Landroid/content/Intent;I)V"),
("N_GET_DATA", "getData"),
("G_GET_DATA", "()Landroid/net/Uri;"),
("N_RESOLVER", "getContentResolver"),
("G_RESOLVER", "()Landroid/content/ContentResolver;"),
("N_OPEN_IN", "openInputStream"),
("G_OPEN_IN", "(Landroid/net/Uri;)Ljava/io/InputStream;"),
("N_GET_TYPE", "getType"),
("G_GET_TYPE", "(Landroid/net/Uri;)Ljava/lang/String;"),
("N_QUERY", "query"),
("G_QUERY", "(Landroid/net/Uri;[Ljava/lang/String;Ljava/lang/String;[Ljava/lang/String;Ljava/lang/String;)Landroid/database/Cursor;"),
("N_MOVE_FIRST", "moveToFirst"),
("G_Z", "()Z"),
("N_COL_INDEX", "getColumnIndex"),
("G_COL_INDEX", "(Ljava/lang/String;)I"),
("N_GET_STRING", "getString"),
("G_GET_STRING", "(I)Ljava/lang/String;"),
("N_LAST_SEG", "getLastPathSegment"),
("G_STR", "()Ljava/lang/String;"),
("N_CLOSE", "close"),
("G_V", "()V"),
("N_READ", "read"),
("G_READ", "([B)I"),
("S_COL_NAME", "_display_name"),
("S_TAG", "firn-pickfile"),
("L_START", "file picker started"),
("L_GOT", "file read"),
("L_NONE", "no file chosen"),
("L_FAIL", "file could not be read"),
("L_BIG", "file too large"),
("L_OPEN", "file picker failed to open"),
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
P = os.path.join(os.path.dirname(os.path.abspath(__file__)), "../../lib/plat/android/pickfile.fi")
src = open(P).read()
a = src.index("// ---- BEGIN GENERATED STRINGS")
a = src.index("\n", a) + 1
b = src.index("// ---- END GENERATED STRINGS")
src = src[:a] + tab + consts + src[b:]
open(P, "w").write(src)
print("ok", off)

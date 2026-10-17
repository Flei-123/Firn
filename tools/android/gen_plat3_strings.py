#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/android/gen_plat3_strings.py -- the JNI names of lib/plat/android/{speech,tts,share}.fi as ONE zero
# separated table per file plus offset constants (same scheme as gen_pick_strings.py: offsets by hand go wrong).
# Edit the lists, run: python3 tools/android/gen_plat3_strings.py
import os

SPEECH = [
("S_TAG", "firn-speech"),
("S_PERM", "android.permission.RECORD_AUDIO"),
("C_STRING", "java/lang/String"),
("N_CHECK_PERM", "checkSelfPermission"),
("G_CHECK_PERM", "(Ljava/lang/String;)I"),
("N_REQ_PERM", "requestPermissions"),
("G_REQ_PERM", "([Ljava/lang/String;I)V"),
("L_OPEN", "microphone stream opened"),
("L_OPEN_FAIL", "microphone stream could not be opened"),
("L_START_FAIL", "microphone stream could not be started"),
("L_STOP", "microphone stopped"),
("L_FULL", "recording buffer full"),
("L_READ_FAIL", "microphone read failed"),
]

TTS = [
("S_TAG", "firn-tts"),
("C_TTS", "android/speech/tts/TextToSpeech"),
("C_LOCALE", "java/util/Locale"),
("C_STRING", "java/lang/String"),
("N_INIT", "<init>"),
("G_TTS_INIT", "(Landroid/content/Context;Landroid/speech/tts/TextToSpeech$OnInitListener;)V"),
("N_FOR_TAG", "forLanguageTag"),
("G_FOR_TAG", "(Ljava/lang/String;)Ljava/util/Locale;"),
("N_SPEAK", "speak"),
("G_SPEAK", "(Ljava/lang/CharSequence;ILandroid/os/Bundle;Ljava/lang/String;)I"),
("N_STOP", "stop"),
("G_I", "()I"),
("N_SHUTDOWN", "shutdown"),
("G_V", "()V"),
("N_IS_SPEAKING", "isSpeaking"),
("G_Z", "()Z"),
("N_SET_LANG", "setLanguage"),
("G_SET_LANG", "(Ljava/util/Locale;)I"),
("N_SET_RATE", "setSpeechRate"),
("G_SET_RATE", "(F)I"),
("N_GET_ENGINES", "getEngines"),
("G_GET_ENGINES", "()Ljava/util/List;"),
("C_LIST", "java/util/List"),
("N_SIZE", "size"),
("L_START", "engine requested"),
("L_READY", "engine ready, speaking"),
("L_NOENGINE", "no engine answered in time"),
("L_DONE", "finished speaking"),
("S_UTT", "firn-utt"),
]

SHARE = [
("S_TAG", "firn-share"),
("C_INTENT", "android/content/Intent"),
("C_VALUES", "android/content/ContentValues"),
("C_DOWNLOADS", "android/provider/MediaStore$Downloads"),
("C_URI", "android/net/Uri"),
("C_CLIPBOARD", "android/content/ClipboardManager"),
("C_CLIP", "android/content/ClipData"),
("N_INIT", "<init>"),
("G_V", "()V"),
("G_INTENT_S", "(Ljava/lang/String;)V"),
("G_INTENT_SU", "(Ljava/lang/String;Landroid/net/Uri;)V"),
("N_PUT", "put"),
("G_PUT_SS", "(Ljava/lang/String;Ljava/lang/String;)V"),
("S_DISPLAY", "_display_name"),
("S_MIME", "mime_type"),
("S_RELPATH", "relative_path"),
("S_DOWNLOAD", "Download"),
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
("N_START", "startActivity"),
("G_START", "(Landroid/content/Intent;)V"),
("N_SET_FLAGS", "setFlags"),
("G_SET_FLAGS", "(I)Landroid/content/Intent;"),
("N_SET_TYPE", "setType"),
("G_SET_TYPE", "(Ljava/lang/String;)Landroid/content/Intent;"),
("N_SET_DT", "setDataAndType"),
("G_SET_DT", "(Landroid/net/Uri;Ljava/lang/String;)Landroid/content/Intent;"),
("N_PUT_EXTRA", "putExtra"),
("G_PUT_PARC", "(Ljava/lang/String;Landroid/os/Parcelable;)Landroid/content/Intent;"),
("G_PUT_STR", "(Ljava/lang/String;Ljava/lang/CharSequence;)Landroid/content/Intent;"),
("S_EXTRA_STREAM", "android.intent.extra.STREAM"),
("S_EXTRA_TEXT", "android.intent.extra.TEXT"),
("S_ACTION_VIEW", "android.intent.action.VIEW"),
("S_ACTION_SEND", "android.intent.action.SEND"),
("S_TEXT_PLAIN", "text/plain"),
("N_CHOOSER", "createChooser"),
("G_CHOOSER", "(Landroid/content/Intent;Ljava/lang/CharSequence;)Landroid/content/Intent;"),
("N_PARSE", "parse"),
("G_PARSE", "(Ljava/lang/String;)Landroid/net/Uri;"),
("N_GET_SERVICE", "getSystemService"),
("G_GET_SERVICE", "(Ljava/lang/String;)Ljava/lang/Object;"),
("S_CLIPBOARD", "clipboard"),
("N_NEW_PLAIN", "newPlainText"),
("G_NEW_PLAIN", "(Ljava/lang/CharSequence;Ljava/lang/CharSequence;)Landroid/content/ClipData;"),
("N_SET_PRIMARY", "setPrimaryClip"),
("G_SET_PRIMARY", "(Landroid/content/ClipData;)V"),
("N_GET_PRIMARY", "getPrimaryClip"),
("G_GET_PRIMARY", "()Landroid/content/ClipData;"),
("N_ITEM_COUNT", "getItemCount"),
("N_ITEM_AT", "getItemAt"),
("G_ITEM_AT", "(I)Landroid/content/ClipData$Item;"),
("C_ITEM", "android/content/ClipData$Item"),
("N_COERCE", "coerceToText"),
("G_COERCE", "(Landroid/content/Context;)Ljava/lang/CharSequence;"),
("C_CHARSEQ", "java/lang/CharSequence"),
("N_TOSTRING", "toString"),
("G_TOSTRING", "()Ljava/lang/String;"),
("S_LABEL", "firn"),
("N_HAS_CLIP", "hasPrimaryClip"),
("G_Z", "()Z"),
]

def render(name, items):
    txt, offs, off = "", [], 0
    for n, v in items:
        b = v.encode("utf-8")
        offs.append((n, off, v))
        txt += "".join(c if ord(c) < 128 else "\\u{%x}" % ord(c) for c in v) + "\\0"
        off += len(b) + 1
    tab = "static TXT: [u8; %d] = \"%s\"\n\n" % (off, txt)
    consts = "".join("const %s: usize = %d // %s\n" % (n, o, v) for n, o, v in offs)
    P = os.path.join(os.path.dirname(os.path.abspath(__file__)), "../../lib/plat/android/%s.fi" % name)
    src = open(P).read()
    a = src.index("// ---- BEGIN GENERATED STRINGS")
    a = src.index("\n", a) + 1
    b = src.index("// ---- END GENERATED STRINGS")
    open(P, "w").write(src[:a] + tab + consts + src[b:])
    print(name, "ok", off)

render("speech", SPEECH)
render("tts", TTS)
render("share", SHARE)

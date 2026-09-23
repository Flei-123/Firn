#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/android/gen_push_strings.py -- the JNI names of lib/plat/android/push.fi
# as ONE zero separated table plus offset constants. Offsets by hand are how
# "requestPermissions" became "tPermissions" once (measured, 23.09.2026).
# Edit the list, run: python3 tools/android/gen_push_strings.py
import os
S = [
("C_INTENT","android/content/Intent"),
("C_CHANNEL","android/app/NotificationChannel"),
("C_BUILDER","android/app/Notification$Builder"),
("C_PENDING","android/app/PendingIntent"),
("C_STRING","java/lang/String"),
("C_DRAWABLE","android/R$drawable"),
("N_INIT","<init>"),
("N_GETSVC","getSystemService"),
("G_GETSVC","(Ljava/lang/String;)Ljava/lang/Object;"),
("S_NOTIFICATION","notification"),
("G_CHANNEL_INIT","(Ljava/lang/String;Ljava/lang/CharSequence;I)V"),
("N_CREATE_CHANNEL","createNotificationChannel"),
("G_CREATE_CHANNEL","(Landroid/app/NotificationChannel;)V"),
("G_BUILDER_INIT","(Landroid/content/Context;Ljava/lang/String;)V"),
("N_SMALL_ICON","setSmallIcon"),
("G_I_B","(I)Landroid/app/Notification$Builder;"),
("N_TITLE","setContentTitle"),
("N_TEXT","setContentText"),
("G_CS_B","(Ljava/lang/CharSequence;)Landroid/app/Notification$Builder;"),
("N_AUTOCANCEL","setAutoCancel"),
("N_ONGOING","setOngoing"),
("G_Z_B","(Z)Landroid/app/Notification$Builder;"),
("N_CONTENT_INTENT","setContentIntent"),
("G_PI_B","(Landroid/app/PendingIntent;)Landroid/app/Notification$Builder;"),
("N_BUILD","build"),
("G_BUILD","()Landroid/app/Notification;"),
("N_NOTIFY","notify"),
("G_NOTIFY","(ILandroid/app/Notification;)V"),
("N_GET_PM","getPackageManager"),
("G_GET_PM","()Landroid/content/pm/PackageManager;"),
("N_GET_PKG","getPackageName"),
("G_STR","()Ljava/lang/String;"),
("N_LAUNCH","getLaunchIntentForPackage"),
("G_LAUNCH","(Ljava/lang/String;)Landroid/content/Intent;"),
("N_GET_ACTIVITY","getActivity"),
("G_GET_ACTIVITY","(Landroid/content/Context;ILandroid/content/Intent;I)Landroid/app/PendingIntent;"),
("N_START_FG","startForeground"),
("G_START_FG","(ILandroid/app/Notification;I)V"),
("N_FILES_DIR","getFilesDir"),
("G_FILES_DIR","()Ljava/io/File;"),
("N_ABS_PATH","getAbsolutePath"),
("G_V","()V"),
("N_SET_CLASS","setClassName"),
("G_SET_CLASS","(Ljava/lang/String;Ljava/lang/String;)Landroid/content/Intent;"),
("N_START_FGS","startForegroundService"),
("G_START_FGS","(Landroid/content/Intent;)Landroid/content/ComponentName;"),
("N_STOP_SVC","stopService"),
("G_STOP_SVC","(Landroid/content/Intent;)Z"),
("N_CHECK_PERM","checkSelfPermission"),
("G_CHECK_PERM","(Ljava/lang/String;)I"),
("N_REQ_PERM","requestPermissions"),
("G_REQ_PERM","([Ljava/lang/String;I)V"),
("S_POST_NOTIF","android.permission.POST_NOTIFICATIONS"),
("S_SERVICE_CLASS","org.firn.FirnService"),
("S_ICON","stat_notify_chat"),
("G_INT","I"),
("S_CH_PUSH","firn_push"),
("S_CH_PUSH_NAME","Nachrichten"),
("S_CH_SVC","firn_service"),
("S_CH_SVC_NAME","Verbindung"),
("S_SVC_TITLE","Firn Push"),
("S_SVC_TEXT","Verbindung zum Relay aktiv"),
("S_MSG_TITLE","Neue Nachricht"),
("S_CFG","/firn-push.cfg"),
("S_TAG","firn-push"),
("S_L_CONNECTED","connected"),
("S_L_LOST","connection lost, retrying"),
("S_L_START","service started"),
("S_L_STOP","service destroyed"),
("S_L_NOTE","notification posted"),
("S_L_NOCFG","no firn-push.cfg"),
]
txt=""; offs=[]; off=0
for n,v in S:
    offs.append((n,off,v)); txt+=v+"\\0"; off+=len(v.encode())+1
L=sum(len(v)+1 for _,v in S)
tab="static TXT: [u8; %d] = \"%s\"\n\n" % (L, txt)
consts="".join("const %s: usize = %d   // %s\n" % (n,o,v) for n,o,v in offs)
P=os.path.join(os.path.dirname(os.path.abspath(__file__)),"../../lib/plat/android/push.fi")
src=open(P).read()
a=src.index("// ---- BEGIN GENERATED STRINGS"); a=src.index("\n",a)+1
b=src.index("// ---- END GENERATED STRINGS")
src=src[:a]+tab+consts+src[b:]
open(P,"w").write(src)
print("ok", L)

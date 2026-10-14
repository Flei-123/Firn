# SPDX-License-Identifier: MPL-2.0
# tools/android/gen_camera_tags.py -- prints the T_* metadata tag table of lib/plat/android/camera.fi
# from the real NdkCameraMetadataTags.h (AOSP frameworks/av/camera/ndk/include/camera/), so the numbers
# are checked against the header and not remembered.
#   python3 tools/android/gen_camera_tags.py /path/to/NdkCameraMetadataTags.h | diff - <(grep '^const T_' lib/plat/android/camera.fi)
import re, subprocess, sys, tempfile, os
NAMES = {
 "CONTROL_AE_ANTIBANDING": "CONTROL_AE_ANTIBANDING_MODE", "CONTROL_AE_COMP": "CONTROL_AE_EXPOSURE_COMPENSATION",
 "CONTROL_AE_LOCK": "CONTROL_AE_LOCK", "CONTROL_AE_MODE": "CONTROL_AE_MODE", "CONTROL_AF_MODE": "CONTROL_AF_MODE",
 "JPEG_ORIENTATION": "JPEG_ORIENTATION", "JPEG_QUALITY": "JPEG_QUALITY", "NR_AVAILABLE": "NOISE_REDUCTION_AVAILABLE_NOISE_REDUCTION_MODES",
 "SENSOR_BLACK_PATTERN": "SENSOR_BLACK_LEVEL_PATTERN", "SENSOR_ORIENTATION": "SENSOR_ORIENTATION",
 "PHYSICAL_IDS": "LOGICAL_MULTI_CAMERA_PHYSICAL_IDS", "SENSOR_NOISE_PROFILE": "SENSOR_NOISE_PROFILE",
}
hdr = sys.argv[1]
src = '#include <stdio.h>\n#include <stdint.h>\n#include "%s"\nint main(){\n' % os.path.basename(hdr)
for k, v in NAMES.items():
    src += 'printf("const T_%s: u32 = %%u\\n", (unsigned)ACAMERA_%s);\n' % (k, v)
src += 'return 0;}\n'
d = tempfile.mkdtemp()
open(d + "/c.c", "w").write(src)
subprocess.check_call(["cc", "-w", "-I", os.path.dirname(os.path.abspath(hdr)), d + "/c.c", "-o", d + "/c"])
sys.stdout.write(subprocess.check_output([d + "/c"]).decode())

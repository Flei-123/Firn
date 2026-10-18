#!/usr/bin/env bash
# Builds the release APK (R8, arm64-v8a, signed). Heavy: run via /root/jarvis/bin/heavy.
# Usage: /root/jarvis/bin/heavy bench/android_kotlin/build.sh [-PbenchAbis=x86_64]
set -euo pipefail
cd "$(dirname "$0")"
export ANDROID_HOME=${ANDROID_HOME:-/root/android-sdk}
export GRADLE_USER_HOME=${GRADLE_USER_HOME:-/root/jarvis/data/toolchains/gh}
exec ./gradlew --no-daemon -Dorg.gradle.jvmargs="-Xmx2g -XX:MaxMetaspaceSize=512m" :app:assembleRelease "$@"

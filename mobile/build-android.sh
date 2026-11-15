#!/usr/bin/env sh
set -eu
cd "$(dirname "$0")/android"
: "${JAVA_HOME:?Set JAVA_HOME to JDK 17 or 21}"
: "${ANDROID_HOME:?Set ANDROID_HOME to the Android SDK}"
rustup target add aarch64-linux-android x86_64-linux-android
if ! command -v cargo-ndk >/dev/null 2>&1; then
    cargo install cargo-ndk --version 4.1.2 --locked
fi
sh ./gradlew "${1:-assembleDebug}" testDebugUnitTest lintDebug

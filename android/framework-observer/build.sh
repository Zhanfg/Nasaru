#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
set -eu

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)"
SDK="${ANDROID_SDK_ROOT:-${ANDROID_HOME:-}}"

if [ -z "$SDK" ] || [ ! -d "$SDK/platforms" ] || [ ! -d "$SDK/build-tools" ]; then
  echo "Android SDK not found via ANDROID_SDK_ROOT/ANDROID_HOME" >&2
  exit 1
fi

ANDROID_JAR="$(find "$SDK/platforms" -maxdepth 2 -name android.jar -print | sort -V | tail -n 1)"
BUILD_TOOLS="$(find "$SDK/build-tools" -mindepth 1 -maxdepth 1 -type d -print | sort -V | tail -n 1)"
D8="$BUILD_TOOLS/d8"

if [ -z "$ANDROID_JAR" ] || [ ! -f "$ANDROID_JAR" ]; then echo "No android.jar found" >&2; exit 1; fi
if [ ! -x "$D8" ]; then echo "d8 not found: $D8" >&2; exit 1; fi

OUT="$ROOT/build/framework-observer"
rm -rf "$OUT"
mkdir -p "$OUT/stubs" "$OUT/classes" "$OUT/dex"

javac -source 17 -target 17 -cp "$ANDROID_JAR" -d "$OUT/stubs" \
  "$ROOT/android/framework-observer/compile-stubs/android/app/IProcessObserver.java"

javac -source 17 -target 17 -cp "$ANDROID_JAR:$OUT/stubs" -d "$OUT/classes" \
  "$ROOT/android/framework-observer/src/dev/nasaru/collector/FrameworkObserverMain.java"

jar --create --file "$OUT/helper-classes.jar" -C "$OUT/classes" .
"$D8" --min-api 30 --lib "$ANDROID_JAR" --output "$OUT/dex" "$OUT/helper-classes.jar"
(
  cd "$OUT/dex"
  zip -q "$OUT/nasaru-framework-observer.jar" classes.dex
)
test -s "$OUT/nasaru-framework-observer.jar"
echo "$OUT/nasaru-framework-observer.jar"

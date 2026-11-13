#!/usr/bin/env bash
# ==============================================================================
# VolStep Terminal - Android APK Build Helper Script
# ==============================================================================

set -e

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
cd "$DIR"

echo "============================================================"
echo "⚡ VolStep Terminal - Android APK Builder"
echo "============================================================"

# Cek Java JDK
if ! command -v javac &> /dev/null; then
    echo "⚠️ Java JDK belum terpasang di sistem PATH."
    echo "   Untuk build APK di Linux, pasang OpenJDK 17:"
    echo "   sudo apt update && sudo apt install -y openjdk-17-jdk"
    echo ""
    echo "💡 Atau buka folder ini langsung di Android Studio:"
    echo "   File -> Open -> Pilih folder: $DIR"
    echo "   Lalu klik Build -> Build Bundle(s) / APK(s) -> Build APK(s)"
    exit 1
fi

# Cek Android SDK
if [ -z "$ANDROID_HOME" ] && [ -z "$ANDROID_SDK_ROOT" ]; then
    if [ -d "$HOME/Android/Sdk" ]; then
        export ANDROID_HOME="$HOME/Android/Sdk"
    elif [ -d "/usr/lib/android-sdk" ]; then
        export ANDROID_HOME="/usr/lib/android-sdk"
    else
        echo "⚠️ ANDROID_HOME belum disetel."
        echo "   Buka project ini di Android Studio untuk build otomatis dengan SDK terpasang."
        exit 1
    fi
fi

echo "✓ Java JDK: $(javac -version 2>&1)"
echo "✓ Android SDK: $ANDROID_HOME"

# Download gradle wrapper jar jika belum ada
if [ ! -f "gradle/wrapper/gradle-wrapper.jar" ]; then
    echo "📥 Mengunduh Gradle Wrapper JAR..."
    curl -sLo gradle/wrapper/gradle-wrapper.jar https://raw.githubusercontent.com/gradle/gradle/master/gradle/wrapper/gradle-wrapper.jar || true
fi

chmod +x gradlew 2>/dev/null || true

echo "🔨 Memulai proses kompilasi APK Android..."
if [ -f "./gradlew" ]; then
    ./gradlew assembleDebug
else
    gradle assembleDebug
fi

APK_PATH="app/build/outputs/apk/debug/app-debug.apk"
if [ -f "$APK_PATH" ]; then
    echo "============================================================"
    echo "🎉 BERHASIL! File APK Android telah dibuat:"
    echo "   Lokasi: $DIR/$APK_PATH"
    echo "============================================================"
    echo "Untuk install langsung ke HP Android via USB Debugging:"
    echo "   adb install -r $DIR/$APK_PATH"
fi

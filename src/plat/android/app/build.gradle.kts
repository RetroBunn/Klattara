// Klattara for Android: seven builds of one app, for different devices.
//
//   anyDevice     Android 4.0 and newer, every processor: install this if unsure
//   legacy        Android 4.0 to 4.4, 32-bit ARM and x86 (older phones, notetakers)
//   modernArm64   Android 5.0 and newer, 64-bit ARM (most phones and notetakers today)
//   modernArm32   Android 5.0 and newer, 32-bit ARM (older or lower-cost devices)
//   modernX86_64  Android 5.0 and newer, 64-bit Intel (emulators, Chromebooks)
//   modernX86     Android 5.0 and newer, 32-bit Intel (older Intel tablets, emulators)
//   watch         Wear OS watches: Pixel Watch, Galaxy Watch and others
//
// All share one application ID and version code, so any one replaces another. The native
// engine (app/src/main/jniLibs) is built first by ../build-native.sh.

plugins {
    id("com.android.application")
}

android {
    namespace = "com.dgomeze42.klattara"
    compileSdk = 35

    defaultConfig {
        applicationId = "com.dgomeze42.klattara"
        minSdk = 14
        targetSdk = 35
        versionName = "1.9.6"
        // The same for every build, so any one can replace another in either
        // direction (a higher number on one would make the others "downgrades").
        versionCode = 19600
    }

    flavorDimensions += "device"
    productFlavors {
        create("anyDevice") {
            dimension = "device"
            minSdk = 14
            ndk { abiFilters += listOf("armeabi-v7a", "arm64-v8a", "x86", "x86_64") }
        }
        create("legacy") {
            dimension = "device"
            minSdk = 14
            ndk { abiFilters += listOf("armeabi-v7a", "x86") }
        }
        create("modernArm64") {
            dimension = "device"
            minSdk = 21
            ndk { abiFilters += listOf("arm64-v8a") }
        }
        create("modernArm32") {
            dimension = "device"
            minSdk = 21
            ndk { abiFilters += listOf("armeabi-v7a") }
        }
        create("modernX86_64") {
            dimension = "device"
            minSdk = 21
            ndk { abiFilters += listOf("x86_64") }
        }
        create("modernX86") {
            dimension = "device"
            minSdk = 21
            ndk { abiFilters += listOf("x86") }
        }
        create("watch") {
            dimension = "device"
            minSdk = 25
            ndk { abiFilters += listOf("armeabi-v7a", "arm64-v8a") }
        }
    }

    buildTypes {
        getByName("release") {
            // Shrunk, so the app is small and starts quickly on older devices.
            isMinifyEnabled = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            // Signed with the debug key so the APKs install straight away; sign
            // with your own key for wider distribution.
            signingConfig = signingConfigs.getByName("debug")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_1_8
        targetCompatibility = JavaVersion.VERSION_1_8
    }

    packaging {
        // Keep the engine uncompressed and page-aligned in the APK where the
        // Android version can load it from there (6.0 and newer), so it starts
        // without being unpacked first.
        jniLibs { useLegacyPackaging = false }
    }

    lint {
        // Android 4.0 support is deliberate.
        disable += listOf("ExpiredTargetSdkVersion", "OldTargetApi", "MinSdkTooLow")
        abortOnError = false
        checkReleaseBuilds = false
    }
}

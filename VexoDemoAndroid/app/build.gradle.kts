plugins {
    id("com.android.application")
}

android {
    namespace = "com.vexo.demo"
    compileSdk = 34

    defaultConfig {
        applicationId = "com.vexo.demo"

        // Vulkan is mandatory on Android 7.0 (API 24) and above. wgpu's
        // Android backend uses Vulkan, so we set minSdk = 24.
        minSdk = 24
        targetSdk = 34

        versionCode = 1
        versionName = "0.1.0"

        // Minimal bring-up: arm64 only. Additional ABIs (armv7, x86_64,
        // x86) will be added once the architecture is validated on device.
        ndk {
            abiFilters += "arm64-v8a"
        }
    }

    buildTypes {
        getByName("release") {
            isMinifyEnabled = false
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro"
            )
        }
        getByName("debug") {
            isMinifyEnabled = false
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    // The `jniLibs` folder is populated by `build_for_android.sh` with
    // `libmain.so` (built from the `android_demo` Rust crate). No CMake
    // or ndk-build integration is needed here — Gradle just packages the
    // prebuilt `.so` into the APK.
    sourceSets {
        getByName("main") {
            jniLibs.srcDirs("src/main/jniLibs")
        }
    }

    packaging {
        jniLibs {
            // GameActivity's AAR also ships a `libgame-activity.so`. Don't
            // pick that up twice if it appears in multiple ABIs.
            useLegacyPackaging = false
        }
    }
}

// Since Kotlin 1.8, `kotlin-stdlib` bundles the classes that used to live
// in `kotlin-stdlib-jdk7` / `kotlin-stdlib-jdk8` (those modules are now
// empty aliases). Some transitive deps (e.g. core-ktx 1.13.1) still
// request the old 1.6.21 jdk7/jdk8 artifacts, whose classes clash with
// the ones now bundled in `kotlin-stdlib:1.8.22` and trigger
// `checkDebugDuplicateClasses` failures. Forcing all three artifacts to
// the same 1.8.22 version makes the jdk7/jdk8 modules empty wrappers and
// eliminates the duplicate classes.
configurations.all {
    resolutionStrategy {
        force("org.jetbrains.kotlin:kotlin-stdlib:1.8.22")
        force("org.jetbrains.kotlin:kotlin-stdlib-jdk7:1.8.22")
        force("org.jetbrains.kotlin:kotlin-stdlib-jdk8:1.8.22")
    }
}

dependencies {
    // GameActivity AAR — version pinned to match `android-activity` 0.6.0
    // (see android-activity CHANGELOG: "GameActivity updated to 2.0.2").
    // Note: the Maven coordinate is `androidx.games:games-activity`, NOT
    // `com.google.androidgamesdk:games-activity` (the latter is the Java
    // package name of the GameActivity class, which lives inside this AAR).
    // The class `com.google.androidgamesdk.GameActivity` declared in
    // AndroidManifest.xml loads `libmain.so` and invokes our
    // `#[no_mangle] fn android_main`.
    implementation("androidx.games:games-activity:2.0.2")

    // AndroidX core — required by GameActivity (it subclasses
    // androidx.appcompat.app.AppCompatActivity).
    implementation("androidx.appcompat:appcompat:1.7.0")
    implementation("androidx.core:core-ktx:1.13.1")
}

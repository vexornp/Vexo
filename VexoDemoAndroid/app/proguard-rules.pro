# Keep GameActivity and its native loader. Without this, R8/ProGuard may
# rename or strip the Activity class that the manifest references, breaking
# app launch.

-keep class com.google.androidgamesdk.GameActivity { *; }
-keep class com.google.androidgamesdk.** { *; }
-keep class androidx.appcompat.app.AppCompatActivity { *; }

# Keep the native `android_main` symbol indirection (libmain.so is loaded
# by GameActivity via System.loadLibrary("main")). No Java symbols to keep
# here, but we keep native method declarations intact.
-keepclasseswithmembernames class * {
    native <methods>;
}

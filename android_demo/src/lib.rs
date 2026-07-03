//! Android entry point for the Vexo demo application.
//!
//! This crate is compiled to `libmain.so` and loaded by `GameActivity`
//! (declared in `VexoDemoAndroid/app/src/main/AndroidManifest.xml`).
//! The `#[no_mangle]` `android_main` symbol is invoked by `android-activity`
//! on a dedicated thread once the Activity is created.
//!
//! On non-Android targets this crate compiles to an empty cdylib so that
//! `cargo test --workspace` / `cargo check --workspace` work from desktop
//! hosts without pulling in `ndk-sys` (which only compiles for Android).

#[cfg(target_os = "android")]
mod android {
    use android_activity::AndroidApp;
    use shared_app::State;

    /// Entry point invoked by `android-activity`'s GameActivity glue.
    ///
    /// Receives an [`AndroidApp`] handle and forwards it to
    /// [`vexo::run_android_demo`], which constructs the winit `EventLoop`
    /// (via `EventLoopBuilderExtAndroid::with_android_app`) and runs the
    /// same `VexoApp` / three-tree pipeline used on desktop and iOS.
    #[no_mangle]
    fn android_main(app: AndroidApp) {
        if let Err(e) = vexo::run_android_demo::<State>(app) {
            log::error!("vexo android demo exited with error: {e:?}");
        }
    }
}

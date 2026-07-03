/// The platform the application is running on.
///
/// Components use this to adapt their appearance and behavior.
/// Detected automatically via `Platform::current()`, or overridden
/// per-component via builder methods like `Button::platform()`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Desktop,
    Mobile,
}

impl Platform {
    /// Detect the current platform at runtime.
    pub fn current() -> Self {
        #[cfg(any(target_os = "ios", target_os = "android"))]
        {
            Platform::Mobile
        }
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        {
            Platform::Desktop
        }
    }
}

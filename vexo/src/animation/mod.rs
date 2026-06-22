pub mod tween;
pub mod ticker;
pub mod controller;

pub use tween::{Tween, ColorTween, FloatTween};
pub use ticker::{AnimationTicker, TickHandle};
pub use controller::{AnimationController, AnimationDirection};

pub mod controller;
pub mod curve;
pub mod ticker;
pub mod tween;

pub use controller::{AnimationController, AnimationDirection};
pub use curve::{Curve, CurvedAnimation, EaseInCurve, EaseInOutCurve, EaseOutCurve, LinearCurve};
pub use ticker::{AnimationTicker, TickHandle};
pub use tween::{ColorTween, FloatTween, Tween};

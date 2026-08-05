pub mod controller;
pub mod curve;
pub mod simulation;
pub mod ticker;
pub mod tween;

pub use controller::{AnimationController, AnimationDirection};
pub use curve::{
    CubicBezierCurve, Curve, CurvedAnimation, EaseInCurve, EaseInOutCurve, EaseOutCurve,
    LinearCurve,
};
pub use simulation::{
    FrictionSimulation, Simulation, SpringDescription, SpringSimulation, Tolerance,
};
pub use ticker::{AnimationTicker, TickHandle};
pub use tween::{ColorTween, FloatTween, Tween};

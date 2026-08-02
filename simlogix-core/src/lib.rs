//! SimLogix simulation engine: circuit model and discrete events.

mod circuit;
mod component;
mod components;
mod net;
mod pin;
mod signal;

pub use circuit::{Circuit, ComponentId, UnstableCircuit};
pub use component::Component;
pub use components::button::Button;
pub use components::led::Led;
pub use net::NetId;
pub use pin::{Pin, PinDirection};
pub use signal::Signal;

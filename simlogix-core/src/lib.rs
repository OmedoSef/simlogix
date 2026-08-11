//! SimLogix simulation engine: circuit model and discrete events.

mod circuit;
mod component;
mod components;
mod level;
mod net;
mod pin;
mod signal;

pub use circuit::{Circuit, ComponentId, UnstableCircuit};
pub use component::Component;
pub use components::and::And;
pub use components::buffer::Buffer;
pub use components::bus_transceiver::BusTransceiver;
pub use components::button::Button;
pub use components::clock::Clock;
pub use components::led::Led;
pub use components::nand::Nand;
pub use components::nor::Nor;
pub use components::not::Not;
pub use components::or::Or;
pub use components::port::{
    all_ones, CircuitAnchor, CircuitOutput, CircuitPort, PortDrive, PortHandles, PortSetting,
};
pub use components::probe::Probe;
pub use components::rail::Rail;
pub use components::splitter::Splitter;
pub use components::sr_latch::SrLatch;
pub use components::transistor::Transistor;
pub use components::tri_state_buffer::TriStateBuffer;
pub use components::xnor::Xnor;
pub use components::xor::Xor;
pub use level::Level;
pub use net::{Member, NetGroup, NetId};
pub use pin::{Pin, PinDirection};
pub use signal::Signal;

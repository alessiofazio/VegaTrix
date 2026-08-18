//! OpenPay domain: entities, value objects, and the payment state machine.
//!
//! This crate must not depend on HTTP, SQLx, Redis, or other infrastructure.

pub mod audit;
pub mod connector;
pub mod error;
pub mod ids;
pub mod merchant;
pub mod money;
pub mod payment;
pub mod routing;
pub mod status;
pub mod tenant;
pub mod webhook;

pub use audit::*;
pub use connector::*;
pub use error::*;
pub use ids::*;
pub use merchant::*;
pub use money::*;
pub use payment::*;
pub use routing::*;
pub use status::*;
pub use tenant::*;
pub use webhook::*;

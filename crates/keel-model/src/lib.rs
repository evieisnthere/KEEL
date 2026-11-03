//! KEEL vehicle model. Pure data + algorithms; no UI or rendering dependencies (PROJECT_PLAN §12).
pub mod catalogue;
pub mod diagnostics;
pub mod edit;
pub mod geometry;

pub use catalogue::{Catalogue, Definition};
pub use edit::{CompId, Model};

//! Single source of truth for shapes.
//!
//! Populated from M1 onwards per `claude/standards/ARCHITECTURE.md`
//! "Validation": typed structs (serde) live here; repos and services share
//! these definitions, never hand-duplicate types.

pub mod attribution;
pub mod spdx;

//! Business logic layer.
//!
//! Populated from M1 onwards per `claude/standards/ARCHITECTURE.md` 3-layer
//! rules: services own *what* the app does, depend only on `repos` and
//! `schemas`.

pub mod blame;
pub mod spdx_emit;
pub mod transcript;

//! Changing which mod applies to a champion while the patcher is already up.
//!
//! The manager's ordinary rule is that the library does not move while a
//! session is running, and `reject_if_patcher_running` enforces it on every
//! mutation. This module does not relax that rule, it goes around it: one
//! path, with preconditions of its own, that writes an overlay a waiting
//! patcher has not served yet.
//!
//! What makes that safe is measured rather than assumed. The overlay is read
//! when the game opens a file, not when the DLL injects, so an archive written
//! after the patcher started still reaches the game. And champion select ends
//! about four seconds before the game opens a champion's archive, against half
//! a second to rebuild one. Both were measured on 2026-09-16 and both are
//! written down in `docs/metrics/`.
//!
//! The decision is in [`decision`] and is pure, because its failure mode is a
//! game that crashes in a loading screen and that is worth being able to state
//! as a test.

pub mod budget;
pub mod decision;
pub mod preference;
pub mod swap;

pub use budget::Budget;
pub use decision::{Refusal, SwapContext, Verdict, decide};
pub use preference::PreferenceChange;
pub use swap::{SwapOutcome, forget_wad_layouts};

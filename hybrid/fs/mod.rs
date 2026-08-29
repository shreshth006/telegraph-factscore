//! The standalone fact scorer, vendored as a module.
//!
//! `telegraph-factscore` is a complete `no_std` scoring module in its own
//! right, with its own `#[panic_handler]` and `#[no_mangle]` ABI exports. Both
//! collide with this crate's, so its internals are vendored here rather than
//! linked as a dependency. Only the paths change; the scoring logic is byte
//! for byte the module that measures a margin of 0.9278 on the node.
pub mod aliases;
pub mod antonyms;
pub mod bytes;
pub mod facts;
pub mod profile;
pub mod score;
pub mod sets;
pub mod tokens;
pub mod units;

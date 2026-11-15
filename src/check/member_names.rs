//! The names completion may offer after `.` on a value for the compiler's
//! built-in methods; the prelude's methods come from its blocks.

/// Every name a built-in method may have; each receiver keeps only those
/// the checker's resolvers accept for it.
pub(super) const CANDIDATES: [&str; 27] = [
    "join",
    "push",
    "pop",
    "clear",
    "slice",
    "get",
    "set",
    "update",
    "is",
    "to_bytes",
    "unwrap",
    "expect",
    "wrapping_add",
    "wrapping_sub",
    "wrapping_mul",
    "checked_add",
    "checked_sub",
    "checked_mul",
    "checked_div",
    "saturating_add",
    "saturating_sub",
    "saturating_mul",
    "rotate_left",
    "rotate_right",
    "count_ones",
    "leading_zeros",
    "trailing_zeros",
];

/// More helper names, kept apart so each list stays readable.
pub(super) const MORE_CANDIDATES: [&str; 5] = ["send", "recv", "try_recv", "close", "to_bits"];

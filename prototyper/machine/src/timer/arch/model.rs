//! Deterministic host identity used only by type checking and unit tests.

pub(in crate::timer) const fn current_hart_id() -> usize {
    0
}

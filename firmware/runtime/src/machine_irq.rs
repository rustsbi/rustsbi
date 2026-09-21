//! Runtime's optional machine-interrupt policy service.
//!
//! Runtime itself transports the machine timer/software/external interrupts.
//! A machine-mode security manager may own further machine interrupts — the
//! Smmtt MSDEI, for example, must be claimed (its pending supervisor domains
//! acknowledged) before it can safely be deferred. An installed policy
//! receives every machine interrupt cause that Runtime does not transport
//! itself, ahead of the fail-stop default.

use spin::Once;

static POLICY: Once<Option<&'static dyn MachineInterruptPolicy>> = Once::new();

/// Policy for machine interrupt causes outside Runtime's own transports.
pub trait MachineInterruptPolicy: Sync {
    /// Claims and handles a machine interrupt cause. Returns `false` when
    /// the cause is not owned by this policy; the cause then stays fatal.
    fn handle_interrupt(&self, cause: usize) -> bool;
}

/// Publishes the optional machine-interrupt policy once during boot.
pub fn install(policy: Option<&'static dyn MachineInterruptPolicy>) {
    POLICY.call_once(|| policy);
}

/// Returns the published machine-interrupt policy.
pub(crate) fn get() -> Option<&'static dyn MachineInterruptPolicy> {
    POLICY.get().and_then(|policy| *policy)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestPolicy;
    impl MachineInterruptPolicy for TestPolicy {
        fn handle_interrupt(&self, _cause: usize) -> bool {
            true
        }
    }

    #[test]
    fn installed_policy_receives_causes() {
        install(Some(&TestPolicy));
        let policy = get().expect("BUG: installed policy not published");
        assert!(policy.handle_interrupt(14)); // 14 = Smmtt MSDEI
    }
}

//! SBI policy publication.
use crate::{Error, Result};

use rustsbi::RustSBI;
use spin::Once;

type Policy = &'static (dyn RustSBI + Sync);

static POLICY: Once<Policy> = Once::new();

pub(super) fn policy() -> Policy {
    *POLICY
        .get()
        .expect("SBI ecall before trap::init published its policy")
}

/// Publishes the SBI policy shared by all harts.
///
/// Publish the policy before any hart handles an SBI call.
/// Returns [`Error::AlreadyInitialized`] if a policy is already published.
pub fn init<P: RustSBI + Sync + 'static>(policy: &'static P) -> Result<()> {
    publish(&POLICY, policy)
}

fn publish(slot: &Once<Policy>, policy: Policy) -> Result<()> {
    let mut published = false;
    slot.call_once(|| {
        published = true;
        policy
    });
    if published {
        Ok(())
    } else {
        Err(Error::AlreadyInitialized)
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use rustsbi::SbiRet;

    struct StubPolicy(usize);
    impl RustSBI for StubPolicy {
        fn handle_ecall(&self, _: usize, _: usize, _: [usize; 6]) -> SbiRet {
            SbiRet::success(self.0)
        }
    }
    static FIRST: StubPolicy = StubPolicy(1);
    static SECOND: StubPolicy = StubPolicy(2);

    #[test]
    fn duplicate_publication_preserves_the_original_policy() {
        let slot = Once::new();
        assert_eq!(publish(&slot, &FIRST), Ok(()));
        assert_eq!(publish(&slot, &SECOND), Err(Error::AlreadyInitialized));
        assert_eq!(slot.get().unwrap().handle_ecall(0, 0, [0; 6]).value, 1);
    }

    #[test]
    fn concurrent_publication_has_one_winner() {
        let slot = Once::new();
        std::thread::scope(|scope| {
            let first = scope.spawn(|| publish(&slot, &FIRST));
            let second = scope.spawn(|| publish(&slot, &SECOND));
            let results = (first.join().unwrap(), second.join().unwrap());
            let winner = match results {
                (Ok(()), Err(Error::AlreadyInitialized)) => 1,
                (Err(Error::AlreadyInitialized), Ok(())) => 2,
                other => panic!("publication must have exactly one winner: {other:?}"),
            };
            assert_eq!(slot.get().unwrap().handle_ecall(0, 0, [0; 6]).value, winner);
        });
    }
}

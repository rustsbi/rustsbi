# Design

### Give each unit a single responsibility (`single-responsibility`) {#single-responsibility}

Each module, type, or function
should have one, and only one, reason to change.
If you cannot describe what a unit does
without the words "and," "or," or "but,"
it has too many responsibilities.

This applies at every scale:

- **Functions** should do one thing, do it well, and do it only.
  If you can extract another function from it
  with a name that is not merely a restatement of its implementation,
  the original function is doing more than one thing.
  Do not mix levels of abstraction:
  a syscall handler should read like a specification,
  while byte-level manipulation belongs in a helper.
- **Files** should hold one concept.
  When a file grows long or contains multiple distinct concepts,
  split it — each major data structure, subsystem entry point,
  or significant abstraction deserves its own file.

```rust
// Good — each function operates at one level of abstraction
/// Dispatches the current hart's saved machine trap.
///
/// # Safety
///
/// 1. Enter through `entry` with a complete, matching trap record on the Runtime stack.
/// 2. Keep `MIE` and `MPRV` clear and `mscratch` zero on entry.
/// 3. Every optional CSR indicated by the record must exist on this hart.
pub(super) unsafe extern "C" fn trap_dispatch(saved: &mut SavedTrap) {
    let frame = &mut saved.frame;
    let info = &saved.info;
    match CauseValue::from(info.cause).cause() {
        TrapCause::Interrupt(interrupt) => {
            // SAFETY: Runtime entry keeps MIE clear throughout interrupt dispatch.
            let irq = unsafe { crate::irq::LocalIrqDisabled::assume_disabled() };
            match interrupt {
                Interrupt::MachineSoftware => handlers::software_interrupt(frame, &irq),
                Interrupt::MachineTimer => {
                    crate::boot::timer()
                        .expect("Timer service not published")
                        .on_interrupt(&irq);
                }
                Interrupt::MachineExternal => handlers::external_interrupt(frame, &irq),
                // ...
            }
        }
        // ...
    }
}

// Bad — mixes high-level logic with low-level details
pub fn sys_connect(sockfd: i32, addr: Vaddr, len: u32) -> Result<()> {
    let fd_table = current_process().fd_table().lock();
    let file = fd_table.get(sockfd).ok_or(Errno::EBADF)?;
    let socket = file.downcast_ref::<Socket>().ok_or(Errno::ENOTSOCK)?;
    let bytes = read_bytes_from_user(addr, len as usize)?;
    let family = u16::from_ne_bytes([bytes[0], bytes[1]]);
    // ... 30 more lines of byte parsing ...
}
```

See also:
_Clean Code_, Chapter 3 "Functions";
PR [#639](https://github.com/asterinas/asterinas/pull/639#discussion_r1524629393).

### Hide details behind interfaces (`information-hiding`) {#information-hiding}

Hide details behind well-defined interfaces.
A module's public surface should contain
only what its consumers need.
Internal data structures, helper types,
and bookkeeping fields should remain private,
and implementation details should not leak
through public APIs (including their documentation).

See also:
[`narrow-visibility`](rust-specific/crates-and-modules.md#narrow-visibility)
for Rust-specific visibility rules;
PR [#2951](https://github.com/asterinas/asterinas/pull/2951#discussion_r2786925410).

### Be open for extension, closed for modification (`open-closed`) {#open-closed}

Stable modules and APIs should be
open to extension
but closed to breaking modification.
Prefer adding new behavior
through existing interfaces
(traits, enums, and pluggable components)
instead of repeatedly editing established call paths.
Do not introduce extension points preemptively;
add them when there is a concrete extension need.

### Aim for loose coupling and strong cohesion (`coupling-cohesion`) {#coupling-cohesion}

Connections between modules should be
small, visible, and flexible.
Within a module, every part should contribute
to a single, well-defined purpose.

### Be consistent (`consistency`) {#consistency}

Do similar things the same way throughout the codebase.
Consistency reduces surprise and cognitive load
even when neither approach is objectively superior.
When a convention already exists, follow it;
do not introduce a competing convention
without compelling justification.

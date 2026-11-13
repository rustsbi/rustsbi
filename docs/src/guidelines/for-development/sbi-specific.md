# SBI and Firmware Contracts

### Stop harts without returning (`hsm-stop-lifecycle`) {#hsm-stop-lifecycle}

On success, `hart_stop` must park the calling hart in firmware
until an accepted `hart_start` transfers control to the new entry point.
Keep a working restart notification path while the hart is stopped.
Return to the supervisor caller only if the stop request fails,
using the error specified by HSM.

```rust
// Bad — waking from WFI resumes the old supervisor caller
local_hsm().stop();
riscv::asm::wfi();
SbiRet::success(0)
```

An interrupt or a spurious wakeup can resume execution after `wfi`.
Recheck the stopped state after waking and wait for an accepted start request.
Test both parking and transfer to the new entry point.

### Map each SBI argument explicitly (`sbi-argument-order`) {#sbi-argument-order}

Map the caller's registers to the specified arguments for each extension and function ID.
Translate explicitly when an internal API uses a different order.
DBTR uninstall, enable and disable take the trigger base before the mask,
while `TriggerMask::from_mask_base` takes the mask first.

```rust
// Good — name the ABI roles before converting to the internal representation
let trigger_base = param0;
let trigger_mask = param1;
let triggers = TriggerMask::from_mask_base(trigger_mask, trigger_base);
dbtr.disable_triggers(triggers);

// Bad — register order is assumed to match constructor order
dbtr.disable_triggers(TriggerMask::from_mask_base(param0, param1));
```

Test with different base and mask values and check the selected objects.
Supply the specification's register encoding directly on at least one side of the test:
a caller and dispatcher that swap the same arguments can pass a round-trip test.

### Return the specified errors (`sbi-error-codes`) {#sbi-error-codes}

Each SBI function has its own result conditions and allowed errors.
Check every return path against that function's table in the implemented SBI version.
Distinguish invalid arguments, missing shared memory, denied access and backend failure.

For SBI 3.0 MPXY Get Channel IDs,
the permitted results are `SUCCESS`, `INVALID_PARAM`, `NO_SHMEM`, `DENIED` and `FAILED`.
Return `FAILED` for an unspecified backend failure
after handling the function's argument, shared-memory and permission checks.

### Preserve the disable encoding (`shmem-disable-value`) {#shmem-disable-value}

Decode an SBI control value according to the function that defines it.
STA selects the disabled state when both XLEN-sized address halves are all ones.
Physical address zero remains valid when it meets the alignment and accessibility requirements.
Keep enabled state distinct from the stored address internally.

```rust
// Good — decode the control value before validating ordinary addresses
let address = if lo == usize::MAX && hi == usize::MAX {
    None
} else {
    Some((lo, hi))
};
if let Some(address) = address {
    validate_writable_range(address, SHMEM_SIZE)?;
}
```

### Claim device interrupts atomically (`atomic-csr-claim`) {#atomic-csr-claim}

For AIA `*topei`, a write claims the current top external interrupt,
and the written value is ignored.
Use `csrrw rd, *topei, x0` with a nonzero destination register
to return and claim the same interrupt in one instruction.
A higher-priority interrupt can change the top entry between separate read and write instructions.

See [Expose side effects as operations](csr-access.md#expose-effects-as-operations)
for the `claim()` API and
[Complete device handshakes in a single instruction](csr-access.md#single-instruction-handshake)
for the atomicity requirement.

### Preserve the naked call ABI (`naked-call-abi`) {#naked-call-abi}

Keep the stack 16-byte aligned at function entry under the standard RV32 and RV64 C ABIs,
including calls between naked functions.
Naked functions implement their declared calling convention in assembly.
Check the stack adjustment at every assembly-to-C or assembly-to-Rust call,
using the alignment required by the selected ABI.

```asm
# Good — RV32 saves ra in a full aligned frame
addi sp, sp, -16
sw ra, 12(sp)
call callee
lw ra, 12(sp)
addi sp, sp, 16
ret

# Bad — saving one RV32 word misaligns an initially aligned stack
addi sp, sp, -4
sw ra, 0(sp)
call callee
```

See [Keep the trap entry ABI aligned](assembly-conventions.md#trap-entry-abi-alignment)
for the stack and scratch-register requirements at trap entry.

### Fence ADUE changes (`translation-fence`) {#translation-fence}

After changing `menvcfg.ADUE`, execute `sfence.vma x0, x0`
before resuming execution that relies on the new A/D-bit interpretation.
When the H extension is implemented, also execute `hfence.gvma x0, x0`
to synchronize the affected G-stage and VS-stage state.
Cached translations can otherwise retain the previous interpretation.
Apply this synchronization to every path that changes ADUE, including hart reset.

### Reject incompatible firmware inputs (`firmware-build-spec`) {#firmware-build-spec}

Resolve CLI options, configuration files and other build inputs
into one validated description of the firmware before building it.
Validate the combination after applying overrides, with explicit input precedence.
Reject inputs that conflict with the selected firmware mode.
For `FW_JUMP`, reject a next-stage binary override.
For `FW_PAYLOAD` with fixed payload placement, reject a next-stage address override.

```rust
// Good — check the merged inputs before resolving the next stage
match mode {
    Mode::Jump if next_stage_binary.is_some() => {
        return Err(Error::ConflictingStageSources);
    }
    Mode::Payload if next_stage_address.is_some() => {
        return Err(Error::ConflictingStageSources);
    }
    _ => {}
}
```

Derive Cargo arguments, linker configuration, artifact names and run settings
from the resolved build description.

### Run kernels in test and bench commands (`run-firmware-tests`) {#run-firmware-tests}

A test or benchmark command must execute the corresponding kernel by default
and decide success from its runtime result.
Run it in QEMU, another suitable emulator or hardware,
and wait for the expected completion result.
Propagate runtime failures, timeouts and missing completion results as command failures.
An explicit `--no-run` option may perform a build only;
report that outcome as a completed build.

### Measure changes to hot paths (`measure-hot-paths`) {#measure-hot-paths}

When changing inlining, dispatch or abstractions on a frequently executed firmware path,
compare the alternatives with the bench kernel or a representative workload.
For trap handlers, include the effect on the dispatcher and the handled workload.
Hold the toolchain, target and firmware configuration constant across runs.

Record the tested revisions, workload, instruction count or elapsed time,
and affected code size alongside the chosen trade-off.
Label QEMU guest instruction counts and physical-board latency measurements
with their respective execution environments.

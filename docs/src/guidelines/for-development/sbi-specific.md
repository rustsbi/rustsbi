# SBI and Firmware Contracts

### Stop harts without returning (`hsm-stop-lifecycle`) {#hsm-stop-lifecycle}

On the normal success path, `hart_stop` must stop the calling hart
without returning to its previous supervisor context.
A single `wfi` does not establish that contract:
an interrupt or a spurious wakeup may resume execution after it.
Keep the hart stopped until an accepted start request performs the next-stage handoff.
The stopped state must retain a working restart notification path.

Delegate parking and restart to a runtime operation
whose successful result cannot return to the old context.
Failures may return the error allowed by the HSM specification.

```rust
// Good — stop_current returns only on failure
match hart::stop_current() {
    Ok(never) => match never {},
    Err(_) => SbiRet::failed(),
}
```

```rust
// Bad — waking from WFI resumes the old supervisor caller
local_hsm().stop();
riscv::asm::wfi();
SbiRet::success(0)
```

Exercise both stop and restart:
the old call must not return success,
and a later start request must make the hart execute the new entry point.
This rule does not make other SBI calls non-returning.

### Map each SBI argument explicitly (`sbi-argument-order`) {#sbi-argument-order}

For each extension and function ID,
check the specification's argument order against the registers supplied by the caller
and the arguments passed to the implementation.
An internal Rust constructor can use a different order,
but the boundary must translate it explicitly.
Equal argument types do not establish equal meanings.

For example, DBTR uninstall, enable and disable take the trigger base before the mask,
while a mask-first constructor needs the opposite order.

```rust
// Good — name the ABI roles before converting to the internal representation
let trigger_base = param0;
let trigger_mask = param1;
let triggers = TriggerMask::from_mask_base(trigger_mask, trigger_base);
dbtr.disable_triggers(triggers)
```

```rust
// Bad — register order is assumed to match constructor order
dbtr.disable_triggers(TriggerMask::from_mask_base(param0, param1))
```

Test with different base and mask values and check the selected objects.
A caller and a dispatcher that make the same mistake can pass a round-trip test;
at least one side of the test must supply the specification's register encoding directly.
The internal API need not change its order to match the external ABI.

### Return the specified errors (`sbi-error-codes`) {#sbi-error-codes}

Each SBI function has its own result conditions and allowed errors.
Check every return path against that function's table in the implemented SBI version.
Distinguish invalid arguments, missing shared memory, denied access and backend failure.
Do not use `NOT_SUPPORTED` as a catch-all for an absent device or unavailable backend.

For SBI 3.0 MPXY Get Channel IDs,
the permitted results are `SUCCESS`, `INVALID_PARAM`, `NO_SHMEM`, `DENIED` and `FAILED`.
After checking the specific conditions,
an unspecified backend failure belongs to `FAILED`, not `NOT_SUPPORTED`.

```rust
// Good — earlier checks already handled arguments, shared memory and permission
match backend.channel_ids(start_index) {
    Ok(ids) => write_channel_ids(ids),
    Err(_) => SbiRet::failed(),
}
```

```rust
// Bad — an implemented function returns an error absent from its error table
match backend.channel_ids(start_index) {
    Ok(ids) => write_channel_ids(ids),
    Err(_) => SbiRet::not_supported(),
}
```

This does not require implementing every optional extension.
Extension discovery and rejection of unsupported functions have their own contracts;
do not confuse those with a failure inside an implemented function.

### Preserve the disable encoding (`shmem-disable-value`) {#shmem-disable-value}

Decode an SBI control value according to the function that defines it.
For STA and other interfaces that disable shared memory with two all-ones XLEN words,
both address halves must be all ones to select the disabled state.
Physical address zero is not a substitute for that encoding.
It may be valid if it meets the function's alignment and accessibility requirements.

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

```rust
// Bad — a valid physical address is confused with disabled state
let address = if lo == 0 && hi == 0 {
    None
} else {
    Some((lo, hi))
};
```

Keep enabled state distinct from the stored address internally as well.
An `Option` or a separate state field is suitable;
the internal representation need not use the wire encoding.
Other interfaces can define different sentinels and must follow their own specification.

### Claim device interrupts atomically (`atomic-csr-claim`) {#atomic-csr-claim}

Some CSR accesses perform device operations instead of ordinary state assignment.
For AIA `*topei`, a write claims the current top external interrupt,
and the written value is ignored.
Expose the operation as `claim()` rather than `write(value)`.
Use a single CSR instruction to read and claim the same interrupt.

```rust
// Good — one operation, with no ignored interrupt-ID argument
/// # Safety
/// The caller must have M-mode access to an implemented IMSIC interrupt file.
unsafe fn claim() -> usize {
    let value;
    // SAFETY: the caller guarantees access to this interrupt file.
    unsafe {
        core::arch::asm!(
            "csrrw {value}, mtopei, zero",
            value = out(reg) value,
            options(nostack),
        );
    }
    value
}
```

```rust
// Bad — the write may claim an interrupt that arrived after the read
let interrupt = mtopei::read();
mtopei::write(interrupt);
```

A newly pending higher-priority interrupt can change the top entry between two instructions.
Disabling local interrupt delivery does not stop the device from changing that entry.
Making the accessor read-only also loses the required claim operation.
Ordinary CSR fields without such effects can still use ordinary read and write APIs.

### Preserve the naked call ABI (`naked-call-abi`) {#naked-call-abi}

`#[unsafe(naked)]` removes the compiler-generated prologue and epilogue;
it does not remove the function's declared calling convention.
Under the standard RV32 and RV64 C ABIs,
keep the stack 16-byte aligned when entering a function.
Check the stack adjustment at every assembly-to-C or assembly-to-Rust call,
including calls between naked functions.

```asm
# Good — RV32 saves ra in a full aligned frame
addi sp, sp, -16
sw ra, 12(sp)
call callee
lw ra, 12(sp)
addi sp, sp, 16
ret
```

```asm
# Bad — saving one RV32 word misaligns an initially aligned stack
addi sp, sp, -4
sw ra, 0(sp)
call callee
```

On RV64, saving an eight-byte return address still needs an aligned frame,
not an eight-byte stack adjustment before a call.
Use the requirements of the actual ABI for variants such as ILP32E.
An explicitly documented custom assembly convention is a separate contract;
an `extern "C"` declaration cannot silently opt into it.

### Fence ADUE changes (`translation-fence`) {#translation-fence}

Changing a control bit that affects page-table interpretation
can leave address-translation caches using the old interpretation.
Include the synchronization required by the privileged architecture
in the operation that changes that control state.
Writing the CSR alone is not sufficient.

For a change to `menvcfg.ADUE`,
`sfence.vma x0, x0` synchronizes the local S-mode translation state.
When the H extension is implemented,
also satisfy its additional synchronization requirements for G-stage and VS-stage state,
including `hfence.gvma x0, x0` for the affected interpretation.

```rust
// Good — the fence helpers execute the architectural instructions named above
if previous_adue != requested_adue {
    set_adue(requested_adue)?;
    sfence_vma_all();
    if hart_has_h_extension() {
        hfence_gvma_all();
    }
}
```

```rust
// Bad — the next stage can reuse translations with the old A/D-bit interpretation
set_adue(requested_adue)?;
return SbiRet::success(0);
```

Derive the fence scope from the actual state change and the relevant architecture rules.
Do not add an unconditional full flush to every context transfer
when no state requiring synchronization has changed.

### Reject incompatible firmware inputs (`firmware-build-spec`) {#firmware-build-spec}

Resolve CLI options, configuration files and other build inputs
into one validated description of the firmware before building it.
Reject inputs that conflict with the selected firmware mode
instead of silently ignoring them.
For example, `FW_JUMP` must reject a next-stage binary override,
and `FW_PAYLOAD` must reject a next-stage address override
when its payload placement is fixed by the firmware contract.

Derive Cargo arguments, linker configuration, artifact names and run settings
from that same resolved description.
Later stages must not independently reinterpret the original inputs.

```rust
// Good — resolution rejects inconsistent inputs before any build stage
let spec = resolve_and_validate(cli, config, environment)?;
let artifact = build_firmware(&spec)?;
run_firmware(&spec, &artifact)?;
```

```rust
// Bad — a supplied binary is silently discarded in jump mode
let next_stage = match mode {
    Mode::Jump => NextStage::Jump { address: cli.address },
    Mode::Payload => NextStage::Payload { binary: cli.binary },
    Mode::Dynamic => NextStage::Dynamic,
};
```

Environment variables and device-tree inputs are allowed,
but their resolved values and precedence must be explicit.
Validate the combination after applying configuration and CLI overrides;
validating a configuration file before overrides does not validate the final request.
The build description must be sufficient to determine what image is produced
and which configuration the run or test stage exercises.

### Run kernels in test and bench commands (`run-firmware-tests`) {#run-firmware-tests}

A test or benchmark command must execute the corresponding kernel by default
and decide success from its runtime result.
Cross-compilation, packing and copying artifacts only establish that a build completed.
They do not establish that the firmware booted or its tests passed.
Trace each `test` and `bench` command through its shared helpers:
returning the build process's exit status does not establish a runtime result,
even if the command never prints an explicit "tests passed" message.

```rust
// Good — build-only mode and a verified execution have different outcomes
let artifact = build_test_kernel(&spec)?;
if args.no_run {
    report_build_complete(&artifact);
} else {
    let result = run_test_kernel(&spec, &artifact)?;
    verify_test_results(&result)?;
    report_tests_passed();
}
```

```rust
// Bad — no test kernel has executed
let artifact = build_test_kernel(&spec)?;
copy_artifact(&artifact)?;
report_tests_passed();
```

Check the expected completion results as well as execution failures.
A timeout, early termination or missing success result must not count as a pass.
An explicit `--no-run` option may skip execution,
but its result must say that only the build was performed.
QEMU, another suitable emulator or hardware can provide the execution environment.

### Request trap-handler inlining benchmarks (`measure-hot-paths`) {#measure-hot-paths}

Changes to trap handling, instruction emulation or other frequent firmware paths
need measurements when choosing an inlining, dispatch or abstraction strategy.
An explicit inlining annotation on a trap handler is such a choice,
even when that exception is rare in some workloads.
Check its effect on the dispatcher as well as the handler's intended workload.
An annotation such as `inline(always)` or `inline(never)` does not prove an improvement.
Compare the relevant alternatives with the bench kernel or a representative workload,
using the same environment and configuration.
When a reviewed change chooses an explicit inlining strategy
without supplying comparative results, request those measurements.
This is a request for evidence supporting the choice;
it does not establish that measurements are absent elsewhere
or that the change causes a regression.
A rationale comment alone does not supply that comparison.
Functional correctness does not resolve this performance decision.
If the review input omits the performance report,
ask for the comparison rather than assume it exists or assert that it was never made.
Record the alternatives and results that support the chosen strategy.

```text
Good — evidence attached to the performance decision
Baseline and candidate revisions: recorded
Toolchain, target, firmware configuration and workload: held constant
Alternatives: default inlining and the proposed explicit inlining strategy
Results: instruction count or elapsed time, plus affected code size
Decision: the chosen strategy and any measured trade-off
```

```rust
// Bad — an unsupported performance claim decides the annotation
#[inline(always)] // Always faster on the trap path.
fn emulate_access(frame: &mut TrapFrame) {
    dispatch_access(frame);
}
```

State what was measured:
QEMU guest instruction counts are not physical-board latency measurements.
Record the tested revisions and any unmeasured platform limits.
Measurements can justify a trade-off;
this rule neither forbids every regression nor requires benchmarking unrelated changes.

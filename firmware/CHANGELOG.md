# Changelog

All notable changes to this project will be documented in this file. See [conventional commits](https://www.conventionalcommits.org/) for commit guidelines.

---

## Unreleased

### Added

- Register V821 custom SBI extensions through derive and publish the dispatcher directly.
- Add Avaota F1 build settings with target JSON and key loader DTB nodes documented inline.
- Support V821 AWBASE USB enable through the DTB-described DMA word-address bypass register.
- Support V821 Andes cache-maintenance SBI calls using the device-tree L2 cache window.
- Configure the V821 noncacheable physical alias through PMA15.
- Support the Avaota F1 noncacheable RAM alias in PMP while denying aliases of firmware memory.
- Discover and bind Andes PLMT timers and Sunxi PLICSW IPIs through the device tree.
- Describe V821 PLMT clock control and its noncacheable alias from the device-tree root compatible.
- Add configurable timer expiry acknowledgement while retaining explicit cancellation.
- Make Runtime's event, IPI, interrupt-controller and timer service interfaces available on host targets.
- Add a separate Sunxi WDT V105 reboot backend, discovering watchdog and RTC V203 GPRCM addresses from the device tree.
- Add test-kernel checks for misaligned accesses, trap register preservation and redirection.
- Add Runtime-owned trap handling, guarded CSR access, hart lifecycle and local storage.
- Support `--no-default-features` in firmware builds and track it in the build stamp.
- Add an Avaota F2 RV32 configuration and V861 reset-vector hart wake backend.
- Discover the shared Sunxi watchdog through `allwinner,sun20i-d1-wdt` and `allwinner,wdt-v104` device-tree nodes.
- Add independent syscon poweroff and reboot peripherals with device-tree discovery to RustSBI Prototyper.
- Add Allwinner F101 watchdog reboot support, requiring an enabled D1-compatible watchdog device-tree node.
- Add RV32 support to bench-kernel while preserving RV64 support.
- Add DBCN contract tests with deferred error reporting to the test kernel.
- Add RV32 support to RustSBI Prototyper and test-kernel while preserving RV64 support.
- Add Yuzuki Neko board configuration with an RV32 firmware jump target of `0x40000000`.
- Add SBI Collaborative Processor Performance Control extension support to RustSBI Prototyper.
- Add SBI Message Proxy extension support to RustSBI Prototyper.
- Add SBI Steal-time Accounting extension support to RustSBI Prototyper.
- Add SBI Debug Triggers extension support to RustSBI Prototyper.
- Add SBI Firmware Features extension support to RustSBI Prototyper.
- Add SBI Nested Acceleration extension support to RustSBI Prototyper.
- Add SBI Supervisor Software Events scaffolding to RustSBI Prototyper; the extension remains
  unavailable until supervisor context switching is implemented.
- Add AIA IMSIC IPI backend support for RustSBI Prototyper.
- Add SpacemiT K1 SoC platform support for RustSBI Prototyper, including OrangePi RV2 board configuration.

### Modified
- Initialize Smstateen before supervisor handoff, expose supported S-mode state, and clear lower-level state-enable registers.
- Resolve firmware Clippy warnings and use a named Runtime IPI error type.
- Split PMP logging ranges to avoid equal-operand comparisons in macro expansion.
- Move the IPI backend interface into a directory module.
- Keep complex trap branches out of the common entry prologue.
- Avoid full-context time reads for caller-saved destinations.
- Skip trap CSR recovery bookkeeping when emulating an instruction supplied in mtval with a direct device counter.
- Allow timer backends to provide direct counter words for CSR emulation without probing architecture CSRs.
- Reject CSRRS writes during read-only CSR emulation instead of silently treating them as reads.
- Omit x0 storage from trap frames while preserving register mapping and ABI stack alignment.
- Avoid locking side-effect-free timer reads while keeping comparator writes serialized.
- Inline Runtime MMIO reads and address validation into device access paths.
- Name Runtime interrupt enables by privilege mode and leave `stvec`/`sscratch` untouched on HSM entry.
- Reuse Runtime Sstc detection and skip firmware PMU bookkeeping when no PMU is installed.
- Preserve `mstatus` and declare all clobbers across guarded CSR and memory fault recovery.
- Split the test kernel into boot, platform, PMU, RFENCE, misaligned-access and trap modules.
- Compose SBI extensions in Prototyper over Runtime trap and hart mechanisms and independent devices.
- Size per-hart storage through Runtime configuration instead of Prototyper board settings.
- Build Avaota F2 with compile-time INFO logging and size optimization level `s` to reduce firmware size and startup time.
- Match escaped paths in generated firmware source tests on Windows.
- Use hyphen-separated board configuration filenames.
- Reuse mtval instruction bits for CSR read emulation, falling back to guarded instruction fetch.
- Enable V861 C907 MHCR bits 12 and 24 on both harts while preserving the remaining bootloader cache policy.
- Share the `SunxiWdtV104` reboot backend between D1-compatible and V104 watchdogs and remove the F101 fixed-address fallback.
- Complete pending remote fences across hart transitions and skip stopped harts in valid IPI masks.
- Count and forward access faults to supervisor mode, preserving the interrupted interrupt state.
- Accept standard firmware PMU events, including events that cannot occur on the current ISA.
- Support fixed PMU cycle and instruction counters without device-tree event mappings.
- Clear stale registers on HSM entry, including hart restart and non-retentive resume.
- Enable PBMTE for the C907 RV32 page-memory-type extension when the device tree advertises `svpbmt`.
- Reuse a firmware reservation that already covers the image to avoid a redundant DTB copy.
- Recognize the Allwinner T-Head PLIC compatible string.
- Recognize the V861 UART compatible string.
- Fix RV32 PMU counter-mask validation and skip the time counter when stopping selected counters.
- Clarify the SpacemiT P1 PMIC reset field and platform log naming.
- Probe Sstc per hart so old device trees do not prevent supervisor timer access.
- fix(prototyper): correct CBIE invalidate encoding (#275)
- Isolate RFence queues, completion counters and IPI flags to reduce cache-line contention.
- Avoid redundant RFence queue locks and share IPI backends without a global lock, preserving I/O ordering.
- Execute self RFence locally and validate selected IPI harts without ordinary-mask heap allocation.
- Skip inactive firmware PMU scans, iterate selected IPI harts and benchmark local RFence calls.
- Avoid RFence stalls by servicing local requests when a remote queue lock is busy.
- Skip instruction fetch on SBI return and add batched per-call latency benchmarks with SBI console output.
- Generalize CCI-550, derive per-hart availability and fix K1 setup, I/O ordering and HSM restart with PMU wakeup.
- Remove K1-only initialization logging from platform publication.
- Reuse generic UART backends for K1 and preserve the baud divisor when its clock rate is unknown.
- Enable T-Head PLIC supervisor access and configure Svpbmt from the device tree.
- Clear T-Head MAEE for standard RV32 or Svpbmt page tables.
- Replace Prototyper IPI devices with fallible `IpiBackend` windows and SBI-layer target validation.
- Replace Prototyper console devices with fallible, non-blocking `DbcnBackend` slice operations.
- Replace Prototyper reset devices with typed requests and a fallible `ResetBackend` interface.
- Forward unsupported misaligned loads to S-mode so Linux vector alignment probes do not panic.
- refactor(prototyper): unify build commands (#227)
- deps: update `sbi-spec` to version 0.0.10.
- test-kernel: update PMU flag parameter trait names.
- Refine CSR group comments.
- fix(prototyper): temporary PMU fix for possible S-mode DTB modification
- fix(prototyper): validate DBCN console shared memory range

### Removed
- Remove Prototyper's fast-trap integration and unavailable NACL/SSE adapters.

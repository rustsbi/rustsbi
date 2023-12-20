// RustSBI derive example. To derive RustSBI implementation, first we use `RustSBI`
// derive macro using code `use rustsbi::RustSBI`.
use rustsbi::RustSBI;
mod commons;
use commons::*;

// Now we create a new structure and fill fields into it.
#[derive(RustSBI)]
struct MySBI {
    // We include a SBI RFNC (rustsbi::Fence) extension implementation by including
    // a struct field. The name `fence` is special; RustSBI derive macro will identify
    // fence implementation using the variable name. Valid names are listed in RISC-V
    // SBI specification.
    // Here we include a mock MyFence implementation; this structure prints to output
    // then the SBI function `remote_fence_i` is called. Actual code should use any
    // machine-mode mechanism as a valid RISC-V SBI implementation.
    fence: MyFence,
    // Machine information is required by RISC-V SBI specification to provide supervisor
    // with some method to read `mvendorid`, `marchid` and `mimpid` values from the SBI
    // environment.
    // By default RustSBI requires the implementation to declare machine info values
    // for the environment explicitly, which is suitable for emulators and hypervisors.
    // For bare metal developers, RustSBI also provides a way to read from machine-mode
    // CSR accesses; developers should enable RustSBI feature `machine` in this case.
    // The name `info` is also special, like the name `fence` we have mentioned;
    // RustSBI identifies machine information from the field name `info`.
    info: MyMachineInfo,
}

// We have a properly defined RustSBI implementation called `MySBI`. Now `MySBI`
// implements Rust trait `rustsbi::RustSBI` with derived code dealing with RISC-V
// SBI extensions, functions and forward it to all fields of `MySBI` with minimum
// runtime cost. Let's try to use it!

fn main() {
    // In main program, create an SBI instance. It's normally located in global storages
    // like global variables or stack of the main function. As a mock example we define it
    // as a stack variable for now.
    let sbi = MySBI {
        fence: MyFence,
        info: MyMachineInfo,
    };

    // In S-mode environment call handler, call the `handle_ecall` of the SBI instance.
    // We mock this method by providing consts here; actual implementation should fill
    // `extension`, `function` and `param` from trap context.
    let ret = sbi.handle_ecall(sbi_spec::rfnc::EID_RFNC, 0, [0; 6]);

    // Finally, fill SBI return value into exception environment and return.
    // In bare metal: fill `a0` and `a1` register in trap context with `SbiRet` value;
    // In hypervisor: fill guest supervisor `a0` and `a1` with `SbiRet` value.
    let _ = ret; // It should be filled into context on real programs.

    // Congratulations! You have learned how to use RustSBI to create your SBI implementaion.
    // You may consider using the RustSBI Prototyping System, build a standalone
    // binary package with runtime environment from scratch, or begin with your hypervisor
    // development.

    // Additionally, we present another mock function suggesting this instance is running
    // RustSBI by showing that SBI implementation ID equals 4.
    let ret = sbi.handle_ecall(0x10, 0x1, [0; 6]);
    println!("SBI implementation ID: {:x?}", ret.value);
}

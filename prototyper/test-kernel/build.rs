use std::{env, path::PathBuf};

fn main() {
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let ld = &out.join("rustsbi-test-kernel.ld");
    println!("cargo:rerun-if-env-changed=RUSTSBI_TEST_LINK_ADDRESS");
    let default_address = match env::var("CARGO_CFG_TARGET_POINTER_WIDTH").as_deref() {
        Ok("32") => 0x8040_0000,
        Ok("64") => 0x8020_0000,
        _ => panic!("the test kernel supports only 32-bit and 64-bit RISC-V targets"),
    };
    let link_address = env::var("RUSTSBI_TEST_LINK_ADDRESS")
        .map(|value| {
            let digits = value.strip_prefix("0x").unwrap_or(&value);
            usize::from_str_radix(digits, 16)
                .unwrap_or_else(|_| panic!("invalid test-kernel link address: {value}"))
        })
        .unwrap_or(default_address);
    assert_eq!(
        link_address & 0xfff,
        0,
        "test-kernel link address must be page aligned"
    );
    let link_address = format!("0x{link_address:x}");

    // QEMU's RISC-V direct-kernel convention reserves a 4 MiB offset on RV32
    // and a 2 MiB offset on RV64. The raw image is not relocatable, so its
    // link address must agree with the address supplied in the dynamic boot
    // information.
    std::fs::write(ld, LINKER_SCRIPT.replace("@LINK_ADDRESS@", &link_address)).unwrap();

    println!("cargo:rustc-link-arg=-T{}", ld.display());
    println!("cargo:rustc-link-search={}", out.display());
}

const LINKER_SCRIPT: &str = "OUTPUT_ARCH(riscv)
ENTRY(_start) 
SECTIONS {
    . = @LINK_ADDRESS@;
    istart = .;
	  .head.text : ALIGN(8) {		
        KEEP(*(.head.text))
	  }

    .text : ALIGN(8) { 
        *(.text.entry)
        *(.text .text.*)
    }
    .rodata : ALIGN(8) { 
        srodata = .;
        *(.rodata .rodata.*)
        *(.srodata .srodata.*)
        . = ALIGN(8);  
        erodata = .;
    } 
    .data : ALIGN(8) { 
        sdata = .;
        *(.data .data.*)
        *(.sdata .sdata.*)
        . = ALIGN(8); 
        edata = .;
    }
    sidata = LOADADDR(.data);
    .bss (NOLOAD) : ALIGN(8) {  
        *(.bss.uninit)
        sbss = .;
        *(.bss .bss.*)
        *(.sbss .sbss.*)
        ebss = .;
    } 
    iend = .;
    /DISCARD/ : {
        *(.eh_frame)
    }
}";

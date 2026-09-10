use std::{env, path::PathBuf};

fn main() {
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let ld = &out.join("rustsbi-test-kernel.ld");

    println!("cargo:rerun-if-env-changed=RUSTSBI_BENCH_LINK_ADDRESS");
    let address = env::var("RUSTSBI_BENCH_LINK_ADDRESS").unwrap_or_else(|_| "0x80200000".into());
    let address = usize::from_str_radix(address.trim_start_matches("0x"), 16)
        .expect("RUSTSBI_BENCH_LINK_ADDRESS must be a hexadecimal address");
    let script = std::str::from_utf8(LINKER_SCRIPT)
        .unwrap()
        .replace("0x80200000", &format!("0x{address:x}"));
    std::fs::write(ld, script).unwrap();

    println!("cargo:rustc-link-arg=-T{}", ld.display());
    println!("cargo:rustc-link-search={}", out.display());
}

const LINKER_SCRIPT: &[u8] = b"OUTPUT_ARCH(riscv)
ENTRY(_start) 
SECTIONS {
    . = 0x80200000;
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

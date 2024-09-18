fn main() {
    use std::{env, fs, path::PathBuf};

    let ld = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("linker.ld");
    fs::write(&ld, LINKER).unwrap();
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=LOG");
    println!("cargo:rustc-link-arg=-T{}", ld.display());
}

const LINKER: &[u8] = b"
OUTPUT_ARCH(riscv)
ENTRY(_start)
MEMORY {
    RAM : ORIGIN = 0x0, LENGTH = 64M
}
SECTIONS {
    .text : {
        *(.text.entry)
        *(.text .text.*)
    } > RAM
    .rodata : {
        *(.rodata .rodata.*)
        *(.srodata .srodata.*)
    } > RAM
    .data : {
        sidata = LOADADDR(.data);
        sdata = .;
        *(.data .data.*)
        *(.sdata .sdata.*)
        edata = .;
    } > RAM
    .bss (NOLOAD) : {
        *(.bss.uninit)
        . = ALIGN(8);
        sbss = .;
        *(.bss .bss.*)
        *(.sbss .sbss.*)
        . = ALIGN(8);
        ebss = .;
    } > RAM
    /DISCARD/ : {
        *(.eh_frame)
    }
}";

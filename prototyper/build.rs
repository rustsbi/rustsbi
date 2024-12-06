use std::{env, path::PathBuf};

fn main() {
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let ld = &out.join("rustsbi-prototyper.ld");

    std::fs::write(ld, LINKER_SCRIPT).unwrap();

    println!("cargo:rerun-if-env-changed=RUST_LOG,PROTOTYPER_FDT,PROTOTYPER_IMAGE");
    println!("cargo:rustc-link-arg=-T{}", ld.display());
    println!("cargo:rustc-link-search={}", out.display());
}

const LINKER_SCRIPT: &[u8] = b"OUTPUT_ARCH(riscv)
ENTRY(_start) 
SECTIONS {
    . = 0x80000000;

    . = ALIGN(0x1000); /* Need this to create proper sections */

    sbi_start = .;
    .text : ALIGN(8) { 
        *(.text.entry)
        *(.text .text.*)
    }

    . = ALIGN(0x1000); /* Ensure next section is page aligned */

    .rodata : ALIGN(8) { 
        srodata = .;
        *(.rodata .rodata.*)
        *(.srodata .srodata.*)
        . = ALIGN(8);  
    } 

    .dynsym : ALIGN(8) {
        *(.dynsym)
    }

    . = ALIGN(0x1000); /* Ensure next section is page aligned */

    .rela.dyn : ALIGN(8) {
        __rel_dyn_start = .;
        *(.rela*)
        __rel_dyn_end = .;
    }

    erodata = .;

	/*
	 * PMP regions must be to be power-of-2. RX/RW will have separate
	 * regions, so ensure that the split is power-of-2.
	 */
	. = ALIGN(1 << LOG2CEIL((SIZEOF(.rodata) + SIZEOF(.text)
				+ SIZEOF(.dynsym) + SIZEOF(.rela.dyn))));

    .data : ALIGN(8) { 
        sdata = .;
        *(.data .data.*)
        *(.sdata .sdata.*)
        . = ALIGN(8); 
        edata = .;
    }
    sidata = LOADADDR(.data);

    . = ALIGN(0x1000); /* Ensure next section is page aligned */

    .bss (NOLOAD) : ALIGN(8) {  
        *(.bss.uninit)
        sbss = .;
        *(.bss .bss.*)
        *(.sbss .sbss.*)
        ebss = .;
    } 
    /DISCARD/ : {
        *(.eh_frame)
    }

	. = ALIGN(0x1000); /* Need this to create proper sections */
    sbi_end = .;

    .text 0x80200000 : ALIGN(8) {
        *(.payload)
    }
}";

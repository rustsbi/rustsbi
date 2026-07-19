use std::{env, path::PathBuf};

fn main() {
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let ld = &out.join("rustsbi-prototyper.ld");

    let mtest_section = if env::var_os("CARGO_FEATURE_MTEST").is_some() {
        let descriptor_size = match env::var("CARGO_CFG_TARGET_POINTER_WIDTH").as_deref() {
            Ok("32") => 12,
            Ok("64") => 24,
            _ => panic!("mtest requires a 32-bit or 64-bit pointer target"),
        };
        format!(
            ".mtest_array : ALIGN(8) {{\n        __mtest_start = .;\n        KEEP(*(.mtest_array))\n        __mtest_end = .;\n    }}\n    ASSERT(__mtest_end > __mtest_start, \"machine-test registry is empty\")\n    ASSERT((__mtest_end - __mtest_start) % {descriptor_size} == 0, \"machine-test descriptor layout mismatch\")"
        )
    } else {
        String::new()
    };
    let script = LINKER_SCRIPT.replace("/* MTEST_SECTION */", &mtest_section);
    std::fs::write(ld, script).unwrap();

    println!(
        "cargo:rerun-if-env-changed=RUST_LOG,PROTOTYPER_FDT,PROTOTYPER_IMAGE,RUSTSBI_MTEST_FILTER,RUSTSBI_TEST_SHARD,RUSTSBI_TEST_RUN_ID,RUSTSBI_TEST_DIGEST"
    );
    println!("cargo:rustc-link-arg=-T{}", ld.display());
    println!("cargo:rustc-link-search={}", out.display());
}

const LINKER_SCRIPT: &str = "OUTPUT_ARCH(riscv)
ENTRY(_start) 
SECTIONS {
    . = 0x80000000;

    . = ALIGN(0x1000); /* Need this to create proper sections */
    sbi_start = .;

    .text : ALIGN(0x1000) { 
        *(.text.entry)
        *(.text .text.*)
    }

    . = ALIGN(0x1000);
    sbi_rodata_start = .;

    .rodata : ALIGN(0x1000) { 
        *(.rodata .rodata.*)
        *(.srodata .srodata.*)
        . = ALIGN(0x1000);  
    } 

    /* MTEST_SECTION */

    .dynsym : ALIGN(8) {
        *(.dynsym)
    }

    .rela.dyn : ALIGN(8) {
        __rel_dyn_start = .;
        *(.rela*)
        __rel_dyn_end = .;
    }

    . = ALIGN(0x1000);
    sbi_rodata_end = .;

	/*
	 * PMP regions must be to be power-of-2. RX/RW will have separate
	 * regions, so ensure that the split is power-of-2.
	 */
	/* . = ALIGN(1 << LOG2CEIL((SIZEOF(.rodata) + SIZEOF(.text)
				+ SIZEOF(.dynsym) + SIZEOF(.rela.dyn)))); */

    .data : ALIGN(0x1000) { 
        sbi_data_start = .;
        *(.data .data.*)
        *(.sdata .sdata.*)
        . = ALIGN(0x1000); 
        sbi_data_end = .;
    }
    sidata = LOADADDR(.data);

    .bss (NOLOAD) : ALIGN(0x1000) {  
        *(.bss.stack)
        . = ALIGN(0x1000);
        sbi_heap_start = .;
        *(.bss.heap)
        sbi_heap_end = .;
        . = ALIGN(0x1000); 
        sbi_bss_start = .;
        *(.bss .bss.*)
        *(.sbss .sbss.*)
        sbi_bss_end = .;
    } 

    /DISCARD/ : {
        *(.eh_frame)
    }

    . = ALIGN(0x1000);

    .text : ALIGN(0x1000) {
        *(.fdt)
    }
    . = ALIGN(0x1000);
    sbi_end = .;

    .handoff (NOLOAD) : ALIGN(0x1000) {
        sbi_handoff_start = .;
        KEEP(*(.handoff))
        . = ALIGN(0x1000);
        sbi_handoff_end = .;
    }
    ASSERT(sbi_handoff_end <= 0x80200000, \"firmware handoff storage overlaps payload\")

    .text 0x80200000 : ALIGN(0x1000) {
        *(.payload)
    }
}";

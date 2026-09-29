// FW_DYNAMIC's preferred relocation hart. Runs before relocation, BSS, or
// stacks exist; a2 points to the previous stage's DynamicInfo. Return the
// preferred hart ID in a0, or -1 to use the relocation lottery. Preserve a1
// and a2 so the common entry can pass them unchanged to Rust.

    li       a0, -1
    beqz     a2, 1f
.if {XLEN} == 64
    ld       t0, 0(a2)
    ld       t1, {DYNAMIC_INFO_VERSION_OFFSET}(a2)
.else
    lw       t0, 0(a2)
    lw       t1, {DYNAMIC_INFO_VERSION_OFFSET}(a2)
.endif
    li       t2, {DYNAMIC_INFO_MAGIC}
    bne      t0, t2, 1f
    li       t2, {DYNAMIC_INFO_VERSION_2}
    bne      t1, t2, 1f
.if {XLEN} == 64
    ld       a0, {DYNAMIC_INFO_BOOT_HART_OFFSET}(a2)
.else
    lw       a0, {DYNAMIC_INFO_BOOT_HART_OFFSET}(a2)
.endif
1:
    ret

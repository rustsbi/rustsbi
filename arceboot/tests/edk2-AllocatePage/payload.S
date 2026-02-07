    .text
    .p2align 2
    .global _start        # entry symbol name for payload (not used by EFI; we will call by address)
_start:
    # Convention:
    # a0 = framebuffer base (u64)
    # a1 = framebuffer size (u64)
    # a2 = stride_pixels (u32)  (optional)
    # a3 = width (u32)         (optional)
    # a4 = height (u32)        (optional)

    # Simple, position-independent loop: write 0xFF408DEB (32-bit) across framebuffer.
    li      t0, 0xff408deb    # color constant (fits 32-bit)
    mv      t1, a0            # ptr = fb_base
    add     t2, a0, a1        # end = fb_base + fb_size

1:  sw      t0, 0(t1)
    addi    t1, t1, 4
    bltu    t1, t2, 1b

    # never return: infinite loop
2:  j       2b

// payload_embed.S — return-only payload;

    .section .payload, "ax", @progbits
    .p2align 2
    .globl _payload_start
    .globl _payload_end
_payload_start:

    .globl entry
    .type  entry, @function
entry:
    add     t0, a0, a1
    add     t0, t0, a2
    add     t0, t0, a3
    add     t0, t0, a4
    li      t1, 0x123456789ABCDEF0
    add     a0, t0, t1
    ret

    .size entry, .-entry
_payload_end:

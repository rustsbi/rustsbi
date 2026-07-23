.option push
.option arch, +a

csrw mie, zero
li t0, {mprv}
csrc mstatus, t0
mv s0, a0
mv s1, a1
mv s2, a2
beqz s2, 90f
andi t0, s2, {xlen_align_mask}
bnez t0, 90f
addi t0, s2, {dynamic_size}
bltu t0, s2, 90f

.if {word_size} == 8
ld s3, 0 * {word_size}(s2)
ld s4, 1 * {word_size}(s2)
ld s5, 2 * {word_size}(s2)
ld s6, 3 * {word_size}(s2)
ld s7, 4 * {word_size}(s2)
ld s8, 5 * {word_size}(s2)
.else
lw s3, 0 * {word_size}(s2)
lw s4, 1 * {word_size}(s2)
lw s5, 2 * {word_size}(s2)
lw s6, 3 * {word_size}(s2)
lw s7, 4 * {word_size}(s2)
lw s8, 5 * {word_size}(s2)
.endif

li t0, {dynamic_magic}
bne s3, t0, 90f
li t0, 1
beq s4, t0, 2f
li t0, 2
bne s4, t0, 90f
li t0, -1
beq s8, t0, 2f
bne s0, s8, 3f
2:
lla t0, {early_state}
20:
lr.w.aq t2, (t0)
bnez t2, 3f
li t1, {initializing}
sc.w.rl t2, t1, (t0)
bnez t2, 20b
call {relocation_update}

lla t0, sbi_bss_start
lla t1, sbi_bss_end
10:
bgeu t0, t1, 11f
.if {word_size} == 8
sd zero, 0(t0)
.else
sw zero, 0(t0)
.endif
addi t0, t0, {word_size}
j 10b
11:

lla sp, {boot_stack}
li t0, {boot_stack_size}
add sp, sp, t0
andi sp, sp, -16
mv a0, s0
mv a1, s1
mv a2, s3
mv a3, s4
mv a4, s5
mv a5, s6
mv a6, s7
mv a7, s8
tail {cold_entry}

3:
lla t0, {early_state}
30:
lla t3, {early_failed}
amoadd.w.aq t1, zero, (t3)
bnez t1, 90f
amoadd.w.aq t1, zero, (t0)
li t2, {ready}
bne t1, t2, 30b

lla t0, {dynamic_snapshot}
.if {word_size} == 8
ld t1, 0 * {word_size}(t0)
bne t1, s3, 90f
ld t1, 1 * {word_size}(t0)
bne t1, s4, 90f
ld t1, 2 * {word_size}(t0)
bne t1, s5, 90f
ld t1, 3 * {word_size}(t0)
bne t1, s6, 90f
ld t1, 4 * {word_size}(t0)
bne t1, s7, 90f
ld t1, 5 * {word_size}(t0)
bne t1, s8, 90f
.else
lw t1, 0 * {word_size}(t0)
bne t1, s3, 90f
lw t1, 1 * {word_size}(t0)
bne t1, s4, 90f
lw t1, 2 * {word_size}(t0)
bne t1, s5, 90f
lw t1, 3 * {word_size}(t0)
bne t1, s6, 90f
lw t1, 4 * {word_size}(t0)
bne t1, s7, 90f
lw t1, 5 * {word_size}(t0)
bne t1, s8, 90f
.endif

40:
lla t0, {runtime_state}
amoadd.w.aq t1, zero, (t0)
beqz t1, 40b
li t2, {runtime_ready}
bne t1, t2, 90f
mv a0, s0
call {hart_entry_index}
li t0, -1
beq a0, t0, 90f
mv s1, a0
lla t0, {hart_stacks}
li t1, {hart_stack_stride}
li t2, 0
mv t3, s1
41:
beqz t3, 42f
add t2, t2, t1
addi t3, t3, -1
j 41b
42:
add sp, t0, t2
add sp, sp, t1
andi sp, sp, -16
mv a0, s0
mv a1, s1
tail {warm_entry}

90:
lla t0, {early_failed}
li t1, 1
amoswap.w.rl zero, t1, (t0)
91:
wfi
j 91b

.option pop

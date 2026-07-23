.option push
.option arch, +a

csrw mie, zero
li t0, {mprv}
csrc mstatus, t0
mv s0, a0
mv s1, a1
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
tail {cold_configured_entry}

3:
lla t0, {early_state}
30:
lla t3, {early_failed}
amoadd.w.aq t1, zero, (t3)
bnez t1, 90f
amoadd.w.aq t1, zero, (t0)
li t2, {ready}
bne t1, t2, 30b

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

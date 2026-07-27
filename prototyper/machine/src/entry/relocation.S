li t0, {link_start}
lla t1, sbi_start
sub t2, t1, t0
lla t0, __rel_dyn_start
lla t1, __rel_dyn_end
li t3, {relative}
1:
bgeu t0, t1, 2f
.if {word_size} == 8
ld t4, 8(t0)
bne t4, t3, 3f
ld t4, 0(t0)
ld t5, 16(t0)
add t4, t4, t2
add t5, t5, t2
sd t5, 0(t4)
.else
lw t4, 4(t0)
bne t4, t3, 3f
lw t4, 0(t0)
lw t5, 8(t0)
add t4, t4, t2
add t5, t5, t2
sw t5, 0(t4)
.endif
3:
.if {word_size} == 8
addi t0, t0, 24
.else
addi t0, t0, 12
.endif
j 1b
2:
fence.i
ret

.syntax unified
.arch armv5te
.thumb
.text
#ifdef DARK_SLAYER
.arm
 add ip,pc,#1
 bx ip
.thumb
#endif
.global epic_public_getter
.type epic_public_getter,%function
.thumb_func
epic_public_getter:
 push {r4,r5,r6,r7,lr}
 sub sp,#20
#ifdef EPIC_TWO
 movs r4,r2
 ldr r5,[r1,#24]
 movs r0,#0
 str r0,[r4,#0]
 str r0,[r4,#4]
 str r0,[r4,#8]
 str r0,[r4,#12]
 str r0,[r4,#16]
#ifdef DARK_SLAYER
 nop
 nop
#else
 str r0,[r4,#20]
 str r0,[r4,#24]
#endif
#else
 ldr r4,[sp,#48]
 movs r5,r1
 .rept 8
 nop
 .endr
#endif
 movs r6,#0
 movs r7,#0
 str r6,[sp,#0]
 str r6,[sp,#4]
 movs r0,r5
 mov r1,sp
 ldr r3,[r0,#0]
 ldr r3,[r3,#64]
 blx r3
 str r0,[sp,#8]
 cmp r0,#0
 bne cleanup
 ldr r6,[sp,#0]
 cmp r6,#0
 beq unsupported
 movs r0,r6
 ldr r1,idib_iid
 add r2,sp,#4
 ldr r3,[r0,#0]
 ldr r3,[r3,#8]
 blx r3
 str r0,[sp,#8]
 cmp r0,#0
 bne cleanup
 ldr r7,[sp,#4]
 cmp r7,#0
 beq unsupported
 ldrb r0,[r7,#28]
 cmp r0,#16
 bne unsupported
 ldrb r0,[r7,#29]
 cmp r0,#16
 bne unsupported
 ldr r0,[r7,#8]
 cmp r0,#0
 beq unsupported
 str r0,[r4,#0]
 ldr r0,[r7,#20]
 str r0,[r4,#4]
 ldrh r0,[r7,#24]
 strh r0,[r4,#8]
 ldr r0,format_565
 strh r0,[r4,#10]
 movs r0,#0
 str r0,[sp,#8]
 b cleanup
unsupported:
 movs r0,#20
 str r0,[sp,#8]
cleanup:
 cmp r7,#0
 beq release_bitmap
 movs r0,r7
 ldr r3,[r0,#0]
 ldr r3,[r3,#4]
 blx r3
release_bitmap:
 cmp r6,#0
 beq done
 movs r0,r6
 ldr r3,[r0,#0]
 ldr r3,[r3,#4]
 blx r3
done:
 ldr r0,[sp,#8]
 add sp,#20
 pop {r4,r5,r6,r7}
 pop {r3}
 bx r3
 .p2align 2
idib_iid: .word 0x01001045
format_565: .word 0x4110
.size epic_public_getter,.-epic_public_getter

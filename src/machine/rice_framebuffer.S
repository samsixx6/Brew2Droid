.syntax unified
.arch armv5te
.text
.arm
 add ip,pc,#1
 bx ip
.thumb
.global rice_public_framebuffer
.type rice_public_framebuffer,%function
.thumb_func
rice_public_framebuffer:
 push {r4,r5,r6,r7,lr}
 sub sp,#20
 ldr r3,applet_getter
 blx r3
 movs r4,r0
 ldr r5,[r4,#40]
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
 ldr r1,raster_offset
 adds r1,r4,r1
 str r0,[r1,#8]
 ldrh r2,[r7,#24]
 adds r2,r0,r2
 str r2,[r1,#12]
 movs r0,#3
 str r0,[r1,#0]
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
applet_getter: .word 0x1dd70
raster_offset: .word 0x260
.size rice_public_framebuffer,.-rice_public_framebuffer

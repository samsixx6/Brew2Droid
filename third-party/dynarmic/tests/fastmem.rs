mod a32 {
    use dynarmic::a32::{Callbacks, VAddr};
    use dynarmic::{CallbackImpl, DynarmicA32, GuestInt};
    use std::cmp::Ordering;
    struct ArmFastmemTestEnv {
        ticks_left: u64,
        backing_memory: *mut u8,
    }

    impl ArmFastmemTestEnv {
        fn new(backing_memory: *mut u8) -> Self {
            Self {
                ticks_left: 0,
                backing_memory,
            }
        }
    }

    impl Callbacks for ArmFastmemTestEnv {
        extern "C" fn memory_read<T>(cb: &CallbackImpl<Self>, addr: VAddr) -> T {
            unsafe {
                cb.backing_memory
                    .wrapping_add(addr as usize)
                    .cast::<T>()
                    .read()
            }
        }

        extern "C" fn memory_write<T>(cb: &mut CallbackImpl<Self>, addr: VAddr, val: T) {
            unsafe {
                cb.backing_memory
                    .wrapping_add(addr as usize)
                    .cast::<T>()
                    .copy_from_nonoverlapping(&val, 1);
            }
        }

        extern "C" fn memory_write_exclusive<T: GuestInt>(
            cb: &mut CallbackImpl<Self>,
            addr: VAddr,
            val: T,
            _expected: T,
        ) -> bool {
            Self::memory_write(cb, addr, val);
            true
        }

        extern "C" fn call_svc(_cb: &mut CallbackImpl<Self>, _swi: u32) {
            unimplemented!()
        }

        extern "C" fn add_ticks(cb: &mut CallbackImpl<Self>, ticks: u64) {
            if ticks > cb.ticks_left {
                cb.ticks_left = 0;
                return;
            }
            cb.ticks_left -= ticks;
        }

        extern "C" fn get_ticks_remaining(cb: &CallbackImpl<Self>) -> u64 {
            cb.ticks_left
        }
    }

    #[test]
    fn a32_fastmem() {
        unsafe {
            static address_width: usize = 12;
            static memory_size: usize = 1 << address_width; // 4K
            static page_size: usize = 4 * 1024;
            static buffer_size: usize = 2 * page_size; // buffer size is 2x for alignment?
            let mut buffer = vec![0u8; buffer_size];

            let mut ptr = buffer.as_mut_ptr();
            ptr = ptr.add(ptr.align_offset(page_size));

            let mut env = ArmFastmemTestEnv::new(ptr);
            env.ticks_left = 3;

            let mut jit = DynarmicA32::new_config()
                .fastmem(ptr.cast(), false)
                .processor_id(0)
                .init(env);

            std::ptr::copy_nonoverlapping(
                std::ffi::CString::new("Lorem ipsum dolor sit amet, consectetur adipiscing elit.")
                    .unwrap()
                    .as_ptr()
                    .cast(),
                ptr.add(0x100),
                57,
            );

            ArmFastmemTestEnv::memory_write(&mut jit, 0, 0xE5904000u32); // LDR R4, [R0]
            ArmFastmemTestEnv::memory_write(&mut jit, 4, 0xE5814000u32); // STR R4, [R1]
            ArmFastmemTestEnv::memory_write(&mut jit, 8, 0xEAFFFFFEu32); // B .
            jit.set_reg(0, 0x100);
            jit.set_reg(1, 0x1F0);

            jit.set_pc(0);
            jit.set_cpsr(0x000001d0); // User-mode

            jit.run();

            assert_eq!(
                std::slice::from_raw_parts(ptr.add(0x100), 4)
                    .cmp(std::slice::from_raw_parts(ptr.add(0x1F0), 4)),
                Ordering::Equal
            );
        }
    }
}
mod a64 {
    use dynarmic::a64::{Callbacks, VAddr};
    use dynarmic::{CallbackImpl, DynarmicA64, GuestInt};
    use std::cmp::Ordering;

    #[repr(C)]
    struct A64FastmemTestEnv {
        ticks_left: u64,
        backing_memory: *mut u8,
    }

    impl A64FastmemTestEnv {
        fn new(backing_memory: *mut u8) -> Self {
            Self {
                ticks_left: 0,
                backing_memory,
            }
        }
    }

    impl Callbacks for A64FastmemTestEnv {
        extern "C" fn memory_read<T>(cb: &CallbackImpl<Self>, vaddr: VAddr) -> T {
            unsafe {
                cb.backing_memory
                    .wrapping_add(vaddr as usize)
                    .cast::<T>()
                    .read()
            }
        }
        extern "C" fn memory_write<T>(cb: &mut CallbackImpl<Self>, vaddr: VAddr, val: T) {
            unsafe {
                cb.backing_memory
                    .wrapping_add(vaddr as usize)
                    .cast::<T>()
                    .copy_from_nonoverlapping(&val, 1);
            }
        }
        extern "C" fn memory_write_exclusive<T: GuestInt>(
            cb: &mut CallbackImpl<Self>,
            addr: VAddr,
            val: T,
            _expected: T,
        ) -> bool {
            Self::memory_write(cb, addr, val);
            true
        }

        extern "C" fn call_svc(_cb: &mut CallbackImpl<Self>, _swi: u32) {
            unimplemented!()
        }

        extern "C" fn add_ticks(cb: &mut CallbackImpl<Self>, ticks: u64) {
            if ticks > cb.ticks_left {
                cb.ticks_left = 0;
                return;
            }
            cb.ticks_left -= ticks;
        }
        extern "C" fn get_ticks_remaining(cb: &CallbackImpl<Self>) -> u64 {
            cb.ticks_left
        }
        extern "C" fn get_cntpct(cb: &CallbackImpl<Self>) -> u64 {
            0x10000000000 - cb.ticks_left
        }
    }

    #[test]
    fn a64_fastmem() {
        unsafe {
            static address_width: usize = 12;
            static memory_size: usize = 1 << address_width; // 4K
            static page_size: usize = 4 * 1024;
            static buffer_size: usize = 2 * page_size; // buffer size is 2x for alignment?
            let mut buffer = vec![0u8; buffer_size];

            let mut ptr = buffer.as_mut_ptr();
            ptr = ptr.add(ptr.align_offset(page_size));

            let mut env = A64FastmemTestEnv::new(ptr);
            env.ticks_left = 5;
            let mut jit = DynarmicA64::new_config()
                .fastmem(ptr as *mut _, address_width, false, true)
                .processor_id(0)
                .init(env);
            
            std::ptr::copy_nonoverlapping(
                std::ffi::CString::new("Lorem ipsum dolor sit amet, consectetur adipiscing elit.")
                    .unwrap()
                    .as_ptr()
                    .cast(),
                ptr.add(0x100),
                57,
            );

            A64FastmemTestEnv::memory_write(&mut jit, 0, 0xA9401404u32); // LDP x4, x5, [x0]
            A64FastmemTestEnv::memory_write(&mut jit, 4, 0xF9400046u32); // LDR x6, [x2]
            A64FastmemTestEnv::memory_write(&mut jit, 8, 0xA9001424u32); // STP x4, x5, [x1]
            A64FastmemTestEnv::memory_write(&mut jit, 12, 0xF9000066u32); // STR x6, [x3]
            A64FastmemTestEnv::memory_write(&mut jit, 16, 0x14000000u32); // B .
            jit.set_reg(0, 0x100);
            jit.set_reg(1, 0x1F0);
            jit.set_reg(2, 0x10F);
            jit.set_reg(3, 0x1FF);

            jit.set_pc(0);
            jit.set_sp(memory_size as u64 - 1u64);
            jit.set_fpcr(0x03480000);
            jit.set_pstate(0x30000000);

            jit.run();

            assert_eq!(
                std::slice::from_raw_parts(ptr.add(0x100), 23)
                    .cmp(std::slice::from_raw_parts(ptr.add(0x1F0), 23)),
                Ordering::Equal
            );
        }
    }
}
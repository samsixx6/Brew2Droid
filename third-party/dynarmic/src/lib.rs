pub mod a32;
pub mod a64;
pub(crate) mod cxx;

pub type DynarmicA32<T> = a32::Dynarmic<T>;
pub type DynarmicA64<'a, T> = a64::Dynarmic<'a, T>;

use std::mem::MaybeUninit;
use std::ops::{Deref, DerefMut};

/// Integer type used for memory reads and writes.
pub trait GuestInt: num_traits::PrimInt + num_traits::Unsigned {}
impl<T> GuestInt for T where T: num_traits::PrimInt + num_traits::Unsigned {}

/// Wrapper struct for an empty type implementing dynarmic's `Callbacks`.
///
/// Dereferences to `T`.
#[repr(C)]
pub struct CallbackImpl<T> {
    pub(crate) vtable: *const (),
    pub(crate) ptr: *mut T,
}

impl<T> CallbackImpl<T> {
    const _PTR_ASSERT: () = {
        assert!(std::mem::offset_of!(CallbackImpl<T>, vtable) == 0);
    };
}

impl<T> Deref for CallbackImpl<T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.ptr }
    }
}

impl<T> DerefMut for CallbackImpl<T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.ptr }
    }
}

#[repr(transparent)]
pub struct OptimizationFlag(u32);
impl OptimizationFlag {
    /// This optimization avoids dispatcher lookups by allowing emitted basic blocks to jump
    /// directly to other basic blocks if the destination PC is predictable at JIT-time.
    /// This is a safe optimization.
    pub const BLOCK_LINKING: Self = Self(1);

    /// This optimization avoids dispatcher lookups by emulating a return stack buffer. This
    /// allows for function returns and syscall returns to be predicted at runtime.
    /// This is a safe optimization.
    pub const RETURN_STACK_BUFFER: Self = Self(2);

    /// This optimization enables a two-tiered dispatch system.
    /// A fast dispatcher (written in assembly) first does a look-up in a small MRU cache.
    /// If this fails, it falls back to the usual slower dispatcher.
    /// This is a safe optimization.
    pub const FAST_DISPATCH: Self = Self(4);

    /// This is an IR optimization. This optimization eliminates unnecessary emulated CPU state
    /// context lookups.
    /// This is a safe optimization.
    pub const GET_SET_ELIMINATION: Self = Self(8);

    /// This is an IR optimization. This optimization does constant propagation.
    /// This is a safe optimization.
    pub const CONST_PROP: Self = Self(16);

    /// This is enables miscellaneous safe IR optimizations.
    pub const MISC_IR_OPT: Self = Self(32);

    /// This is an UNSAFE optimization that reduces accuracy of fused multiply-add operations.
    /// This unfuses fused instructions to improve performance on host CPUs without FMA support.
    pub const UNSAFE_UNFUSE_FMA: Self = Self(65536);

    /// This is an UNSAFE optimization that reduces accuracy of certain floating-point instructions.
    /// This allows results of FRECPE and FRSQRTE to have **less** error than spec allows.
    pub const UNSAFE_REDUCED_ERROR_FP: Self = Self(131072);

    /// This is an UNSAFE optimization that causes floating-point instructions to not produce correct NaNs.
    /// This may also result in inaccurate results when instructions are given certain special values.
    pub const UNSAFE_INACCURATENAN: Self = Self(262144);

    /// This is an UNSAFE optimization that causes ASIMD floating-point instructions to be run with incorrect
    /// rounding modes. This may result in inaccurate results with all floating-point ASIMD instructions.
    pub const UNSAFE_IGNORE_STANDARD_FPCR_VALUE: Self = Self(524288);

    /// This is an UNSAFE optimization that causes the global monitor to be ignored. This may
    /// result in unexpected behaviour in multithreaded scenarios, including but not limited
    /// to data races and deadlocks.
    pub const UNSAFE_IGNORE_GLOBALMONITOR: Self = Self(1048576);

    /// All safe optimizations.
    /// The default used by dynarmic.
    pub const ALL: Self = Self(0x0000FFFF);
    pub const NONE: Self = Self(0);
}

#[repr(u32)]
#[derive(Debug)]
pub enum HaltReason {
    Step = 1,
    CacheInvalidation = 2,
    MemoryAbort = 4,
    UserDefined1 = 16777216,
    UserDefined2 = 33554432,
    UserDefined3 = 67108864,
    UserDefined4 = 134217728,
    UserDefined5 = 268435456,
    UserDefined6 = 536870912,
    UserDefined7 = 1073741824,
    UserDefined8 = 2147483648,
}

#[repr(transparent)]
struct Spinlock {
    storage: i32,
}

#[repr(C)]
pub struct ExclusiveMonitor {
    spinlock: Spinlock,
    exclusive_addr: cxx::CxxVector<a64::VAddr>,
    exclusive_val: cxx::CxxVector<u128>,
}

impl ExclusiveMonitor {
    #[inline(always)]
    pub fn new(processor_count: usize) -> Self {
        unsafe extern "C-unwind" {
            fn ExclusiveMonitor_ExclusiveMonitor(
                this: *mut ExclusiveMonitor,
                processor_count: usize,
            );
        }
        let mut init = MaybeUninit::<Self>::uninit();
        unsafe {
            ExclusiveMonitor_ExclusiveMonitor(init.as_mut_ptr(), processor_count);
            init.assume_init()
        }
    }

    #[inline]
    pub fn clear_processor(&mut self, id: usize) {
        unsafe {
            *self.get_addr_ptr(id) = 0xDEADDEADDEADDEAD; // INVALID_EXCLUSIVE_ADDRESS
        }
    }
    pub fn clear(&mut self) {
        for i in 0..self.get_processor_count() {
            self.clear_processor(i)
        }
    }

    #[inline(always)]
    pub fn get_processor_count(&self) -> usize {
        unsafe extern "C-unwind" {
            fn size_vec_u64(vec: *const cxx::CxxVector<u64>) -> usize;
        }
        unsafe { size_vec_u64(&self.exclusive_addr) }
    }

    #[inline(always)]
    fn get_addr_ptr(&mut self, proc: usize) -> *mut u64 {
        debug_assert!(proc < self.get_processor_count());
        unsafe extern "C-unwind" {
            fn get_vec_u64(vec: *mut cxx::CxxVector<u64>, index: usize) -> *mut u64;
        }
        unsafe { get_vec_u64(&mut self.exclusive_addr, proc) }
    }
    #[inline(always)]
    fn get_value_ptr(&mut self, proc: usize) -> *mut u128 {
        debug_assert!(proc < self.get_processor_count());
        unsafe extern "C-unwind" {
            fn get_vec_u128(vec: *mut cxx::CxxVector<u64>, index: usize) -> *mut u128;
        }
        unsafe { get_vec_u128(&mut self.exclusive_addr, proc) }
    }
    #[inline(always)]
    fn lock(&mut self) {
        unsafe extern "C-unwind" {
            pub fn SpinLock_Lock(this: *mut Spinlock);
        }
        unsafe { SpinLock_Lock(&mut self.spinlock) }
    }
    #[inline(always)]
    fn unlock(&mut self) {
        unsafe extern "C-unwind" {
            pub fn SpinLock_Unlock(this: *mut Spinlock);
        }
        unsafe { SpinLock_Unlock(&mut self.spinlock) }
    }

    /// Marks a region containing [`address`, `address`+size) to be exclusive to
    /// processor `proc_id`.
    pub fn read_and_mark<T: GuestInt, F>(&mut self, proc_id: usize, addr: a64::VAddr, op: F)
    where
        F: Fn() -> T,
    {
        self.lock();

        let val = op();
        // SAFETY: pointer validity should be ensured by C++
        unsafe {
            *self.get_addr_ptr(proc_id) = addr;
            // note that we use copy here as the original code specifically chooses
            // not to zero out the other bytes and i'm not really sure if that's on purpose
            // SAFETY: .cast() is safe as T can't be bigger than u128
            std::ptr::copy_nonoverlapping(&val, self.get_value_ptr(proc_id).cast(), 1);
        }
        self.unlock();
    }

    /// Checks to see if processor `proc_id` has exclusive access to the
    /// specified region. If it does, executes the operation then clears
    /// the exclusive state for processors if their exclusive region(s)
    /// contain [`addr`, `addr`+size).
    pub fn do_exclusive_op<T: GuestInt, F>(
        &mut self,
        proc_id: usize,
        addr: a64::VAddr,
        op: F,
    ) -> bool
    where
        F: Fn(T) -> bool,
    {
        // CheckAndClear (private function)
        self.lock();
        if unsafe { *self.get_addr_ptr(proc_id) } != addr {
            self.unlock();
            return false;
        }

        for i in 0..self.get_processor_count() {
            let val = self.get_addr_ptr(i);
            unsafe {
                if *val == addr {
                    *val = 0xDEADDEADDEADDEAD; // INVALID_EXCLUSIVE_ADDRESS
                }
            }
        }

        // DoExclusiveOperation
        let saved_value = unsafe { *self.get_value_ptr(proc_id).cast::<T>() };
        let result = op(saved_value);

        self.unlock();
        result
    }
}

impl Drop for ExclusiveMonitor {
    fn drop(&mut self) {
        unsafe extern "C-unwind" {
            pub fn delete_vec_u64(vec: *mut cxx::CxxVector<u64>);
            pub fn delete_vec_u128(vec: *mut cxx::CxxVector<u128>);
        }
        unsafe {
            delete_vec_u64(&mut self.exclusive_addr);
            delete_vec_u128(&mut self.exclusive_val);
        }
    }
}

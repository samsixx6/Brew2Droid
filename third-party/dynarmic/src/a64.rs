use crate::{CallbackImpl, GuestInt, OptimizationFlag};
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
pub type VAddr = u64;

// Internal VTable for Callbacks
#[repr(C)]
struct DynarmicCallbacks<T> {
    #[cfg(not(target_env = "msvc"))]
    offset_to_top: isize, // our inheritence is fake, we're essentially making UserCallbacks a final class, so this should always be 0 as UserCallbacks has no fields
    #[cfg(not(target_env = "msvc"))]
    typeinfo: crate::cxx::TypeInfoPtr,

    // these functions should never be called; UserCallbacks should always be owned by Rust
    cpp_destructor: extern "C" fn(),
    #[cfg(not(target_env = "msvc"))]
    itanium_destructor: extern "C" fn(),

    // https://github.com/rust-lang/rust/issues/38258
    #[cfg(not(target_env = "msvc"))]
    memory_read_code:
        unsafe extern "C" fn(&CallbackImpl<T>, VAddr) -> crate::cxx::CxxOptional<u32>,
    #[cfg(target_env = "msvc")]
    memory_read_code:
        unsafe extern "C" fn(&CallbackImpl<T>, *mut crate::cxx::CxxOptional<u32>, VAddr),

    memory_read_8: extern "C" fn(&CallbackImpl<T>, VAddr) -> u8,
    memory_read_16: extern "C" fn(&CallbackImpl<T>, VAddr) -> u16,
    memory_read_32: extern "C" fn(&CallbackImpl<T>, VAddr) -> u32,
    memory_read_64: extern "C" fn(&CallbackImpl<T>, VAddr) -> u64,
    memory_read_128: extern "C" fn(&CallbackImpl<T>, VAddr) -> u128,
    memory_write_8: extern "C" fn(&mut CallbackImpl<T>, VAddr, u8),
    memory_write_16: extern "C" fn(&mut CallbackImpl<T>, VAddr, u16),
    memory_write_32: extern "C" fn(&mut CallbackImpl<T>, VAddr, u32),
    memory_write_64: extern "C" fn(&mut CallbackImpl<T>, VAddr, u64),
    memory_write_128: extern "C" fn(&mut CallbackImpl<T>, VAddr, u128),
    memory_write_exclusive_8: extern "C" fn(&mut CallbackImpl<T>, VAddr, u8, u8) -> bool,
    memory_write_exclusive_16: extern "C" fn(&mut CallbackImpl<T>, VAddr, u16, u16) -> bool,
    memory_write_exclusive_32: extern "C" fn(&mut CallbackImpl<T>, VAddr, u32, u32) -> bool,
    memory_write_exclusive_64: extern "C" fn(&mut CallbackImpl<T>, VAddr, u64, u64) -> bool,
    memory_write_exclusive_128: extern "C" fn(&mut CallbackImpl<T>, VAddr, u128, u128) -> bool,

    is_readonly_memory: extern "C" fn(&CallbackImpl<T>, VAddr) -> bool,
    interpreter_fallback: extern "C" fn(&mut CallbackImpl<T>, VAddr, usize),
    call_svc: extern "C" fn(&mut CallbackImpl<T>, u32),
    exception_raised: extern "C" fn(&mut CallbackImpl<T>, VAddr, Exception),
    data_cache_operation_raised: extern "C" fn(&mut CallbackImpl<T>, DataCacheOperation, VAddr),
    instruction_cache_operation_raised:
        extern "C" fn(&mut CallbackImpl<T>, InstructionCacheOperation, VAddr),
    instruction_synchronization_barrier_raised: extern "C" fn(&mut CallbackImpl<T>),
    add_ticks: extern "C" fn(&mut CallbackImpl<T>, u64),
    get_ticks_remaining: extern "C" fn(&CallbackImpl<T>) -> u64,
    get_cntpct: extern "C" fn(&CallbackImpl<T>) -> u64,
}

#[repr(C)]
pub(crate) struct DynarmicConfig<T> {
    callbacks: *mut CallbackImpl<T>,
    processor_id: usize,
    global_monitor: *mut crate::ExclusiveMonitor,
    optimizations: OptimizationFlag,
    unsafe_optimizations: bool,
    hook_data_cache_operations: bool,
    hook_isb: bool,
    hook_hint_instructions: bool,
    cntfrq_el0: u32,
    ctr_el0: u32,
    dczid_el0: u32,
    tpidrro_el0: *mut std::ffi::c_void,
    tpidr_el0: *mut std::ffi::c_void,
    page_table: *mut *mut std::ffi::c_void,
    page_table_address_space_bits: usize,
    page_table_pointer_mask_bits: std::os::raw::c_int,
    silently_mirror_page_table: bool,
    absolute_offset_page_table: bool,
    detect_misaligned_access_via_page_table: u8,
    only_detect_misalignment_via_page_table_on_page_boundary: bool,
    fastmem_pointer: crate::cxx::CxxOptional<usize>,
    recompile_on_fastmem_failure: bool,
    fastmem_address_space_bits: usize,
    silently_mirror_fastmem: bool,
    fastmem_exclusive_access: bool,
    recompile_on_exclusive_fastmem_failure: bool,
    define_unpredictable_behaviour: bool,
    wall_clock_cntpct: bool,
    check_halt_on_memory_access: bool,
    enable_cycle_counting: bool,
    code_cache_size: usize,
    very_verbose_debugging_output: bool,
}

#[repr(i32)]
#[derive(Debug)]
pub enum Exception {
    /// An UndefinedFault occured due to executing instruction with an unallocated encoding
    UnallocatedEncoding = 0,
    /// An UndefinedFault occured due to executing instruction containing a reserved value
    ReservedValue = 1,
    /// An unpredictable instruction is to be executed. Implementation-defined behaviour should now happen.
    /// This behaviour is up to the user of this library to define.
    /// Note: Constraints on unpredictable behaviour are specified in the ARMv8 ARM.
    UnpredictableInstruction = 2,
    /// A WFI instruction was executed. You may now enter a low-power state. (Hint instruction.)
    WaitForInterrupt = 3,
    /// A WFE instruction was executed. You may now enter a low-power state if the event register is clear. (Hint instruction.)
    WaitForEvent = 4,
    /// A SEV instruction was executed. The event register of all PEs should be set. (Hint instruction.)
    SendEvent = 5,
    /// A SEVL instruction was executed. The event register of the current PE should be set. (Hint instruction.)
    SendEventLocal = 6,
    /// A YIELD instruction was executed. (Hint instruction.)
    Yield = 7,
    /// A BRK instruction was executed. (Hint instruction.)
    Breakpoint = 8,
    /// Attempted to execute a code block at an address for which MemoryReadCode returned std::nullopt.
    /// (Intended to be used to emulate memory protection faults.)
    NoExecuteFault = 9,
}

#[repr(i32)]
#[derive(Debug)]
pub enum DataCacheOperation {
    /// DC CISW
    CleanAndInvalidateBySetWay = 0,
    /// DC CIVAC
    CleanAndInvalidateByVAToPoC = 1,
    /// DC CSW
    CleanBySetWay = 2,
    /// DC CVAC
    CleanByVAToPoC = 3,
    /// DC CVAU
    CleanByVAToPoU = 4,
    /// DC CVAP
    CleanByVAToPoP = 5,
    /// DC ISW
    InvalidateBySetWay = 6,
    /// DC IVAC
    InvalidateByVAToPoC = 7,
    /// DC ZVA
    ZeroByVA = 8,
}

#[repr(i32)]
#[derive(Debug)]
pub enum InstructionCacheOperation {
    /// IC IVAU
    InvalidateByVAToPoU = 0,
    /// IC IALLU
    InvalidateAllToPoU = 1,
    /// IC IALLUIS
    InvalidateAllToPoUInnerSharable = 2,
}

/// Callback functions that dynarmic will use to access memory and
/// when the code calls for a higher exception level (e.g. SVC calls)
#[allow(unused_variables)]
pub trait Callbacks: Sized {
    /// Called when reading code memory at `pc` to recompile.
    /// 
    /// # Notes
    /// - All reads through this callback are 4-byte aligned.
    /// - Memory must be interpreted as little endian.
    ///
    /// The default implementation is to call [memory_read::\<u32\>](Self::memory_read).
    fn memory_read_code(cb: &CallbackImpl<Self>, addr: VAddr) -> Option<u32> {
        Some(Self::memory_read(cb, addr))
    }

    /// This function is called when an emulated memory read operation is called on a virtual address.
    /// 
    /// It will only be called once for any address if [is_readonly_memory](Self::is_readonly_memory) returns false,
    /// and the value may be cached by dynarmic.
    extern "C" fn memory_read<T: GuestInt>(cb: &CallbackImpl<Self>, addr: VAddr) -> T;
    /// This function is called when an emulated memory write operation on a virtual address is called.
    /// 
    /// It will not be called if [is_readonly_memory](Self::is_readonly_memory) returns false for this address.
    extern "C" fn memory_write<T: GuestInt>(cb: &mut CallbackImpl<Self>, addr: VAddr, val: T);
    /// This function is called when an emulated exclusive memory write operation is called. (corresponds to `STXR`, `STLXR`, etc.)
    ///
    /// The default implementation will do no write and always return false.
    extern "C" fn memory_write_exclusive<T: GuestInt>(
        cb: &mut CallbackImpl<Self>,
        addr: VAddr,
        val: T,
        expected: T,
    ) -> bool {
        false
    }
    /// This function is called when accessing a virtual memory address for the first time.
    ///
    /// If this callback returns true, the Jit will assume [memory_read](Self::memory_read) callbacks will always
    /// return the same value at any point in time for this vaddr. The Jit may use this information
    /// in optimizations.
    /// 
    /// The default implementation will always return false.
    extern "C" fn is_readonly_memory(cb: &CallbackImpl<Self>, addr: VAddr) -> bool {
        false
    }
    /// This function is called when dynarmic doesn't have an implementation for the instruction at `pc` and `num` instructions after.
    /// 
    /// The default implementation will panic.
    extern "C" fn interpreter_fallback(cb: &mut CallbackImpl<Self>, pc: VAddr, num: usize) {
        panic!(
            "dynarmic: Unhandled instruction '0x{:X}' for '{}' instructions at '0x{:X}'",
            Self::memory_read::<u32>(cb, pc),
            num,
            pc
        )
    }
    /// This function is called when a dynarmic exception is called.
    /// 
    /// # Notes
    /// - See [Exception] for the different types of potential exceptions.
    /// - Hint instructions will only be called for this function if `isb` was enabled
    ///   in [Config::enable_hooks].
    extern "C" fn raised_exception(cb: &mut CallbackImpl<Self>, addr: VAddr, exc: Exception) {
        panic!("dynarmic: Unhandled exception '{exc:?}' at '0x{addr:X}'",)
    }
    extern "C" fn call_svc(cb: &mut CallbackImpl<Self>, swi: u32);
    extern "C" fn data_cache_operation_raised(
        _cb: &mut CallbackImpl<Self>,
        _op: DataCacheOperation,
        _addr: VAddr,
    ) {
    }
    extern "C" fn instruction_cache_operation_raised(
        _cb: &mut CallbackImpl<Self>,
        _op: InstructionCacheOperation,
        _addr: VAddr,
    ) {
    }
    extern "C" fn instruction_synchronization_barrier_raised(_cb: &mut CallbackImpl<Self>) {}
    /// This function is called by the JIT to determine that `tick` ticks have passed and
    /// should be added to the tick counter.
    /// 
    /// # Notes
    /// - This function is only used if [Config::cycle_counting](Config::cycle_counting) was set to true.
    /// - When the tick counter reaches 0, the guest will return.
    extern "C" fn add_ticks(cb: &mut CallbackImpl<Self>, tick: u64);
    /// Returns the remaining amount of ticks before the program stops.
    ///
    /// # Notes
    /// - This function is only used if [Config::cycle_counting](Config::cycle_counting) was set to true.
    /// - When this function returns 0, or when dynarmic estimates that the program is over based on a past call
    ///   to this function, the guest will return.
    extern "C" fn get_ticks_remaining(cb: &CallbackImpl<Self>) -> u64;
    /// Get value in the emulated counter-timer physical count register.
    extern "C" fn get_cntpct(cb: &CallbackImpl<Self>) -> u64;
}

pub type Jit = u64;

/// A Rust-safe wrapper of Dynarmic's A64 Jit.
/// This type can be constructed with [Config].
#[allow(dead_code)]
pub struct Dynarmic<'a, T: Callbacks> {
    ptr: *mut Jit,
    cpp_cb: Box<CallbackImpl<T>>,
    rust_cb: Box<T>,

    _lifetime: PhantomData<&'a ()>, // lifetime for TPIDR_EL0 and TPIDRRO_EL0
}

impl<'a, T: Callbacks> Drop for Dynarmic<'a, T> {
    fn drop(&mut self) {
        unsafe extern "C-unwind" {
            pub fn delete_a64_jit(ptr: *mut Jit);
        }
        unsafe { delete_a64_jit(self.ptr) }
    }
}

impl<'a, T: Callbacks> Dynarmic<'a, T> {
    #[cfg(not(target_env = "msvc"))]
    unsafe extern "C" fn memory_read_code_impl(
        cb: &CallbackImpl<T>,
        addr: VAddr,
    ) -> crate::cxx::CxxOptional<u32> {
        T::memory_read_code(cb, addr).unwrap_or(0).into()
    }
    #[cfg(target_env = "msvc")]
    unsafe extern "C" fn memory_read_code_impl(
        cb: &CallbackImpl<T>,
        out: *mut crate::cxx::CxxOptional<u32>,
        addr: VAddr,
    ) {
        unsafe {
            *out = T::memory_read_code(cb, addr).unwrap_or(0).into();
        }
    }

    const CALLBACKS: DynarmicCallbacks<T> = DynarmicCallbacks {
        #[cfg(not(target_env = "msvc"))]
        offset_to_top: 0,
        #[cfg(not(target_env = "msvc"))]
        typeinfo: unsafe { std::mem::zeroed() },
        memory_read_code: Self::memory_read_code_impl,
        cpp_destructor: crate::cxx::unimplemented_destructor,
        #[cfg(not(target_env = "msvc"))]
        itanium_destructor: crate::cxx::unimplemented_destructor,
        memory_read_8: T::memory_read::<u8>,
        memory_read_16: T::memory_read::<u16>,
        memory_read_32: T::memory_read::<u32>,
        memory_read_64: T::memory_read::<u64>,
        memory_read_128: T::memory_read::<u128>,
        memory_write_8: T::memory_write::<u8>,
        memory_write_16: T::memory_write::<u16>,
        memory_write_32: T::memory_write::<u32>,
        memory_write_64: T::memory_write::<u64>,
        memory_write_128: T::memory_write::<u128>,
        memory_write_exclusive_8: T::memory_write_exclusive::<u8>,
        memory_write_exclusive_16: T::memory_write_exclusive::<u16>,
        memory_write_exclusive_32: T::memory_write_exclusive::<u32>,
        memory_write_exclusive_64: T::memory_write_exclusive::<u64>,
        memory_write_exclusive_128: T::memory_write_exclusive::<u128>,
        is_readonly_memory: T::is_readonly_memory,
        interpreter_fallback: T::interpreter_fallback,
        call_svc: T::call_svc,
        exception_raised: T::raised_exception,
        data_cache_operation_raised: T::data_cache_operation_raised,
        instruction_cache_operation_raised: T::instruction_cache_operation_raised,
        instruction_synchronization_barrier_raised: T::instruction_synchronization_barrier_raised,
        add_ticks: T::add_ticks,
        get_ticks_remaining: T::get_ticks_remaining,
        get_cntpct: T::get_cntpct,
    };

    #[inline(always)]
    pub fn new_config() -> Config<'a, T> {
        Config::new()
    }

    /// Runs the emulated CPU.
    /// Cannot be recursively called.
    ///
    /// # Safety
    /// - All instructions and memory addresses inputted must be valid. Invalid addresses/instructions will cause dynarmic exceptions, which panic by default.
    /// -
    #[inline]
    pub unsafe fn run(&mut self) -> crate::HaltReason {
        unsafe extern "C-unwind" {
            pub fn JitA64_Run(this: *mut Jit) -> crate::HaltReason;
        }
        unsafe { JitA64_Run(self.ptr) }
    }

    /// Step the emulated CPU for one instruction.
    /// Cannot be recursively called.
    #[inline]
    pub fn step(&mut self) -> crate::HaltReason {
        unsafe extern "C-unwind" {
            pub fn JitA64_Step(this: *mut Jit) -> crate::HaltReason;
        }
        unsafe { JitA64_Step(self.ptr) }
    }

    /// Clears the code cache of all compiled code.
    /// Can be called at any time. Halts execution if called within a callback.
    #[inline]
    pub fn clear_cache(&mut self) {
        unsafe extern "C-unwind" {
            pub fn JitA64_ClearCache(this: *mut Jit);
        }
        unsafe { JitA64_ClearCache(self.ptr) }
    }

    /// Invalidate the code cache at a range of addresses.
    /// - `start_address` - The starting address of the range to invalidate.
    /// - `length` - The length (in bytes) of the range to invalidate.
    #[inline]
    pub fn invalidate_cache_range(&mut self, start_addr: VAddr, length: usize) {
        unsafe extern "C-unwind" {
            pub fn JitA64_InvalidateCacheRange(this: *mut Jit, start_address: u64, length: usize);
        }
        unsafe { JitA64_InvalidateCacheRange(self.ptr, start_addr, length) }
    }

    /// Reset CPU state to state at startup. Does not clear code cache.
    /// Cannot be called from a callback.
    #[inline]
    pub fn reset(&mut self) {
        unsafe extern "C-unwind" {
            pub fn JitA64_Reset(this: *mut Jit);
        }
        unsafe { JitA64_Reset(self.ptr) }
    }

    /// Stops execution during [Dynarmic::run].
    #[inline]
    pub fn halt(&mut self, hr: crate::HaltReason) {
        unsafe extern "C-unwind" {
            pub fn JitA64_HaltExecution(this: *mut Jit, hr: crate::HaltReason);
        }
        unsafe { JitA64_HaltExecution(self.ptr, hr) }
    }

    /// Clears a halt reason from flags.
    #[inline]
    pub fn clear_halt(&mut self, hr: crate::HaltReason) {
        unsafe extern "C-unwind" {
            pub fn JitA64_ClearHalt(this: *mut Jit, hr: crate::HaltReason);
        }
        unsafe { JitA64_ClearHalt(self.ptr, hr) }
    }

    /// Read Stack Pointer
    #[inline]
    pub fn get_sp(&self) -> u64 {
        unsafe extern "C-unwind" {
            pub fn JitA64_GetSP(this: *const Jit) -> u64;
        }
        unsafe { JitA64_GetSP(self.ptr) }
    }

    /// Modify Stack Pointer
    #[inline]
    pub fn set_sp(&mut self, sp: u64) {
        unsafe extern "C-unwind" {
            pub fn JitA64_SetSP(this: *mut Jit, value: u64);
        }
        unsafe { JitA64_SetSP(self.ptr, sp) }
    }

    /// Read Program Counter
    #[inline]
    pub fn get_pc(&self) -> u64 {
        unsafe extern "C-unwind" {
            pub fn JitA64_GetPC(this: *const Jit) -> u64;
        }
        unsafe { JitA64_GetPC(self.ptr) }
    }

    /// Modify Program Counter
    #[inline]
    pub fn set_pc(&mut self, pc: u64) {
        unsafe extern "C-unwind" {
            pub fn JitA64_SetPC(this: *mut Jit, value: u64);
        }
        unsafe { JitA64_SetPC(self.ptr, pc) }
    }

    /// Read general-purpose register. (GPR)
    #[inline]
    pub fn get_reg(&self, index: usize) -> u64 {
        unsafe extern "C-unwind" {
            pub fn JitA64_GetReg(this: *const Jit, index: usize) -> u64;
        }
        unsafe { JitA64_GetReg(self.ptr, index as _) }
    }

    /// Read the low 32-bits of a GPR.
    #[inline]
    pub fn get_wreg(&self, index: usize) -> u32 {
        self.get_reg(index) as u32
    }

    /// Modify general-purpose register. (GPR)
    #[inline]
    pub fn set_reg(&mut self, index: usize, val: u64) {
        unsafe extern "C-unwind" {
            pub fn JitA64_SetReg(this: *mut Jit, index: usize, value: u64);
        }
        unsafe { JitA64_SetReg(self.ptr, index as _, val) }
    }

    /// Read all general-purpose registers.
    #[inline]
    pub fn get_regs(&self) -> [u64; 31] {
        unsafe extern "C-unwind" {
            pub fn JitA64_GetRegs(this: *mut Jit, out: *mut [u64; 31]) -> *mut u8;
        }
        let mut regs = [0u64; 31];
        unsafe {
            JitA64_GetRegs(self.ptr, &mut regs as _);
        }
        regs
    }

    /// Replace all general-purpose registers.
    #[inline]
    pub fn set_regs(&mut self, regs: &[u64; 31]) {
        unsafe extern "C-unwind" {
            pub fn JitA64_SetRegs(this: *mut Jit, out: *const u8);
        }
        unsafe { JitA64_SetRegs(self.ptr, regs.as_ptr().cast()) }
    }

    /// Read floating point and SIMD register.
    #[inline]
    pub fn get_vector(&self, index: usize) -> u128 {
        unsafe extern "C-unwind" {
            pub fn JitA64_GetVector(this: *const Jit, out: *mut u128, index: usize);
        }
        let mut out: u128 = 0;
        unsafe { JitA64_GetVector(self.ptr, &mut out, index as _) }
        out
    }

    /// Modify floating point/SIMD register. (GPR)
    #[inline]
    pub fn set_vector(&mut self, index: usize, val: u128) {
        unsafe extern "C-unwind" {
            pub fn JitA64_SetVector(this: *mut Jit, index: usize, value: *const u128);
        }
        unsafe { JitA64_SetVector(self.ptr, index as _, &val) }
    }

    /// Read all floating point and SIMD registers.
    #[inline]
    pub fn get_vectors(&self) -> [u128; 32] {
        unsafe extern "C-unwind" {
            pub fn JitA64_GetVectors(this: *mut Jit, out: *mut [u128; 32]) -> *mut u8;
        }
        let mut regs = [0u128; 32];
        unsafe {
            JitA64_GetVectors(self.ptr, &mut regs as _);
        }
        regs
    }

    /// Replace all general-purpose registers.
    #[inline]
    pub fn set_vectors(&mut self, regs: &[u128; 32]) {
        unsafe extern "C-unwind" {
            pub fn JitA64_SetVectors(this: *mut Jit, value: *const u8);
        }
        unsafe { JitA64_SetVectors(self.ptr, regs.as_ptr().cast()) }
    }

    /// View FPCR
    #[inline]
    pub fn get_fpcr(&self) -> u32 {
        unsafe extern "C-unwind" {
            pub fn JitA64_GetFpcr(this: *const Jit) -> u32;
        }
        unsafe { JitA64_GetFpcr(self.ptr) }
    }

    /// Modify FPCR
    #[inline]
    pub fn set_fpcr(&mut self, val: u32) {
        unsafe extern "C-unwind" {
            pub fn JitA64_SetFpcr(this: *mut Jit, value: u32);
        }
        unsafe { JitA64_SetFpcr(self.ptr, val) }
    }

    /// View FPSR
    #[inline]
    pub fn get_fpsr(&self) -> u32 {
        unsafe extern "C-unwind" {
            pub fn JitA64_GetFpsr(this: *const Jit) -> u32;
        }
        unsafe { JitA64_GetFpsr(self.ptr) }
    }

    /// Modify FPSR
    #[inline]
    pub fn set_fpsr(&mut self, val: u32) {
        unsafe extern "C-unwind" {
            pub fn JitA64_SetFpsr(this: *mut Jit, value: u32);
        }
        unsafe { JitA64_SetFpsr(self.ptr, val) }
    }

    /// View PSTATE
    #[inline]
    pub fn get_pstate(&self) -> u32 {
        unsafe extern "C-unwind" {
            pub fn JitA64_GetPstate(this: *const Jit) -> u32;
        }
        unsafe { JitA64_GetPstate(self.ptr) }
    }

    /// Modify FPCR
    #[inline]
    pub fn set_pstate(&mut self, val: u32) {
        unsafe extern "C-unwind" {
            pub fn JitA64_SetPstate(this: *mut Jit, value: u32);
        }
        unsafe { JitA64_SetPstate(self.ptr, val) }
    }

    /// Clears exclusive states for this core.
    #[inline]
    pub fn clear_exclusive_state(&mut self) {
        unsafe extern "C-unwind" {
            pub fn JitA64_ClearExclusiveState(this: *mut Jit);
        }
        unsafe { JitA64_ClearExclusiveState(self.ptr) }
    }

    /// Returns true if Jit::Run was called but hasn't returned yet.
    /// i.e; we're in a callback
    #[inline]
    pub fn is_executing(&self) -> bool {
        unsafe extern "C-unwind" {
            pub fn JitA64_IsExecuting(this: *const Jit) -> bool;
        }
        unsafe { JitA64_IsExecuting(self.ptr) }
    }
}

impl<'a, T: Callbacks> Deref for Dynarmic<'a, T> {
    type Target = CallbackImpl<T>;

    fn deref(&self) -> &Self::Target {
        self.cpp_cb.as_ref()
    }
}

impl<'a, T: Callbacks> DerefMut for Dynarmic<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.cpp_cb.as_mut()
    }
}

pub struct Config<'a, T: Callbacks> {
    config: DynarmicConfig<T>,
    tpidr_el0: Option<&'a mut std::ffi::c_void>,
    tpidrro_el0: Option<&'a mut std::ffi::c_void>,
}

impl<'a, T: Callbacks> Default for Config<'a, T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a, T: Callbacks> Config<'a, T> {
    pub fn new() -> Self {
        Self {
            config: DynarmicConfig {
                callbacks: unsafe { std::mem::zeroed() },
                processor_id: 0,
                global_monitor: unsafe { std::mem::zeroed() }, // todo
                optimizations: OptimizationFlag::ALL,
                unsafe_optimizations: false,
                hook_data_cache_operations: false,
                hook_isb: false,
                hook_hint_instructions: false,
                cntfrq_el0: 600000000,
                ctr_el0: 0x8444c004,
                dczid_el0: 4,
                tpidrro_el0: std::ptr::null_mut(),
                tpidr_el0: std::ptr::null_mut(),
                page_table: std::ptr::null_mut(),
                page_table_address_space_bits: 36,
                page_table_pointer_mask_bits: 0,
                silently_mirror_page_table: true,
                absolute_offset_page_table: false,
                detect_misaligned_access_via_page_table: 8 | 16 | 32 | 64,
                only_detect_misalignment_via_page_table_on_page_boundary: true,
                fastmem_pointer: 0usize.into(),
                recompile_on_fastmem_failure: true,
                fastmem_address_space_bits: 36,
                silently_mirror_fastmem: true,
                fastmem_exclusive_access: false,
                recompile_on_exclusive_fastmem_failure: true,
                define_unpredictable_behaviour: false,
                wall_clock_cntpct: false,
                check_halt_on_memory_access: false,
                enable_cycle_counting: true,
                code_cache_size: 128 * 1024 * 1024,
                very_verbose_debugging_output: false,
            },
            tpidr_el0: None,
            tpidrro_el0: None,
        }
    }
    pub fn init(&mut self, cb: T) -> Dynarmic<'a, T> {
        unsafe extern "C-unwind" {
            pub fn new_a64_jit(conf: *mut DynarmicConfig<u8>) -> *mut Jit;
        }
        let mut cb = Box::new(cb);
        let mut cpp_cb: Box<CallbackImpl<T>> = Box::new(CallbackImpl {
            vtable: unsafe {
                (&Dynarmic::<T>::CALLBACKS as *const DynarmicCallbacks<T> as *const ())
                    .byte_add(crate::cxx::VTABLE_DIFF)
            }, // SAFETY: vtable_diff is ensured by abi-specific code
            ptr: cb.as_mut(),
        });

        self.config.callbacks = cpp_cb.as_mut();
        if let Some(tpidr_el0) = self.tpidr_el0.as_deref_mut() {
            self.config.tpidr_el0 = tpidr_el0;
        }
        if let Some(tpidrro_el0) = self.tpidrro_el0.as_deref_mut() {
            self.config.tpidrro_el0 = tpidrro_el0;
        }

        Dynarmic {
            ptr: unsafe { &mut *new_a64_jit((&mut self.config as *mut DynarmicConfig<T>).cast()) }, // SAFETY: we're casting T, which has no effect on memory layout
            cpp_cb,
            rust_cb: cb,
            _lifetime: PhantomData,
        }
    }

    /// Sets the processor id.
    pub fn processor_id(&mut self, id: usize) -> &mut Self {
        self.config.processor_id = id;

        self
    }

    /// Sets the fastmem pointer.
    /// This should point to the beginning of a 2^`address_space_bits` bytes
    /// address space which is in arranged just like what you wish for emulated memory to
    /// be. If the host page faults on an address, the JIT will fallback to calling the
    /// [memory_read](Callbacks::memory_read)/[memory_write](Callbacks::memory_write) callbacks.
    ///
    /// - `ptr` - pointer to start of memory space
    /// - `address_space_bits` - width of memory space in bits
    /// - `recompile_on_fault` - whether dynarmic should recompile without fastmem when reaching a pagefault
    /// - `silently_mirror` - should dynarmic mirror the address spaces when going out of bounds, otherwise use callbacks
    ///
    /// # Safety
    /// - `ptr` must be a valid pointer pointing to a correctly sized memory space allocated with read/write access.
    pub unsafe fn fastmem(
        &mut self,
        ptr: *mut std::ffi::c_void,
        address_space_bits: usize,
        recompile_on_fault: bool,
        silently_mirror: bool,
    ) -> &mut Self {
        self.config.fastmem_pointer = (ptr as usize).into();
        self.config.fastmem_address_space_bits = address_space_bits;
        self.config.recompile_on_fastmem_failure = recompile_on_fault;
        self.config.silently_mirror_fastmem = silently_mirror;

        self
    }

    /// Determines if dynarmic should use the above fastmem_pointer for exclusive reads and
    /// writes and if dynarmic should recompile without fastmem if a pagefault is reached.
    ///
    /// On x64, dynarmic currently relies on x64 cmpxchg semantics which may not provide
    /// fully accurate emulation.
    pub fn fastmem_exclusive(
        &mut self,
        exclusive_access: bool,
        recompile_on_fault: bool,
    ) -> &mut Self {
        self.config.fastmem_exclusive_access = exclusive_access;
        self.config.recompile_on_exclusive_fastmem_failure = recompile_on_fault;

        self
    }

    /// This selects other optimizations than can't otherwise be disabled by setting other
    /// configuration options. This includes:
    /// - IR optimizations
    /// - Block linking optimizations
    /// - RSB optimizations
    ///
    /// This is intended to be used for debugging.
    pub fn optimizations(&mut self, optimization_flag: OptimizationFlag) -> &mut Self {
        self.config.optimizations = optimization_flag;

        self
    }

    /// This enables unsafe optimizations that reduce emulation accuracy in favour of speed.
    /// For safety, in order to enable unsafe optimizations you have to set BOTH this flag
    /// AND the appropriate flag bits above.
    /// The prefered and tested mode for dynarmic is with unsafe optimizations disabled.
    pub fn unsafe_optimization(&mut self, enable: bool) -> &mut Self {
        self.config.unsafe_optimizations = enable;

        self
    }

    /// Enable/disable hooks for `ISB`, data cache instructions, and hint instructions.
    ///
    /// - `isb` - when enabled, [instruction_synchronization_barrier_raised](Callbacks::instruction_synchronization_barrier_raised)
    ///   will be called, otherwise it is treated as a NOP
    /// - `hint` - when enabled, [raised_exception](Callbacks::raised_exception) will be
    ///   called, otherwise it is treated as a NOP. Note that the default implementation
    ///   of `exception_raised` is to panic.
    /// - `cache_ops` - when enabled, [data_cache_operation_raised](Callbacks::data_cache_operation_raised)
    ///   will be called when any data cache instruction is executed, otherwise it will
    ///   never be called.
    ///
    /// By default, both are set to false.
    pub fn enable_hooks(&mut self, isb: bool, hint: bool, cache_ops: bool) -> &mut Self {
        self.config.hook_isb = isb;
        self.config.hook_hint_instructions = hint;
        self.config.hook_data_cache_operations = cache_ops;

        self
    }

    /// Counter-timer frequency register. The value of the register is not interpreted by
    /// dynarmic.
    pub fn cntfrq_el0(&mut self, val: u32) -> &mut Self {
        self.config.cntfrq_el0 = val;

        self
    }

    /// Sets CTR_EL0 value.
    ///
    /// - Bits `27:24` is log2 of the cache writeback granule in words.
    /// - Bits `23:20` is log2 of the exclusives reservation granule in words.
    /// - Bits `19:16` is log2 of the smallest data/unified cacheline in words.
    /// - Bits `15:14` is the level 1 instruction cache policy.
    /// - Bits `3:0` is log2 of the smallest instruction cacheline in words.
    pub fn ctr_el0(&mut self, val: u32) -> &mut Self {
        self.config.ctr_el0 = val;

        self
    }

    /// Sets DCZID_EL0 value.
    ///
    /// - Bits `3:0` is the log2 of the block size in words
    /// - Bit `4` is 0 if the DC ZVA instruction is permitted.
    ///
    /// All other bits are unused by dynarmic.
    pub fn dczid_el0(&mut self, val: u32) -> &mut Self {
        self.config.ctr_el0 = val;

        self
    }

    /// Set the pointer/reference to where TPIDRRO_EL0 is stored. This pointer
    /// will be inserted into emitted code.
    ///
    /// # Safety
    /// - `ref` must be a valid reference living for `'a` long.
    /// - Guest code may or may not modify the struct, potentially causing undefined behavior.
    pub unsafe fn tpidrro_el0<V>(&mut self, val: &'a mut V) -> &mut Self
    where
        V: Sized,
    {
        self.tpidr_el0 =
            Some(unsafe { std::mem::transmute::<&'a mut V, &'a mut std::ffi::c_void>(val) }); // SAFETY: transmuting to drop V, we don't need to know this nor complicate generics further

        self
    }

    /// Set the pointer/reference to where TPIDR_EL0 is stored. This pointer
    /// will be inserted into emitted code.
    ///
    /// # Safety
    /// - `ref` must be a valid reference living for `'a` long.
    /// - Guest code may or may not modify the struct, potentially causing undefined behavior.
    pub unsafe fn tpidr_el0<V>(&mut self, val: &'a mut V) -> &mut Self
    where
        V: Sized,
    {
        self.tpidr_el0 =
            Some(unsafe { std::mem::transmute::<&'a mut V, &'a mut std::ffi::c_void>(val) }); // SAFETY: transmuting to drop V, we don't need to know this nor complicate generics further

        self
    }

    /// The page table is used for faster memory access. If an entry in the table is nullptr,
    /// the JIT will fallback to calling the memory_read/memory_write callbacks.
    ///
    /// `silently_mirror` determines whether Dynarmic should mirror the page table when going
    /// out of its bounds.
    ///
    /// # Safety
    /// - `table` must be a valid pointer pointing to an array of pointers of size 2^`address_space_bits`.
    /// - `address_space_bits` must be a value between 12 and 64 inclusive.
    pub unsafe fn page_table(
        &mut self,
        table: *mut *mut std::ffi::c_void,
        address_space_bits: usize,
        silently_mirror: bool,
    ) -> &mut Self {
        self.config.page_table = table;
        self.config.page_table_address_space_bits = address_space_bits;
        self.config.silently_mirror_page_table = silently_mirror;

        self
    }

    /// Masks out the first N bits in host pointers from the page table.
    /// The intention behind this is to allow users of Dynarmic to pack attributes in the
    /// same integer and update the pointer attribute pair atomically.
    /// If the configured value is 3, all pointers will be forcefully aligned to 8 bytes.
    pub fn page_table_mask(&mut self, mask_bits: i32) -> &mut Self {
        self.config.absolute_offset_page_table = false;
        self.config.page_table_pointer_mask_bits = mask_bits;

        self
    }

    /// This option allows you to enable/disable cycle counting. If this is set to false,
    /// [add_ticks](Callbacks::add_ticks) and [get_ticks_remaining](Callbacks::get_ticks_remaining) are never called, and no cycle counting is done.
    ///
    /// By default, this is set to true.
    pub fn cycle_counting(&mut self, enable: bool) -> &mut Self {
        self.config.enable_cycle_counting = enable;

        self
    }

    /// Sets the size of the recompiled code cache.
    pub fn code_cache_size(&mut self, size: usize) -> &mut Self {
        self.config.code_cache_size = size;

        self
    }
}

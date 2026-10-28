use crate::{cxx::CxxOptional, cxx::CxxSharedPtr, CallbackImpl, GuestInt, OptimizationFlag};
use std::ops::{Deref, DerefMut};
pub type VAddr = u32;

// Internal VTable for Callbacks
#[repr(C)]
struct DynarmicCallbacks<T> {
    #[cfg(not(target_env = "msvc"))]
    offset_to_top: isize, // our inheritence is fake, we're essentially making UserCallbacks a final class, so this should always be 0 as UserCallbacks and TranslateCallbacks have no fields
    #[cfg(not(target_env = "msvc"))]
    typeinfo: crate::cxx::TypeInfoPtr,

    // TranslateCallbacks

    // https://github.com/rust-lang/rust/issues/38258
    #[cfg(not(target_env = "msvc"))]
    memory_read_code: unsafe extern "C" fn(&CallbackImpl<T>, VAddr) -> CxxOptional<u32>,
    #[cfg(target_env = "msvc")]
    memory_read_code: unsafe extern "C" fn(&CallbackImpl<T>, *mut CxxOptional<u32>, VAddr),

    pre_code_read_hook: extern "C" fn(&mut CallbackImpl<T>, bool, VAddr, &IREmitter) -> bool,
    pre_code_translation_hook: extern "C" fn(&mut CallbackImpl<T>, bool, VAddr, &IREmitter),
    get_ticks_for_code: extern "C" fn(&mut CallbackImpl<T>, bool, VAddr, u32) -> u64,

    // these functions should never be called; UserCallbacks should always be owned by Rust
    cpp_destructor: extern "C" fn(),
    #[cfg(not(target_env = "msvc"))]
    itanium_destructor: extern "C" fn(),

    // UserCallbacks
    memory_read_8: extern "C" fn(&CallbackImpl<T>, VAddr) -> u8,
    memory_read_16: extern "C" fn(&CallbackImpl<T>, VAddr) -> u16,
    memory_read_32: extern "C" fn(&CallbackImpl<T>, VAddr) -> u32,
    memory_read_64: extern "C" fn(&CallbackImpl<T>, VAddr) -> u64,
    memory_write_8: extern "C" fn(&mut CallbackImpl<T>, VAddr, u8),
    memory_write_16: extern "C" fn(&mut CallbackImpl<T>, VAddr, u16),
    memory_write_32: extern "C" fn(&mut CallbackImpl<T>, VAddr, u32),
    memory_write_64: extern "C" fn(&mut CallbackImpl<T>, VAddr, u64),
    memory_write_exclusive_8: extern "C" fn(&mut CallbackImpl<T>, VAddr, u8, u8) -> bool,
    memory_write_exclusive_16: extern "C" fn(&mut CallbackImpl<T>, VAddr, u16, u16) -> bool,
    memory_write_exclusive_32: extern "C" fn(&mut CallbackImpl<T>, VAddr, u32, u32) -> bool,
    memory_write_exclusive_64: extern "C" fn(&mut CallbackImpl<T>, VAddr, u64, u64) -> bool,

    is_readonly_memory: extern "C" fn(&CallbackImpl<T>, VAddr) -> bool,
    interpreter_fallback: extern "C" fn(&mut CallbackImpl<T>, VAddr, usize),
    call_svc: extern "C" fn(&mut CallbackImpl<T>, u32),
    exception_raised: extern "C" fn(&mut CallbackImpl<T>, VAddr, Exception),
    instruction_synchronization_barrier_raised: extern "C" fn(&mut CallbackImpl<T>),
    add_ticks: extern "C" fn(&mut CallbackImpl<T>, u64),
    get_ticks_remaining: extern "C" fn(&CallbackImpl<T>) -> u64,
}

#[repr(C)]
pub(crate) struct DynarmicConfig<T> {
    callbacks: *mut CallbackImpl<T>,
    processor_id: usize,
    global_monitor: *mut crate::ExclusiveMonitor,
    arch_version: ArchVersion,
    optimizations: OptimizationFlag,
    unsafe_optimizations: bool,
    page_table: *mut [*mut u8; 1 << (32 - 12)],
    absolute_offset_page_table: bool,
    page_table_pointer_mask_bits: i32,
    detect_misaligned_access_via_page_table: u8,
    only_detect_misalignment_via_page_table_on_page_boundary: bool,
    page_table_write_protect_bit: bool,
    fastmem_pointer: CxxOptional<usize>,
    recompile_on_fastmem_failure: bool,
    fastmem_exclusive_access: bool,
    recompile_on_exclusive_fastmem_failure: bool,
    coprocessors: [CxxSharedPtr<Coprocessor>; 16],
    hook_isb: bool,
    hook_hint_instructions: bool,
    define_unpredictable_behaviour: bool,
    wall_clock_cntpct: bool,
    check_halt_on_memory_access: bool,
    enable_cycle_counting: bool,
    always_little_endian: bool,
    code_cache_size: usize,
    very_verbose_debugging_output: bool,
}

#[repr(i32)]
#[derive(Debug)]
pub enum Exception {
    /// An UndefinedFault occured due to executing instruction with an unallocated encoding
    UndefinedInstruction = 0,
    /// An unpredictable instruction is to be executed. Implementation-defined behaviour should now happen.
    /// This behaviour is up to the user of this library to define.
    UnpredictableInstruction = 1,
    /// A decode error occurred when decoding this instruction. This should never happen.
    DecodeError = 2,
    /// A SEV instruction was executed. The event register of all PEs should be set. (Hint instruction.)
    SendEvent = 3,
    /// A SEVL instruction was executed. The event register of the current PE should be set. (Hint instruction.)
    SendEventLocal = 4,
    /// A WFI instruction was executed. You may now enter a low-power state. (Hint instruction.)
    WaitForInterrupt = 5,
    /// A WFE instruction was executed. You may now enter a low-power state if the event register is clear. (Hint instruction.)
    WaitForEvent = 6,
    /// A YIELD instruction was executed. (Hint instruction.)
    Yield = 7,
    /// A BKPT instruction was executed.
    Breakpoint = 8,
    /// A PLD instruction was executed. (Hint instruction.)
    PreloadData = 9,
    /// A PLDW instruction was executed. (Hint instruction.)
    PreloadDataWithIntentToWrite = 10,
    /// A PLI instruction was executed. (Hint instruction.)
    PreloadInstruction = 11,
    /// Attempted to execute a code block at an address for which MemoryReadCode returned std::nullopt.
    /// (Intended to be used to emulate memory protection faults.)
    NoExecuteFault = 12,
}

#[repr(i32)]
#[derive(Debug)]
pub enum ArchVersion {
    V3 = 0,
    V4 = 1,
    V4T = 2,
    V5TE = 3,
    V6K = 4,
    V6T2 = 5,
    V7 = 6,
    V8 = 7,
}

#[repr(C)]
pub struct IREmitter {
    _ffi: u8, // TODO
}

#[repr(C)]
pub struct Coprocessor {
    _ffi: u8, // TODO
}

#[repr(i32)]
pub enum CoprocReg {
    C0 = 0,
    C1 = 1,
    C2 = 2,
    C3 = 3,
    C4 = 4,
    C5 = 5,
    C6 = 6,
    C7 = 7,
    C8 = 8,
    C9 = 9,
    C10 = 10,
    C11 = 11,
    C12 = 12,
    C13 = 13,
    C14 = 14,
    C15 = 15,
}

/// Callback functions that dynarmic will use to access memory and
/// when the code calls for a higher exception level (e.g. SVC calls)
#[allow(unused_variables)]
pub trait Callbacks: Sized {
    /// All reads through this callback are 4-byte aligned.
    /// Memory must be interpreted as little endian.
    fn memory_read_code(cb: &CallbackImpl<Self>, addr: VAddr) -> Option<u32> {
        Some(Self::memory_read(cb, addr))
    }

    /// This function is called before the instruction at pc is read.
    /// IR code can be emitted by the callee prior to instruction handling.
    /// By returning true the callee precludes the translation of the instruction;
    /// in such case the callee is responsible for setting the terminal.
    // TODO: IREmitter
    extern "C" fn pre_code_read_hook(
        cb: &mut CallbackImpl<Self>,
        is_thumb: bool,
        pc: VAddr,
        ir: &IREmitter,
    ) -> bool {
        true
    }

    /// This function is called before the instruction at pc is interpreted.
    /// IR code can be emitted by the callee prior to translation of the instruction.
    extern "C" fn pre_code_translation_hook(
        _cb: &mut CallbackImpl<Self>,
        _is_thumb: bool,
        _pc: VAddr,
        _ir: &IREmitter,
    ) {
    }

    extern "C" fn get_ticks_for_code(
        _cb: &mut CallbackImpl<Self>,
        _is_thumb: bool,
        _pc: VAddr,
        _instruct: u32,
    ) -> u64 {
        1
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
    extern "C" fn is_readonly_memory(_cb: &CallbackImpl<Self>, _addr: VAddr) -> bool {
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
    /// - See [crate::a64::Exception] for the different types of potential exceptions.
    /// - Hint instructions will only be called for this function if `isb` was enabled
    ///   in [crate::a64::Config::enable_hooks].
    extern "C" fn raised_exception(cb: &mut CallbackImpl<Self>, addr: VAddr, exc: Exception) {
        panic!("dynarmic: Unhandled exception '{exc:?}' at '0x{addr:X}'",)
    }
    extern "C" fn call_svc(cb: &mut CallbackImpl<Self>, swi: u32);
    extern "C" fn instruction_synchronization_barrier_raised(cb: &mut CallbackImpl<Self>) {}
    /// This function is called by the JIT to determine that `tick` ticks have passed and
    /// should be added to the tick counter.
    ///
    /// # Notes
    /// - This function is only used if [Config::cycle_counting](Config::cycle_counting) was set to true.
    /// - When the tick counter reaches 0, the guest will return.
    extern "C" fn add_ticks(cb: &mut CallbackImpl<Self>, ticks: u64);
    /// Returns the remaining amount of ticks before the program stops.
    ///
    /// # Notes
    /// - This function is only used if [Config::cycle_counting](Config::cycle_counting) was set to true.
    /// - When this function returns 0, or when dynarmic estimates that the program is over based on a past call
    ///   to this function, the guest will return.
    extern "C" fn get_ticks_remaining(cb: &CallbackImpl<Self>) -> u64;
}

#[repr(C)]
pub(crate) struct Jit {
    is_executing: bool,
    _ptr: u64,
}

const _: () = assert!(size_of::<Jit>() == 16);

/// A Rust-safe wrapper of Dynarmic's A32 Jit.
/// This type can be constructed with [Config].
#[allow(dead_code)]
pub struct Dynarmic<T: Callbacks> {
    ptr: *mut Jit,
    cpp_cb: Box<CallbackImpl<T>>,
    rust_cb: Box<T>,
}

impl<T: Callbacks> Drop for Dynarmic<T> {
    fn drop(&mut self) {
        unsafe extern "C-unwind" {
            pub fn delete_a32_jit(ptr: *mut Jit);
        }
        unsafe { delete_a32_jit(self.ptr) }
    }
}

impl<T: Callbacks> Dynarmic<T> {
    #[cfg(not(target_env = "msvc"))]
    unsafe extern "C" fn memory_read_code_impl(
        cb: &CallbackImpl<T>,
        addr: VAddr,
    ) -> CxxOptional<u32> {
        T::memory_read_code(cb, addr).unwrap_or(0).into()
    }
    #[cfg(target_env = "msvc")]
    unsafe extern "C" fn memory_read_code_impl(
        cb: &CallbackImpl<T>,
        out: *mut CxxOptional<u32>,
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
        pre_code_read_hook: T::pre_code_read_hook,
        pre_code_translation_hook: T::pre_code_translation_hook,
        get_ticks_for_code: T::get_ticks_for_code,
        cpp_destructor: crate::cxx::unimplemented_destructor,
        #[cfg(not(target_env = "msvc"))]
        itanium_destructor: crate::cxx::unimplemented_destructor,
        memory_read_8: T::memory_read::<u8>,
        memory_read_16: T::memory_read::<u16>,
        memory_read_32: T::memory_read::<u32>,
        memory_read_64: T::memory_read::<u64>,
        memory_write_8: T::memory_write::<u8>,
        memory_write_16: T::memory_write::<u16>,
        memory_write_32: T::memory_write::<u32>,
        memory_write_64: T::memory_write::<u64>,
        memory_write_exclusive_8: T::memory_write_exclusive::<u8>,
        memory_write_exclusive_16: T::memory_write_exclusive::<u16>,
        memory_write_exclusive_32: T::memory_write_exclusive::<u32>,
        memory_write_exclusive_64: T::memory_write_exclusive::<u64>,
        is_readonly_memory: T::is_readonly_memory,
        interpreter_fallback: T::interpreter_fallback,
        call_svc: T::call_svc,
        exception_raised: T::raised_exception,
        instruction_synchronization_barrier_raised: T::instruction_synchronization_barrier_raised,
        add_ticks: T::add_ticks,
        get_ticks_remaining: T::get_ticks_remaining,
    };

    #[inline(always)]
    pub fn new_config() -> Config<T> {
        Config::new()
    }

    /// Runs the emulated CPU.
    /// Cannot be recursively called.
    /// # Safety
    /// - All instructions and memory addresses inputted must be valid. Invalid addresses/instructions will cause dynarmic exceptions, which panic by default.
    /// - Some ARM coprocessor instructions may be forwarded to [Coprocessor] callbacks; if these are unhandled (e.g. no coprocessors provided), this may result in a C++ exception.
    // TODO: make this function safe
    #[inline]
    pub unsafe fn run(&mut self) -> crate::HaltReason {
        unsafe extern "C-unwind" {
            pub fn JitA32_Run(this: *mut Jit) -> crate::HaltReason;
        }
        unsafe { JitA32_Run(self.ptr) }
    }

    /// Step the emulated CPU for one instruction.
    /// Cannot be recursively called.
    #[inline]
    pub fn step(&mut self) -> crate::HaltReason {
        unsafe extern "C-unwind" {
            pub fn JitA32_Step(this: *mut Jit) -> crate::HaltReason;
        }
        unsafe { JitA32_Step(self.ptr) }
    }

    /// Clears the code cache of all compiled code.
    /// Can be called at any time. Halts execution if called within a callback.
    #[inline]
    pub fn clear_cache(&mut self) {
        unsafe extern "C-unwind" {
            pub fn JitA32_ClearCache(this: *mut Jit);
        }
        unsafe { JitA32_ClearCache(self.ptr) }
    }

    /// Invalidate the code cache at a range of addresses.
    /// - `start_address` - The starting address of the range to invalidate.
    /// - `length` - The length (in bytes) of the range to invalidate.
    #[inline]
    pub fn invalidate_cache_range(&mut self, start_addr: VAddr, length: usize) {
        unsafe extern "C-unwind" {
            pub fn JitA32_InvalidateCacheRange(this: *mut Jit, start_address: u32, length: usize);
        }
        unsafe { JitA32_InvalidateCacheRange(self.ptr, start_addr, length) }
    }

    /// Reset CPU state to state at startup. Does not clear code cache.
    /// Cannot be called from a callback.
    #[inline]
    pub fn reset(&mut self) {
        unsafe extern "C-unwind" {
            pub fn JitA32_Reset(this: *mut Jit);
        }
        unsafe { JitA32_Reset(self.ptr) }
    }

    /// Stops execution during [Dynarmic::run].
    #[inline]
    pub fn halt(&mut self, hr: crate::HaltReason) {
        unsafe extern "C-unwind" {
            pub fn JitA32_HaltExecution(this: *mut Jit, hr: crate::HaltReason);
        }
        unsafe { JitA32_HaltExecution(self.ptr, hr) }
    }

    /// Clears a halt reason from flags.
    #[inline]
    pub fn clear_halt(&mut self, hr: crate::HaltReason) {
        unsafe extern "C-unwind" {
            pub fn JitA32_ClearHalt(this: *mut Jit, hr: crate::HaltReason);
        }
        unsafe { JitA32_ClearHalt(self.ptr, hr) }
    }

    /// View general-purpose registers.
    #[inline]
    pub fn get_regs(&self) -> &[u32; 16] {
        unsafe extern "C-unwind" {
            pub fn JitA32_Regs(this: *mut Jit) -> *mut u8;
        }
        unsafe { &*(JitA32_Regs(self.ptr).cast::<[u32; 16]>()) }
    }

    /// Replace general-purpose registers.
    #[inline]
    pub fn set_regs(&mut self, regs: [u32; 16]) {
        unsafe extern "C-unwind" {
            pub fn JitA32_Regs(this: *mut Jit) -> *mut u8;
        }
        unsafe {
            std::ptr::copy_nonoverlapping(regs.as_ptr(), JitA32_Regs(self.ptr).cast(), 16);
        }
    }

    /// Get raw FP/SIMD registers in units of u32.
    #[inline]
    pub fn get_extregs(&self) -> &[u32; 64] {
        unsafe extern "C-unwind" {
            pub fn JitA32_ExtRegs(this: *mut Jit) -> *mut u8;
        }
        unsafe { &*(JitA32_ExtRegs(self.ptr).cast::<[u32; 64]>()) }
    }

    /// Replace FP/SIMD registers.
    #[inline]
    pub fn set_extregs(&self, regs: [u32; 64]) {
        unsafe extern "C-unwind" {
            pub fn JitA32_ExtRegs(this: *mut Jit) -> *mut u8;
        }
        unsafe {
            std::ptr::copy_nonoverlapping(regs.as_ptr(), JitA32_ExtRegs(self.ptr).cast(), 64);
        }
    }

    #[inline]
    pub fn get_reg(&self, index: usize) -> u32 {
        self.get_regs()[index]
    }

    #[inline]
    pub fn set_reg(&mut self, index: usize, val: u32) {
        unsafe {
            // SAFETY: get_regs returns a raw reference
            *(self.get_regs() as *const u32 as *mut u32).wrapping_add(index) = val;
        }
    }

    /// Read Stack Pointer
    #[inline]
    pub fn get_sp(&self) -> u32 {
        self.get_reg(13)
    }

    /// Modify Stack Pointer
    #[inline]
    pub fn set_sp(&mut self, sp: u32) {
        self.set_reg(13, sp)
    }

    /// Read Program Counter
    #[inline]
    pub fn get_pc(&self) -> u32 {
        self.get_reg(15)
    }

    /// Modify Program Counter
    #[inline]
    pub fn set_pc(&mut self, pc: u32) {
        self.set_reg(15, pc)
    }

    /// View CPSR
    #[inline]
    pub fn get_cpsr(&self) -> u32 {
        unsafe extern "C-unwind" {
            pub fn JitA32_Cpsr(this: *const Jit) -> u32;
        }
        unsafe { JitA32_Cpsr(self.ptr) }
    }

    /// Modify CPSR
    #[inline]
    pub fn set_cpsr(&mut self, val: u32) {
        unsafe extern "C-unwind" {
            pub fn JitA32_SetCpsr(this: *mut Jit, value: u32);
        }
        unsafe { JitA32_SetCpsr(self.ptr, val) }
    }

    /// View FPSCR
    #[inline]
    pub fn get_fpscr(&self) -> u32 {
        unsafe extern "C-unwind" {
            pub fn JitA32_Fpscr(this: *const Jit) -> u32;
        }
        unsafe { JitA32_Fpscr(self.ptr) }
    }

    /// Modify FPSCR
    #[inline]
    pub fn set_fpscr(&mut self, val: u32) {
        unsafe extern "C-unwind" {
            pub fn JitA32_SetFpscr(this: *mut Jit, value: u32);
        }
        unsafe { JitA32_SetFpscr(self.ptr, val) }
    }

    /// Clears exclusive states for this core.
    #[inline]
    pub fn clear_exclusive_state(&mut self) {
        unsafe extern "C-unwind" {
            pub fn JitA32_ClearExclusiveState(this: *mut Jit);
        }
        unsafe { JitA32_ClearExclusiveState(self.ptr) }
    }

    /// Returns true if Jit::Run was called but hasn't returned yet.
    /// i.e; we're in a callback
    #[inline]
    pub fn is_executing(&self) -> bool {
        unsafe { (*self.ptr).is_executing }
    }
}

impl<T: Callbacks> Deref for Dynarmic<T> {
    type Target = CallbackImpl<T>;

    fn deref(&self) -> &Self::Target {
        self.cpp_cb.as_ref()
    }
}

impl<T: Callbacks> DerefMut for Dynarmic<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.cpp_cb.as_mut()
    }
}

pub struct Config<T: Callbacks> {
    config: DynarmicConfig<T>,
}

impl<T: Callbacks> Default for Config<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Callbacks> Config<T> {
    pub fn new() -> Self {
        Self {
            config: DynarmicConfig {
                callbacks: unsafe { std::mem::zeroed() },
                processor_id: 0,
                global_monitor: unsafe { std::mem::zeroed() }, // todo
                arch_version: ArchVersion::V8,
                optimizations: OptimizationFlag::ALL,
                unsafe_optimizations: false,
                page_table: std::ptr::null_mut(),
                absolute_offset_page_table: true,
                page_table_pointer_mask_bits: 0,
                detect_misaligned_access_via_page_table: 8 | 16 | 32 | 64,
                only_detect_misalignment_via_page_table_on_page_boundary: true,
                page_table_write_protect_bit: false,
                fastmem_pointer: 0usize.into(),
                recompile_on_fastmem_failure: true,
                fastmem_exclusive_access: false,
                recompile_on_exclusive_fastmem_failure: true,
                coprocessors: Default::default(),
                hook_isb: false,
                hook_hint_instructions: false,
                define_unpredictable_behaviour: false,
                wall_clock_cntpct: false,
                check_halt_on_memory_access: false,
                enable_cycle_counting: true,
                always_little_endian: false,
                code_cache_size: 128 * 1024 * 1024,
                very_verbose_debugging_output: false,
            },
        }
    }
    pub fn init(&mut self, cb: T) -> Dynarmic<T> {
        assert!(!self.config.page_table_write_protect_bit || self.config.page_table_pointer_mask_bits >= 1,
            "page-table write protection requires masking pointer bit zero");
        unsafe extern "C-unwind" {
            pub fn new_a32_jit(conf: *mut DynarmicConfig<u8>) -> *mut Jit;
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

        Dynarmic {
            ptr: unsafe { new_a32_jit((&mut self.config as *mut DynarmicConfig<T>).cast()) }, // SAFETY: we're casting T, which has no effect on memory layout
            cpp_cb,
            rust_cb: cb,
        }
    }

    /// Sets the processor id.
    pub fn processor_id(&mut self, id: usize) -> &mut Self {
        self.config.processor_id = id;

        self
    }

    /// Sets the fastmem pointer and determines whether dynarmic should recompile without fastmem
    /// if a pagefault is reached.
    ///
    /// # Safety
    /// - `ptr` must be a valid pointer pointing to a memory space allocated with read/write access.
    /// - The guest code should not attempt to access outside the memory range of `ptr`. Note that dynarmic will not fallback to [memory_read](Callbacks::memory_read)/[memory_write](Callbacks::memory_write)
    ///   when exceeding the fastmem address space as it does not know the size.
    ///
    pub unsafe fn fastmem(
        &mut self,
        ptr: *mut std::ffi::c_void,
        recompile_on_fault: bool,
    ) -> &mut Self {
        self.config.fastmem_pointer = (ptr as usize).into();
        self.config.recompile_on_fastmem_failure = recompile_on_fault;

        self
    }

    /// Determines if dynarmic should use the above fastmem_pointer for exclusive reads and
    /// writes and if dynarmic should recompile without fastmem if a pagefault is reached.
    ///
    /// On x64, dynarmic currently relies on x64 cmpxchg semantics which may not provide
    /// fully accurate emulation.
    pub fn fastmem_exclusive(&mut self, exclusive_access: bool, recompile_on_fault: bool) {
        self.config.fastmem_exclusive_access = exclusive_access;
        self.config.recompile_on_exclusive_fastmem_failure = recompile_on_fault;
    }

    /// Select the architecture version to use.
    /// There are minor behavioural differences between versions.
    /// By default, it is set to V8.
    pub fn arch_ver(&mut self, arch: ArchVersion) {
        self.config.arch_version = arch;
    }

    /// This selects other optimizations than can't otherwise be disabled by setting other
    /// configuration options. This includes:
    /// - IR optimizations
    /// - Block linking optimizations
    /// - RSB optimizations
    ///
    /// This is intended to be used for debugging.
    pub fn optimizations(&mut self, optimization_flag: OptimizationFlag) {
        self.config.optimizations = optimization_flag;
    }

    /// This enables unsafe optimizations that reduce emulation accuracy in favour of speed.
    /// For safety, in order to enable unsafe optimizations you have to set BOTH this flag
    /// AND the appropriate flag bits above.
    /// The prefered and tested mode for dynarmic is with unsafe optimizations disabled.
    pub fn unsafe_optimization(&mut self, enable: bool) {
        self.config.unsafe_optimizations = enable;
    }

    /// The page table is used for faster memory access. If an entry in the table is nullptr,
    /// the JIT will fallback to calling the memory_read/memory_write callbacks.
    /// # Safety
    /// - `table` must be a valid pointer pointing to an array of pointers of size 2^20.
    pub unsafe fn page_table(&mut self, table: *mut [*mut u8; 1 << (32 - 12)]) {
        self.config.page_table = table;
    }

    /// Masks out the first N bits in host pointers from the page table.
    /// The intention behind this is to allow users of Dynarmic to pack attributes in the
    /// same integer and update the pointer attribute pair atomically.
    /// If the configured value is 3, all pointers will be forcefully aligned to 8 bytes.
    pub fn page_table_mask(&mut self, mask_bits: i32) {
        self.config.absolute_offset_page_table = false;
        self.config.page_table_pointer_mask_bits = mask_bits;
    }

    /// Bit zero marca páginas legíveis cuja escrita deve passar pelas callbacks.
    /// A máscara deve retirar esse bit antes de usar o ponteiro; desligado por padrão.
    pub fn page_table_write_protection(&mut self, enable: bool) {
        self.config.page_table_write_protect_bit = enable;
    }

    /// Enable/disable hooks for `ISB` and hint instructions.
    ///
    /// - `isb` - when enabled, [instruction_synchronization_barrier_raised](Callbacks::instruction_synchronization_barrier_raised)
    ///   will be called, otherwise it is treated as a NOP
    /// - `hint` - when enabled, [raised_exception](Callbacks::raised_exception) will be
    ///   called, otherwise it is treated as a NOP. Note that the default implementation
    ///   of `exception_raised` is to panic.
    ///
    /// By default, both are set to false.
    pub fn enable_hooks(&mut self, isb: bool, hint: bool) {
        self.config.hook_isb = isb;
        self.config.hook_hint_instructions = hint;
    }

    /// This option allows you to enable/disable cycle counting. If this is set to false,
    /// AddTicks and GetTicksRemaining are never called, and no cycle counting is done.
    ///
    /// By default, this is set to true.
    pub fn cycle_counting(&mut self, enable: bool) {
        self.config.enable_cycle_counting = enable;
    }

    /// Permite que MemoryAbort encerre o bloco antes da próxima instrução guest.
    pub fn check_halt_on_memory_access(&mut self, enable: bool) {
        self.config.check_halt_on_memory_access = enable;
    }

    /// Sets the size of the recompiled code cache.
    pub fn code_cache_size(&mut self, size: usize) {
        self.config.code_cache_size = size;
    }
}

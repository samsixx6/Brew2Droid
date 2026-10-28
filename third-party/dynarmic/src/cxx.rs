use std::mem::MaybeUninit;

#[repr(C)]
pub struct CxxVector<T: Sized> {
    __data: [*mut T; 3],
}

#[repr(C)]
pub struct CxxOptional<T: Sized> {
    __data: MaybeUninit<T>,
    __bool: bool,
}

impl From<usize> for CxxOptional<usize> {
    fn from(value: usize) -> Self {
        unsafe extern "C-unwind" {
            pub fn new_optional_usize(out: *mut CxxOptional<usize>, s: usize);
        }
        unsafe {
            let mut optional: MaybeUninit<CxxOptional<usize>> = MaybeUninit::uninit();
            new_optional_usize(optional.as_mut_ptr(), value);
            optional.assume_init()
        }
    }
}

impl From<u32> for CxxOptional<u32> {
    fn from(value: u32) -> Self {
        unsafe extern "C-unwind" {
            pub fn new_optional_u32(out: *mut CxxOptional<u32>, s: u32);
        }
        unsafe {
            let mut optional: MaybeUninit<CxxOptional<u32>> = MaybeUninit::uninit();
            new_optional_u32(optional.as_mut_ptr(), value);
            optional.assume_init()
        }
    }
}

#[repr(C)]
pub struct CxxSharedPtr<T: Sized> {
    __data: *mut MaybeUninit<T>,
    __control: *mut (),
}

impl CxxSharedPtr<crate::a32::Coprocessor> {
    /*pub fn new(coprocessor: crate::a32::Coprocessor) -> Self {
        unsafe {
            let mut optional: MaybeUninit<CxxSharedPtr<crate::a32::Coprocessor>> = MaybeUninit::uninit();
            new_coprocessor(
                optional.as_mut_ptr(),
                &coprocessor as *const _,
            );
            optional.assume_init()
        }
    }*/
}

impl Default for CxxSharedPtr<crate::a32::Coprocessor> {
    fn default() -> Self {
        unsafe { std::mem::zeroed() }
    }
}

const _: () = {
    assert!(
        size_of::<CxxOptional<u32>>() == 8,
        "Failed to verify size of type std::optional<u32>"
    );
    assert!(
        size_of::<CxxOptional<u64>>() == 16,
        "Failed to verify size of type std::optional<u64>"
    );
    assert!(
        size_of::<CxxOptional<usize>>() == 16,
        "Failed to verify size of type std::optional<usize>"
    );
    assert!(
        size_of::<crate::a32::DynarmicConfig<u32>>() == 368,
        "Failed to verify size of type a32::UserConfig"
    );
    assert!(
        size_of::<crate::a64::DynarmicConfig<u32>>() == 144,
        "Failed to verify size of type a64::UserConfig"
    );
    assert!(
        size_of::<CxxSharedPtr<crate::a32::Coprocessor>>() == 16,
        "Failed to verify size of type cpp_shared_ptr"
    );
    assert!(
        size_of::<crate::ExclusiveMonitor>() == 56,
        "Failed to verify size of type ExclusiveMonitor"
    );
};

#[repr(transparent)]
#[allow(unused)]
pub struct TypeInfoPtr(*const ());
unsafe impl Send for TypeInfoPtr {}
unsafe impl Sync for TypeInfoPtr {}

#[cfg(target_env = "msvc")]
pub const VTABLE_DIFF: usize = 0;

#[cfg(not(target_env = "msvc"))]
pub const VTABLE_DIFF: usize = 16;

pub extern "C" fn unimplemented_destructor() {
    panic!(
        "Dynarmic attempted to call UserCallbacks destructor; UserCallbacks should ALWAYS be owned by Rust code"
    )
}

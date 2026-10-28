// A mesma política e o mesmo export JNI do APK, com o adaptador de tipos já usado no host.
include!("../handset_android_volume.rs");

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_brew_handset_BackNavigationTest_takeBack(
    _: *mut std::ffi::c_void, _: *mut std::ffi::c_void,
) -> u8 { input_dispatch::take_back() as u8 }

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_brew_handset_GameMenuBoundaryTest_takeMenu(
    _: *mut std::ffi::c_void, _: *mut std::ffi::c_void,
) -> i32 { input_dispatch::take_menu() as i32 }

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_brew_handset_BackNavigationTest_takePhysical(
    _: *mut std::ffi::c_void, _: *mut std::ffi::c_void,
) -> i64 {
    let reports=input_dispatch::take_physical_back();
    let Some(&(device,scan,down))=reports.last() else{return 0;};
    if device<0 {return -(reports.len() as i64);}
    ((reports.len() as i64)<<48)|((device as i64)<<32)|((scan as i64)<<1)|i64::from(down)
}

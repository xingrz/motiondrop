use core_foundation::base::TCFType;
use core_video::pixel_buffer::{CVPixelBuffer, CVPixelBufferRef};
use std::{
    ffi::{CString, c_void},
    os::unix::ffi::OsStrExt,
    path::Path,
    ptr::NonNull,
};

unsafe extern "C" {
    fn md_player_create(path: *const std::ffi::c_char) -> *mut c_void;
    fn md_player_destroy(handle: *mut c_void);
    fn md_player_replay(handle: *mut c_void);
    fn md_player_stop(handle: *mut c_void);
    fn md_player_status(handle: *mut c_void) -> i32;
    fn md_player_frame(handle: *mut c_void) -> CVPixelBufferRef;
}

/// Main-thread-only AVFoundation player, rendered by GPUI's Metal surface.
/// NonNull deliberately prevents Send/Sync; the retained ObjC owner is released on drop.
pub struct Player(NonNull<c_void>);

impl Player {
    pub fn new(path: &Path) -> Option<Self> {
        let path = CString::new(path.as_os_str().as_bytes()).ok()?;
        // SAFETY: NUL-terminated path; native code copies the path and returns a retained owner.
        NonNull::new(unsafe { md_player_create(path.as_ptr()) }).map(Self)
    }

    pub fn replay(&self) {
        unsafe { md_player_replay(self.0.as_ptr()) }
    }

    pub fn stop(&self) {
        unsafe { md_player_stop(self.0.as_ptr()) }
    }

    pub fn status(&self) -> i32 {
        unsafe { md_player_status(self.0.as_ptr()) }
    }

    pub fn frame(&self) -> Option<CVPixelBuffer> {
        // The native copy method follows CoreFoundation's create rule (+1 retain).
        let frame = unsafe { md_player_frame(self.0.as_ptr()) };
        if frame.is_null() {
            None
        } else {
            Some(unsafe { CVPixelBuffer::wrap_under_create_rule(frame) })
        }
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        unsafe { md_player_destroy(self.0.as_ptr()) }
    }
}

pub fn dark_appearance() {
    unsafe extern "C" {
        fn md_dark_appearance();
    }
    unsafe {
        md_dark_appearance();
    }
}

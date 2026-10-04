use image::ImageDecoder;
use std::{io::Cursor, path::Path};

#[derive(Clone, Debug)]
pub struct MediaInfo {
    pub width: u32,
    pub height: u32,
    pub duration: Option<f64>,
}

pub fn probe(path: &Path, bytes: &[u8]) -> Option<MediaInfo> {
    if bytes.starts_with(&[0xff, 0xd8]) {
        let mut decoder = image::ImageReader::new(Cursor::new(bytes))
            .with_guessed_format()
            .ok()?
            .into_decoder()
            .ok()?;
        let (mut width, mut height) = decoder.dimensions();
        if decoder.orientation().ok().is_some_and(|o| o.to_exif() >= 5) {
            std::mem::swap(&mut width, &mut height);
        }
        return Some(MediaInfo {
            width,
            height,
            duration: None,
        });
    }
    probe_video(path)
}

#[cfg(target_os = "macos")]
fn probe_video(path: &Path) -> Option<MediaInfo> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    unsafe extern "C" {
        fn md_probe_video(
            path: *const std::ffi::c_char,
            width: *mut u32,
            height: *mut u32,
            duration: *mut f64,
        ) -> i32;
    }
    let path = CString::new(path.as_os_str().as_bytes()).ok()?;
    let (mut width, mut height, mut duration) = (0, 0, 0.);
    // The native function copies the path and writes only to these live outputs.
    let success = unsafe { md_probe_video(path.as_ptr(), &mut width, &mut height, &mut duration) };
    (success != 0).then_some(MediaInfo {
        width,
        height,
        duration: Some(duration),
    })
}

#[cfg(not(target_os = "macos"))]
fn probe_video(_: &Path) -> Option<MediaInfo> {
    None
}

#[cfg(target_os = "macos")]
pub fn video_thumbnail(input: &Path, output: &Path) -> bool {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    unsafe extern "C" {
        fn md_video_thumbnail(
            input: *const std::ffi::c_char,
            output: *const std::ffi::c_char,
        ) -> i32;
    }
    let Ok(input) = CString::new(input.as_os_str().as_bytes()) else {
        return false;
    };
    let Ok(output) = CString::new(output.as_os_str().as_bytes()) else {
        return false;
    };
    // Both paths remain alive while AVFoundation generates and writes the thumbnail.
    unsafe { md_video_thumbnail(input.as_ptr(), output.as_ptr()) != 0 }
}

#[cfg(not(target_os = "macos"))]
pub fn video_thumbnail(_: &Path, _: &Path) -> bool {
    false
}

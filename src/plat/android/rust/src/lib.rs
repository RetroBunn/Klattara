//! Klattara for Android: a static library the JNI bridge (src/plat/android/jni)
//! links into libklattara.so. It holds:
//!
//! - the engine's C interface, the same functions as klattara.dll
//!   (src/plat/windows/klattara.h), compiled from the Windows layer's source;
//! - each voice's own value of every [:cv] setting, for the settings screen;
//! - pronunciation dictionaries (src/plat/shared/dictionary.rs);
//! - a memory allocator that older Android has the functions for.
//!
//! Its own functions are declared in klattara_android.h.

#[path = "../../../windows/src/lib.rs"]
mod ffi;
pub use ffi::*;

#[path = "../../../shared/voice_settings.rs"]
mod voice_settings;

#[path = "../../../shared/dictionary.rs"]
pub mod dictionary;

use dictionary::Dictionary;
use engine::{VOICES, Voice};
use std::ffi::{CStr, c_char, c_int};
use std::panic::{AssertUnwindSafe, catch_unwind};

/// Rust's own allocator asks for posix_memalign, which Android only has from
/// 4.2 (API 17): a library that needs it will not load on Android 4.0 and 4.1.
/// This one uses malloc, and memalign for the rare larger alignment, which
/// every Android has had.
#[cfg(target_os = "android")]
mod old_android_allocator {
    use std::alloc::{GlobalAlloc, Layout};
    use std::ffi::c_void;

    unsafe extern "C" {
        fn malloc(size: usize) -> *mut c_void;
        fn calloc(count: usize, size: usize) -> *mut c_void;
        fn realloc(pointer: *mut c_void, size: usize) -> *mut c_void;
        fn memalign(alignment: usize, size: usize) -> *mut c_void;
        fn free(pointer: *mut c_void);
    }

    /// What malloc's blocks are always aligned to on Android.
    const MALLOC_ALIGN: usize = if cfg!(target_pointer_width = "64") { 16 } else { 8 };

    struct OldAndroidAllocator;

    unsafe impl GlobalAlloc for OldAndroidAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            unsafe {
                if layout.align() <= MALLOC_ALIGN && layout.align() <= layout.size() {
                    malloc(layout.size()) as *mut u8
                } else {
                    memalign(layout.align().max(size_of::<usize>()), layout.size()) as *mut u8
                }
            }
        }

        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            unsafe {
                if layout.align() <= MALLOC_ALIGN && layout.align() <= layout.size() {
                    calloc(layout.size(), 1) as *mut u8
                } else {
                    let p = self.alloc(layout);
                    if !p.is_null() {
                        std::ptr::write_bytes(p, 0, layout.size());
                    }
                    p
                }
            }
        }

        unsafe fn dealloc(&self, pointer: *mut u8, _: Layout) {
            unsafe { free(pointer as *mut c_void) }
        }

        unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            unsafe {
                if layout.align() <= MALLOC_ALIGN && layout.align() <= new_size {
                    realloc(pointer as *mut c_void, new_size) as *mut u8
                } else {
                    let new = self.alloc(Layout::from_size_align_unchecked(new_size, layout.align()));
                    if !new.is_null() {
                        std::ptr::copy_nonoverlapping(pointer, new, layout.size().min(new_size));
                        self.dealloc(pointer, layout);
                    }
                    new
                }
            }
        }
    }

    #[global_allocator]
    static ALLOCATOR: OldAndroidAllocator = OldAndroidAllocator;
}

/// A voice's own value of a [:cv] setting, in the command's units; NaN when
/// the voice leaves it to the engine, or for an unknown voice or code.
///
/// # Safety
/// `code` must be null or a NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn klattara_android_voice_setting(index: c_int, code: *const c_char) -> f64 {
    if code.is_null() || index < 0 {
        return f64::NAN;
    }
    // SAFETY: the caller passes a NUL-terminated string.
    let code = unsafe { CStr::from_ptr(code) }.to_string_lossy();
    let Some(def) = VOICES.get(index as usize) else { return f64::NAN };
    voice_settings::setting_of(&Voice::from(def), &code)
}

/// A new, empty pronunciation dictionary.
#[unsafe(no_mangle)]
pub extern "C" fn klattara_dictionary_new() -> *mut Dictionary {
    Box::into_raw(Box::new(Dictionary::new()))
}

/// Frees a dictionary. NULL is ignored.
///
/// # Safety
/// `dictionary` must be null or from [`klattara_dictionary_new`], and not used after.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn klattara_dictionary_free(dictionary: *mut Dictionary) {
    if !dictionary.is_null() {
        // SAFETY: from klattara_dictionary_new, freed once.
        drop(unsafe { Box::from_raw(dictionary) });
    }
}

/// Adds a dictionary file's UTF-8 text. Writes how many entries were taken in,
/// how many lines were left out, and the first such line (0 if none) to
/// `results[0..3]`. Returns 0, or -1 for a null argument or an internal error.
///
/// # Safety
/// `dictionary` from [`klattara_dictionary_new`], `text` NUL-terminated, and
/// `results` writable for three ints.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn klattara_dictionary_load(
    dictionary: *mut Dictionary,
    text: *const c_char,
    results: *mut c_int,
) -> c_int {
    // SAFETY: the caller's contract.
    let (Some(dictionary), false, false) = (unsafe { dictionary.as_mut() }, text.is_null(), results.is_null()) else {
        return -1;
    };
    let text = unsafe { CStr::from_ptr(text) }.to_string_lossy();
    catch_unwind(AssertUnwindSafe(|| {
        let r = dictionary.load(&text);
        // SAFETY: writable for three ints.
        unsafe {
            *results = r.added.min(c_int::MAX as usize) as c_int;
            *results.add(1) = r.rejected.min(c_int::MAX as usize) as c_int;
            *results.add(2) = r.first_rejected_line.min(c_int::MAX as usize) as c_int;
        }
        0
    }))
    .unwrap_or(-1)
}

/// How many entries a dictionary holds.
///
/// # Safety
/// `dictionary` null or from [`klattara_dictionary_new`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn klattara_dictionary_size(dictionary: *const Dictionary) -> c_int {
    unsafe { dictionary.as_ref() }.map_or(0, |d| d.len().min(c_int::MAX as usize) as c_int)
}

/// Writes UTF-8 `text`, with the dictionary applied and brackets made safe,
/// into `buffer` if it fits in `size` bytes, NUL-terminated. Returns the size
/// needed including the NUL, or -1. A null dictionary only makes it safe.
///
/// # Safety
/// `dictionary` null or from [`klattara_dictionary_new`], `text`
/// NUL-terminated, `buffer` null or writable for `size` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn klattara_dictionary_apply(
    dictionary: *const Dictionary,
    text: *const c_char,
    buffer: *mut c_char,
    size: c_int,
) -> c_int {
    if text.is_null() {
        return -1;
    }
    // SAFETY: the caller's contract.
    let text = unsafe { CStr::from_ptr(text) }.to_string_lossy();
    let dictionary = unsafe { dictionary.as_ref() };
    catch_unwind(AssertUnwindSafe(|| {
        let out = match dictionary {
            Some(d) => d.apply(&text),
            None => dictionary::sanitize(&text),
        };
        let needed = out.len() + 1;
        if !buffer.is_null() && size >= 0 && needed <= size as usize {
            // SAFETY: writable for `size` bytes, and needed fits.
            unsafe {
                std::ptr::copy_nonoverlapping(out.as_ptr(), buffer as *mut u8, out.len());
                *buffer.add(out.len()) = 0;
            }
        }
        needed.min(c_int::MAX as usize) as c_int
    }))
    .unwrap_or(-1)
}

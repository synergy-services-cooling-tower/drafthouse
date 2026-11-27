use std::alloc::{alloc, dealloc, handle_alloc_error, Layout};
use std::ptr::null_mut;

const HEADER: usize = 4;

const ALIGN: usize = 4;

fn layout(total: usize) -> Option<Layout> {
    Layout::from_size_align(total, ALIGN).ok()
}

fn leak(payload: &[u8]) -> *mut u8 {
    let (Ok(len), Some(layout)) = (u32::try_from(payload.len()), layout(HEADER + payload.len()))
    else {
        return null_mut();
    };
    // SAFETY: the layout holds the header at least, so its size is not zero.
    let pointer = unsafe { alloc(layout) };
    if pointer.is_null() {
        handle_alloc_error(layout);
    }
    // SAFETY: `pointer` owns `HEADER + payload.len()` bytes, and `payload` does not overlap them.
    unsafe {
        pointer.cast::<u32>().write_unaligned(len);
        std::ptr::copy_nonoverlapping(payload.as_ptr(), pointer.add(HEADER), payload.len());
    }
    pointer
}

unsafe fn payload_len(pointer: *const u8) -> usize {
    usize::try_from(pointer.cast::<u32>().read_unaligned()).unwrap_or(0)
}

/// A buffer with room for `len` payload bytes.
#[no_mangle]
pub extern "C" fn ct_alloc(len: u32) -> *mut u8 {
    leak(&vec![0_u8; usize::try_from(len).unwrap_or(0)])
}

/// # Safety `pointer` came from `ct_alloc` or `ct_call`, or is null, and is freed once.
#[no_mangle]
pub unsafe extern "C" fn ct_free(pointer: *mut u8) {
    if pointer.is_null() {
        return;
    }
    // SAFETY: the caller guarantees the pointer came from this module's allocator.
    let len = unsafe { payload_len(pointer) };
    let Some(layout) = layout(HEADER + len) else {
        return;
    };
    // SAFETY: `leak` allocated exactly `HEADER + len` bytes at this alignment.
    unsafe { dealloc(pointer, layout) };
}

/// # Safety `pointer` came from `ct_alloc` with its payload written, and is not used again.
#[no_mangle]
pub unsafe extern "C" fn ct_call(pointer: *mut u8) -> *mut u8 {
    // SAFETY: the caller guarantees the buffer came from `ct_alloc` with its payload written.
    let reply = match unsafe { read_arguments(pointer) } {
        Ok(arguments) => drafthouse_cli::reply(&arguments),
        Err(message) => format!(
            "{{\"ok\":false,\"status\":2,\"error\":{{\"error\":\"usage\",\"message\":{}}}}}",
            drafthouse_cli::json_string(&message)
        ),
    };
    // SAFETY: `ct_call` owns the input buffer, and nothing reads it after this.
    unsafe { ct_free(pointer) };
    leak(reply.as_bytes())
}

unsafe fn read_arguments(pointer: *mut u8) -> Result<Vec<String>, String> {
    if pointer.is_null() {
        return Err("ct_call was given a null buffer".to_string());
    }
    // SAFETY: the caller guarantees the pointer came from `ct_alloc`.
    let len = unsafe { payload_len(pointer) };
    // SAFETY: the caller wrote `len` payload bytes after the header.
    let payload = unsafe { std::slice::from_raw_parts(pointer.add(HEADER), len) };
    let command_line = std::str::from_utf8(payload)
        .map_err(|error| format!("the command line is not UTF-8: {error}"))?;
    let arguments = command_line
        .split('\n')
        .filter(|argument| !argument.is_empty())
        .map(str::to_string)
        .collect::<Vec<String>>();
    if arguments.is_empty() {
        return Err("no command given".to_string());
    }
    Ok(arguments)
}

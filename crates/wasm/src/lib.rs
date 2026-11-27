use std::alloc::{alloc, dealloc, handle_alloc_error, Layout};

const HEADER: usize = 4;

const ALIGN: usize = 4;

fn layout(total: usize) -> Layout {
    Layout::from_size_align(total, ALIGN).expect("a valid layout")
}

fn leak(payload: &[u8]) -> *mut u8 {
    let total = HEADER + payload.len();
    let layout = layout(total);
    // SAFETY: `total` is non-zero (the header alone), so the layout is valid.
    let pointer = unsafe { alloc(layout) };
    if pointer.is_null() {
        handle_alloc_error(layout);
    }
    unsafe {
        pointer.cast::<u32>().write_unaligned(payload.len() as u32);
        std::ptr::copy_nonoverlapping(payload.as_ptr(), pointer.add(HEADER), payload.len());
    }
    pointer
}

unsafe fn payload_len(pointer: *const u8) -> usize {
    pointer.cast::<u32>().read_unaligned() as usize
}

/// A buffer with room for `len` payload bytes.
#[no_mangle]
pub extern "C" fn ct_alloc(len: u32) -> *mut u8 {
    leak(&vec![0u8; len as usize])
}

/// Free a buffer this module produced, input buffers included.
#[no_mangle]
pub extern "C" fn ct_free(pointer: *mut u8) {
    if pointer.is_null() {
        return;
    }
    // SAFETY: the caller guarantees the pointer came from this module's allocator.
    let len = unsafe { payload_len(pointer) };
    // SAFETY: `leak` allocated exactly `HEADER + len` bytes at this alignment.
    unsafe { dealloc(pointer, layout(HEADER + len)) };
}

/// Run one command line (one argument per line) and return the reply envelope.
#[no_mangle]
pub extern "C" fn ct_call(pointer: *mut u8) -> *mut u8 {
    let reply = match unsafe { read_arguments(pointer) } {
        Ok(arguments) => drafthouse_cli::reply(&arguments),
        Err(message) => format!(
            "{{\"ok\":false,\"status\":2,\"error\":{{\"error\":\"usage\",\"message\":{}}}}}",
            drafthouse_cli::json_string(&message)
        ),
    };
    ct_free(pointer);
    leak(reply.as_bytes())
}

unsafe fn read_arguments(pointer: *mut u8) -> Result<Vec<String>, String> {
    if pointer.is_null() {
        return Err("ct_call was given a null buffer".to_string());
    }
    let len = unsafe { payload_len(pointer) };
    // SAFETY: the caller wrote `len` payload bytes after the header.
    let payload = unsafe { std::slice::from_raw_parts(pointer.add(HEADER), len) };
    let command_line = std::str::from_utf8(payload)
        .map_err(|error| format!("the command line is not UTF-8: {error}"))?;
    let arguments = command_line
        .split('\n')
        .filter(|argument| !argument.is_empty())
        .map(|argument| argument.to_string())
        .collect::<Vec<String>>();
    if arguments.is_empty() {
        return Err("no command given".to_string());
    }
    Ok(arguments)
}

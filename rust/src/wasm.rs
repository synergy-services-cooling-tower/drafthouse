//! The wasm export surface (issue #24): a C ABI over [`crate::cli`].
//!
//! Built only for `wasm32-unknown-unknown` (see `lib.rs`), as a `cdylib` (see `Cargo.toml`).
//! There is no binding generator and no dependency: the ABI is three functions over the
//! module's own exported memory, and the JavaScript side is `rust/wasm/binding.mjs`, which
//! is data only — it allocates a buffer, writes the command line, reads the reply and
//! parses it.
//!
//! # The ABI
//!
//! Every buffer that crosses the boundary carries its own length: four little-endian bytes
//! (the payload length), then the payload. `ct_free` takes the pointer alone.
//!
//! * `ct_alloc(len) -> ptr` — a buffer with room for `len` payload bytes; the caller writes
//!   them at `ptr + 4`. The header is already filled in.
//! * `ct_call(ptr) -> ptr` — reads the command line from the buffer, **taking ownership of
//!   it** (the buffer is freed before returning), runs it through [`crate::cli::run`], and
//!   returns a new buffer holding the reply envelope ({@link} below).
//! * `ct_free(ptr)` — frees any buffer this module returned, or one that `ct_call` returned
//!   early because its input was unusable.
//!
//! The command line is UTF-8 with one argument per line (a flag, a value, …), which is the
//! transport `rust/wasm/binding.mjs` speaks; a newline cannot occur inside an argument.
//!
//! # The reply envelope
//!
//! * `{"ok":true,"value":<the command's JSON>}`
//! * `{"ok":false,"status":1,"error":{"error":"DomainError","message":…}}`
//! * `{"ok":false,"status":2,"error":{"error":"usage","message":…}}`
//!
//! The status is the exit status the binary reports for the same command, so a caller that
//! already understands `ct-engine` understands the wasm surface, and the parity harness can
//! run its whole suite against either engine without a second mapping.
//!
//! # Ownership and failure
//!
//! The input buffer belongs to the caller until `ct_call` is entered, and the reply buffer
//! belongs to the caller until `ct_free` is called. Null pointers are refused (an empty
//! reply buffer comes back rather than a trap), and nothing in this module panics on input
//! it cannot read: a malformed transport is reported as a usage refusal.

use std::alloc::{alloc, dealloc, handle_alloc_error, Layout};

/// Four little-endian bytes of payload length in front of every buffer.
const HEADER: usize = 4;

/// The alignment every buffer is allocated at: a `u32` header plus bytes.
const ALIGN: usize = 4;

fn layout(total: usize) -> Layout {
    Layout::from_size_align(total, ALIGN).expect("a valid layout")
}

/// Write `payload` into a fresh length-prefixed buffer and hand the pointer to the caller.
fn leak(payload: &[u8]) -> *mut u8 {
    let total = HEADER + payload.len();
    let layout = layout(total);
    // SAFETY: `total` is non-zero (the header alone), so the layout is valid.
    let pointer = unsafe { alloc(layout) };
    if pointer.is_null() {
        handle_alloc_error(layout);
    }
    unsafe {
        // The header is written unaligned on purpose: the caller reads it with a DataView.
        pointer.cast::<u32>().write_unaligned(payload.len() as u32);
        std::ptr::copy_nonoverlapping(payload.as_ptr(), pointer.add(HEADER), payload.len());
    }
    pointer
}

/// The payload length recorded in a buffer's header.
///
/// # Safety
///
/// `pointer` must come from [`leak`] or [`ct_alloc`], or be null.
unsafe fn payload_len(pointer: *const u8) -> usize {
    pointer.cast::<u32>().read_unaligned() as usize
}

/// A buffer with room for `len` payload bytes. The caller writes the payload at `ptr + 4`;
/// the header is written here, so a caller cannot forget it.
#[no_mangle]
pub extern "C" fn ct_alloc(len: u32) -> *mut u8 {
    leak(&vec![0u8; len as usize])
}

/// Free a buffer this module produced, input buffers included.
///
/// # Safety
///
/// `pointer` must come from [`ct_alloc`] or [`ct_call`], or be null, and must not have been
/// freed already.
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
///
/// Takes ownership of the input buffer and returns a new one, which the caller frees with
/// [`ct_free`].
///
/// # Safety
///
/// `pointer` must come from [`ct_alloc`], have its payload written, and not be freed.
#[no_mangle]
pub extern "C" fn ct_call(pointer: *mut u8) -> *mut u8 {
    let reply = match unsafe { read_arguments(pointer) } {
        Ok(arguments) => crate::cli::reply(&arguments),
        Err(message) => format!(
            "{{\"ok\":false,\"status\":2,\"error\":{{\"error\":\"usage\",\"message\":{}}}}}",
            crate::cli::json_string(&message)
        ),
    };
    // The input buffer is dead either way: `ct_call` owns it from here.
    ct_free(pointer);
    leak(reply.as_bytes())
}

/// The command line out of an input buffer: UTF-8, one argument per line.
///
/// # Safety
///
/// `pointer` must come from [`ct_alloc`], have its payload written, and not be freed.
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

# `drafthouse-wasm` · the ABI

The wasm export surface (issue #24): a C ABI over [`crate::cli`].

Built only for `wasm32-unknown-unknown` (see `lib.rs`), as a `cdylib` (see `Cargo.toml`).
There is no binding generator and no dependency: the ABI is three functions over the
module's own exported memory, and the JavaScript side is `tools/parity/binding.mjs`, which
is data only — it allocates a buffer, writes the command line, reads the reply and
parses it.

# The ABI

Every buffer that crosses the boundary carries its own length: four little-endian bytes
(the payload length), then the payload. `ct_free` takes the pointer alone.

* `ct_alloc(len) -> ptr` — a buffer with room for `len` payload bytes; the caller writes
  them at `ptr + 4`. The header is already filled in.
* `ct_call(ptr) -> ptr` — reads the command line from the buffer, **taking ownership of
  it** (the buffer is freed before returning), runs it through [`drafthouse_cli::run`], and
  returns a new buffer holding the reply envelope ({@link} below).
* `ct_free(ptr)` — frees any buffer this module returned, or one that `ct_call` returned
  early because its input was unusable.

The command line is UTF-8 with one argument per line (a flag, a value, …), which is the
transport `tools/parity/binding.mjs` speaks; a newline cannot occur inside an argument.

# The reply envelope

* `{"ok":true,"value":<the command's JSON>}`
* `{"ok":false,"status":1,"error":{"error":"DomainError","message":…}}`
* `{"ok":false,"status":2,"error":{"error":"usage","message":…}}`

The status is the exit status the binary reports for the same command, so a caller that
already understands `ct-engine` understands the wasm surface, and the parity harness can
run its whole suite against either engine without a second mapping.

# Ownership and failure

The input buffer belongs to the caller until `ct_call` is entered, and the reply buffer
belongs to the caller until `ct_free` is called. Null pointers are refused (an empty
reply buffer comes back rather than a trap), and nothing in this module panics on input
it cannot read: a malformed transport is reported as a usage refusal.

## Notes carried from the source comments

### `const HEADER`

Four little-endian bytes of payload length in front of every buffer.

### `const ALIGN`

The alignment every buffer is allocated at: a `u32` header plus bytes.

### `fn leak`

Write `payload` into a fresh length-prefixed buffer and hand the pointer to the caller.

The header is written unaligned on purpose: the caller reads it with a DataView.

### `fn payload_len`

The payload length recorded in a buffer's header.

# Safety

`pointer` must come from [`leak`] or [`ct_alloc`], or be null.

### `fn ct_alloc`

A buffer with room for `len` payload bytes. The caller writes the payload at `ptr + 4`;
the header is written here, so a caller cannot forget it.

### `fn ct_free`

Free a buffer this module produced, input buffers included.

# Safety

`pointer` must come from [`ct_alloc`] or [`ct_call`], or be null, and must not have been
freed already.

### `fn ct_call`

Run one command line (one argument per line) and return the reply envelope.

Takes ownership of the input buffer and returns a new one, which the caller frees with
[`ct_free`].

# Safety

`pointer` must come from [`ct_alloc`], have its payload written, and not be freed.

The input buffer is dead either way: `ct_call` owns it from here.

### `fn read_arguments`

The command line out of an input buffer: UTF-8, one argument per line.

# Safety

`pointer` must come from [`ct_alloc`], have its payload written, and not be freed.

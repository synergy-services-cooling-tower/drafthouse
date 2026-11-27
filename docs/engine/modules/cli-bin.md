# `ct-engine` binary

`ct-engine` — the JSON command surface as a process.

Nothing but the process shell lives here. The commands are
[`synergy_drafthouse::cli`], shared with the wasm export
(`synergy_drafthouse::wasm`), so the two surfaces cannot drift into two parsers.

Exit status, as documented on the module: `0` one JSON object on stdout, `1` a domain
refusal (`{"error":"DomainError","message":…}` on stderr, stdout empty), `2` a usage
error. The parity harness keys off this status, and so does the wasm binding's envelope.

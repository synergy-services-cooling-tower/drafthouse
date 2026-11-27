# `cli` tests · `wasm_engine`

The engine surface the wasm export adds (issue #24): the `inputs` and `worked` keys
`select` emits, and the reply envelope a caller that is not a process reads.

`cli::run` is the engine both the `ct-engine` binary and the wasm export call, so this
data contract is testable natively, without a wasm runtime. The wasm-specific half — the
C ABI (`src/wasm.rs`), the artifact and the JS binding's catalog projection — is
exercised by the parity harness under `--engine wasm` (`tools/parity/run.mjs`).

The records below are one tower, fill, drift eliminator, fan and nozzle from the bundled
catalog, in the `key:value` spec form the CLI parses; the selection they produce is
feasible at the engine's default duty, which is what makes the worked sheet non-empty.

## Notes carried from the source comments

### `const STEP_FIELDS`

The `worked.js` step's field set — the shape a step is data in, not markup.

### `fn select_reports_the_inputs_it_used_including_catalog_dimensions`

The identity the run resolved for the recommended candidate...

...and the catalog dimensions it read out of the records, as values, not as the spec
text they arrived in.

The requirements the run ranked against travel with the inputs.

### `fn worked_steps_follow_the_worked_js_shape_and_are_data_only`

Data, not markup: nothing in the sheet may carry the characters that would make it
renderable, and nothing may build geometry either (the numbers are values, not paths).

### `fn worked_step_values_are_the_candidates_own_numbers`

The sheet's result block and the ranked candidate agree, field for field, on the
quantities both carry.

### `fn the_reply_envelope_carries_the_status_the_binary_would_exit_with`

The envelope is the process's own classification, not a second opinion.

### `fn select_fields_lists_the_field_names_the_selector_accepts`

Every field name the tower spec above carries must be in the list: the projection a
caller applies (the wasm binding's, and this test's) is exactly this intake.

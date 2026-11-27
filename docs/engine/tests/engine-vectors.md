# `engine` tests · `vectors`

Recorded regression vectors — the ported families of `validation/test-vectors.json`.

Mirrors `tests/vectors.test.js`. The vector file declares itself **unvalidated**: it
records what the reference engine computes, not physical truth. This test only proves the
Rust port reproduces it at the same 1e-6 relative tolerance the JavaScript suite uses.

The file's `naturalDraft` and `performanceCurve` families were added by issue #40 slice 1
(additions only; every pre-existing entry is byte-unchanged) together with the extra
`capability` case, so the three concerns that had no Rust counterpart are now checked
against recorded vectors as well as against the live JavaScript engine.

## Notes carried from the source comments

### `const VECTOR_TOLERANCE`

The tolerance `tests/vectors.test.js` uses.

### `fn the_vector_file_declares_itself_unvalidated`

Guards against someone quietly promoting regression vectors to "reference data".

### `const COMBINATIONS`

The four recorded combinations of convention × quadrature.

### `fn merkel_vectors_reproduce_under_both_conventions_and_both_quadratures`

The vector records an infeasible duty; the port must refuse it too.

### `fn the_cti_convention_is_consistently_the_higher_demand`

If this ever flips, the conventions have been swapped somewhere.

/* ---------------- issue #40 slice 1: the three added families ---------------- */

### `fn number_or`

An optional numeric key: absent means the reference's default argument applies.

### `fn vector_condition`

The `capability` family's condition objects.

### `fn vector_fill_vf38`

`sampleCatalog.fills` / `FILM-VF38`.

### `fn vector_tower_idcf_064`

`sampleCatalog.towers` / `IDCF-064`.

### `fn vector_drift_de_3p_10`

`sampleCatalog.driftEliminators` / `DE-3P-10`.

### `fn performance_curve_vectors_reproduce`

The bounds are recorded implicitly through the probes; the probes themselves are the
recorded cases: a direct prediction, and the inverse solve with the capability ratio.

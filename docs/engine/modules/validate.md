# `drafthouse-engine` · `validate`

The fail-closed catalog-record validator (issue #5).

`../docs/CATALOG_SCHEMA.md` describes the component catalog in prose. This module is that
prose as data: for every record type it declares every field, whether the field is
required, and what its value must be. The `ct-engine` CLI validates catalog records with
this table before the selector may rank anything; a future front end (the HTML side
through the WASM kernel) shares the same table instead of growing a second validator.

Fail-closed means each of these is a refusal that names the record and the field:

* a required field is missing from the record;
* a field name the record type does not declare appears — a mistyped name must fail
  loudly instead of becoming an ignored key, which is the defect this module closes;
* a value is not the declared type, or is a non-finite number (`NaN`, `Infinity`, or a
  literal that overflows to infinity), which is the value class that used to flow into a
  ranking;
* a number is outside its declared domain (positive, non-negative), an enumerated value
  is outside its declared set, or a row/list has the wrong length.

The schema is the port's own record shape: the fields `selection.js` feeds the selector,
plus the catalog fields `../docs/CATALOG_SCHEMA.md` declares that the port does not read
(`name`, `draftType`, `effectiveDraftHeightM`, `geometry`, `material`). Fields the schema
does not declare are refused; `README.md` records that decision, including the reference
catalog's commercial fields.

## Notes carried from the source comments

### `enum RecordKind`

The five record types `docs/CATALOG_SCHEMA.md` declares.

### `const ALL`

Every record type, in the schema's own order.

### `fn as_str`

The spelling used in refusals and in the CLI's record options.

### `enum NumberDomain`

What a numeric field's value must satisfy beyond being a finite number.

### `Finite,`

Any finite number (the schema's signed quantities: the fill power-law exponents).

### `Positive,`

A finite number greater than zero.

### `NonNegative,`

A finite number at or above zero.

### `struct Column`

One named numeric column of a `|`-separated row or a `:`-separated curve point.

### `enum FieldKind`

The value shape a field declares.

### `Text,`

A non-empty string.

### `Number(NumberDomain),`

One number.

### `NumberList {`

A `|`-separated list of numbers.

### `TextList { min_len: usize },`

A `|`-separated list of non-empty strings.

### `Enum(&'static [&'static str]),`

A string from a declared set.

### `EnumList {`

A `|`-separated list of strings from a declared set.

### `RatioBracket,`

Two `|`-separated ratios forming an ordered bracket: `lower|upper`, `lower <= upper`.

### `Row(&'static [Column]),`

One `|`-separated row of numeric columns (`a|b|c`).

### `Points {`

`|`-separated points of `:`-separated numeric columns (`a:b:c|a:b:c`).

### `struct FieldRule`

One field of one record type.

### `struct Violation`

One refused field: the record it belongs to, the field, and what is wrong with it.

### `pub record: String,`

`«kind» record «label»`, e.g. `tower record "IDCF-064"`.

### `fn message`

The refusal line: names the record and the field.

### `const CURVE_MIN_POINTS`

`interpolate1D` refuses a curve with fewer than two points, and
`driftPerformanceAtVelocity` interpolates without clamping the ends, so a curve needs a
span: the same minimum the engine's own contract states.

### `const TOWER_FIELDS`

The tower geometry record (schema section 3).

The ordered-layer contract (issue #54): zero or more complete `+`-joined stack variants,
`|`-separated. Each variant is an ordered list of `<fillId>@<depthM>[@<thermal>[@<pressure>]]`
layers, resolved by the CLI's own reader (a layer's fields are not this record's fields).

### `const FILL_FIELDS`

The fill record (schema section 4).

### `const DRIFT_ELIMINATOR_FIELDS`

The drift-eliminator record (schema section 5).

### `const FAN_FIELDS`

The fan record (schema section 6).

The speed the recorded curve is published at (speed ratio 1.0), in rpm. Optional: the
record's own datum, and one this schema never invents.

### `const NOZZLE_FIELDS`

The nozzle record (schema section 7).

### `fn schema`

The machine-enforced schema of one record type.

### `fn validate_record`

Validate one record's `field: value` pairs against the schema of `kind`.

`label` names the record in every refusal — the CLI passes the record's `id` when it has
one, otherwise its position. `Ok(())` means every declared field is present and every
value is in its declared domain; `Err(violations)` names every refused field, so a
caller can report all of them at once.

/* ---------------- the value checks ---------------- */

### `fn number_problem`

What is wrong with `raw` as a value of `domain`, or `None` when the value is acceptable.

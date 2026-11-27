# `cli` tests · `validate`

The catalog-record validator (issue #5): the machine schema over the five record types,
the refusal classes the fail-closed property is about, and the CLI gate — `select`
refuses an invalid catalog with exit 2, naming the record and the field, before anything
reaches the ranking.

## Notes carried from the source comments

### `top`

/* ---------------- records in the `ct-engine` spec encoding ---------------- */

### `fn bundled_tower`

The bundled catalog's `IDCF-064` tower, in the CLI's `key:value` encoding (`|` separates
list entries and a row's columns, `:` separates a curve point's columns).

### `fn bundled_fill`

The bundled catalog's `FILM-OF25` fill.

### `fn bundled_drift_eliminator`

The bundled catalog's `DE-3P-10` drift eliminator.

### `fn bundled_fan`

The bundled catalog's `AX-500` fan.

### `fn bundled_nozzle`

The bundled catalog's `NZ-20` nozzle.

### `fn refusal`

The refusal messages of a record that must not validate, and a panic when one does.

/* ---------------- the schema ---------------- */

### `fn every_record_kind_declares_a_unique_non_empty_schema`

Every kind is reachable through the checker; the match below is exhaustive.

/* ---------------- the bundled records ---------------- */

### `fn a_record_with_only_its_required_fields_also_validates_clean`

/* ---------------- the refusals ---------------- */

### `fn a_reference_catalog_commercial_field_is_refused_as_undeclared`

Requirement 2's decision, pinned: the machine schema does **not** declare the reference
catalog's commercial fields. The port's own guard
(`tests/selection.rs::no_money_ish_identifier_exists_anywhere_in_the_port`) fails if the
spelling appears anywhere under `rust/src`, so a schema that named them would break a
reviewed gate; a record carrying one is refused by name instead of being silently
ignored, and the bundled data itself is never edited (the reference keeps using them).

### `fn out_of_domain_and_out_of_set_values_are_refused`

A negative area is out of its domain.

An enumerated field outside its set.

An enumerated list with a class outside its set.

A zone row with the wrong arity.

A signed exponent column still has to be a number.

An inverted speed-ratio bracket.

A curve below the engine's own two-point minimum.

A zero efficiency in a fan curve point.

/* ---------------- the CLI gate ---------------- */

### `fn minimal_catalog_args`

A catalog with one record per type, required fields only, that the selector can run.

### `fn complete_catalog_args`

The same catalog with every declared-but-unread field present (`name`, `draftType`,
`geometry`, `material`, and the nozzle's optional reference density).

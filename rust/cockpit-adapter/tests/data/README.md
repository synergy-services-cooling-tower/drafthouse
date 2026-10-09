# The recorded cockpit fixture, vendored for the parity gate

`cockpit-fixture.json` is a byte copy of the cockpit's own fixture file:

| | |
|---|---|
| source | the private design pass that produced the cockpit (its `assets/fixture.json`), plus issue #59's `nominalRpm` field on the four `catalog.fans` records, issue #73's rename of the two synthetic layer fills and issue #136's per-fill height spec (see below) |
| design pass commit | `815d832730b944607afc7aac07f5bea610c8c07c` (the round-4 head of that pass) |
| bytes | 97530 |
| sha256 | `73a71ae1b7858b1ee66dcaba3cc6598dee2f9a8c38d0e733d85a93ff8510fa6f` |

The commit and the sha256 above identify the bytes exactly - the cockpit directory's own name is
not part of this file (the engine layer stays free of UI names).

It is test data, never compiled and never read by the crate: `tests/parity.rs` parses it and compares
the adapter's numbers for the recorded duty against the record. Its `anchor` block is the real
engine's own run (`fixtures/engine-run.json`, generated from the shipped wasm build by
that pass's `tools/build-cockpit-fixture.py`); its `catalog` block carries the engine's records.

**Issue #59's addition and issue #73's rename (the adaptations since the copy was taken).** The four
`catalog.fans` records now carry `nominalRpm` - the rated speed the fan's recorded curve is
published at (speed ratio 1.0) - added by the #59 import script, which also
re-emits the file with the design pass's own formatter
(`json.dumps(..., indent=1, ensure_ascii=False) + "\n"`) and refuses to write when any recorded
value moved. No recorded number changed; the four added lines are the whole diff.

Issue #73 then renamed the two synthetic layer fills to `FILM-MF20` and `FILM-WF25` (D24, which
records the old-to-new mapping) with the #73 rename script: it re-emits the
file with the same formatter and refuses when anything beyond the id/name strings and the
`cockpitExtension` note moves; a masked diff of the two fixtures old -> new
with the id/name fields masked and reports 0 numeric changes. No recorded number moved in #73
either.

The `bytes`/`sha256` pair above is the file's measured identity, re-recorded with each re-cut and
never carried over: the #59 record kept a pre-#59 byte count (96807 against that re-cut's 96816
file) and #60's `provenance.engine` re-point moved the bytes again (96817), so both figures were
stale by issue #73's base; they are replaced here by the measured 96763 / `2302f119...` of the
re-emitted file.

**Issue #136's addition.** Each of the seven `catalog.fills` records now carries its own height spec,
`depth: { moduleM, minM, maxM, source: "illustrative" }` (docs/CATALOG_SCHEMA.md §4), added by
the issue #136 lane's depth-spec script with the same formatter; the script refuses to write when
any value other than the added `depth` keys moves, and writes the same spec into the cockpit's own
`assets/fixture.json`. No recorded number moved. The cockpit's contract crate reads this copy in its
schema test (`every_shipped_fill_record_has_a_height_spec`), so the two copies cannot drift apart
silently. The figures above are the measured 97530 / `73a71ae1...` of that re-emitted file.

Refresh: copy the file again from the named commit and update the two figures above (a copy that no
longer matches is a lane decision, not a silent swap). Do not edit it in place - the numbers in it
are the record.

//! The fixture-file type the copied contract's optional authoring hook names.
//!
//! [`crate::engine::Engine::fixture_catalog_mut`] returns `Option<&mut FixtureFile>`: the cockpit's
//! baseline implementation hands out its loaded fixture file so the curve editor can retune the
//! sampled fill tables in place, and the contract's own doc comment records that the real engine
//! returns `None` instead - catalog authoring belongs to its server - which is what the UI then
//! reads: the fill-curve controls hide instead of pretending they do something.
//!
//! [`crate::RealEngine`] takes the default method, so it never constructs one, and this declaration
//! exists so that the verbatim contract copy resolves `crate::fixture_engine::FixtureFile`. It is
//! deliberately opaque: restating the cockpit's fixture-file schema here would be a second copy of a
//! record this crate does not read, and a silently-wrong stand-in besides. The cockpit's own type
//! stays with the cockpit (`cockpit/src/fixture_engine.rs` in the designer's checkout).

/// The cockpit's fixture file. Never constructed in this crate; see the module docs.
#[derive(Clone, Debug, PartialEq)]
pub struct FixtureFile;

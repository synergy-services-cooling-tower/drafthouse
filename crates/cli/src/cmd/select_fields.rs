use crate::json::{json_array, object};
use crate::json_string;
use crate::records::{
    SELECT_DRIFT_FIELDS, SELECT_FAN_FIELDS, SELECT_FILL_FIELDS, SELECT_NOZZLE_FIELDS,
    SELECT_TOWER_FIELDS,
};

pub(crate) fn select_fields_object() -> String {
    fn names(fields: &[&str]) -> String {
        json_array(
            &fields
                .iter()
                .map(|field| json_string(field))
                .collect::<Vec<String>>(),
        )
    }
    object(&[
        ("towers", names(&SELECT_TOWER_FIELDS)),
        ("fills", names(&SELECT_FILL_FIELDS)),
        ("driftEliminators", names(&SELECT_DRIFT_FIELDS)),
        ("fans", names(&SELECT_FAN_FIELDS)),
        ("nozzles", names(&SELECT_NOZZLE_FIELDS)),
    ])
}

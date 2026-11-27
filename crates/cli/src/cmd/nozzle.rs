use crate::args::{parse_number, record_list, Cli};
use crate::json::{json_array, object};
use crate::records::selection_nozzle;
use crate::selection::nozzle_arrangement_object;
use crate::Failure;
use drafthouse_engine::{NozzleArrangementInput, NozzleRecord};

pub(crate) fn nozzle(cli: &Cli) -> Result<String, Failure> {
    let nozzles = record_list(cli, "--nozzles")?
        .iter()
        .map(selection_nozzle)
        .collect::<Result<Vec<NozzleRecord>, Failure>>()?;
    let mut input = NozzleArrangementInput::new(
        cli.required_number("--flow")?,
        &nozzles,
        cli.required_number("--dp")?,
    );
    if let Some(raw) = cli.values.get("--count-range") {
        let parts: Vec<&str> = raw.split('|').collect();
        let [lower, upper] = parts[..] else {
            return Err(Failure::Usage(format!(
                "--count-range expects lower|upper, got {raw:?}"
            )));
        };
        input = input.with_target_count_range([
            parse_number(lower, "--count-range")?,
            parse_number(upper, "--count-range")?,
        ]);
    }
    let options = drafthouse_engine::select_nozzle_arrangement(&input)?;
    Ok(object(&[(
        "options",
        json_array(
            &options
                .iter()
                .map(nozzle_arrangement_object)
                .collect::<Vec<String>>(),
        ),
    )]))
}

use crate::args::Cli;
use crate::json::object;
use crate::records::{validate_catalog_records, CATALOG_LISTS};
use crate::Failure;

pub(crate) fn validate_catalog_command(cli: &Cli) -> Result<String, Failure> {
    let counts = validate_catalog_records(cli)?;
    let mut pairs: Vec<(&str, String)> = CATALOG_LISTS
        .iter()
        .zip(counts)
        .map(|(list, count)| (list.key, count.to_string()))
        .collect();
    pairs.push(("records", counts.iter().sum::<usize>().to_string()));
    Ok(object(&pairs))
}

use crate::json_string;

pub(crate) fn fixed(value: f64, digits: usize) -> String {
    format!("{value:.digits$}")
}

pub(crate) fn number(value: f64) -> String {
    if value.is_finite() {
        format!("{value}")
    } else {
        "null".to_string()
    }
}

pub(crate) fn optional_number(value: Option<f64>) -> String {
    match value {
        Some(value) => number(value),
        None => "null".to_string(),
    }
}

pub(crate) fn json_string_array(values: &[String]) -> String {
    let body = values
        .iter()
        .map(|value| json_string(value))
        .collect::<Vec<_>>()
        .join(",");
    format!("[{body}]")
}

pub(crate) fn json_array(items: &[String]) -> String {
    format!("[{}]", items.join(","))
}

pub(crate) fn object(pairs: &[(&str, String)]) -> String {
    let body = pairs
        .iter()
        .map(|(key, value)| format!("\"{key}\":{value}"))
        .collect::<Vec<_>>()
        .join(",");
    format!("{{{body}}}")
}

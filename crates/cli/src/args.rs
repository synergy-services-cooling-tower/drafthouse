use crate::Failure;
use drafthouse_engine::{InletEnthalpyConvention, Integration};
use std::collections::{HashMap, HashSet};

pub(crate) struct Cli {
    pub(crate) values: HashMap<String, String>,
    pub(crate) switches: HashSet<String>,
}

impl Cli {
    pub(crate) fn parse(args: &[String]) -> Result<(String, Self), Failure> {
        let mut values = HashMap::new();
        let mut switches = HashSet::new();
        let mut command = None;
        let mut arguments = args.iter().peekable();
        while let Some(argument) = arguments.next() {
            if let Some(key) = argument.strip_prefix("--") {
                match arguments.next_if(|value| !value.starts_with("--")) {
                    Some(value) => {
                        values.insert(format!("--{key}"), value.clone());
                    }
                    None => {
                        switches.insert(format!("--{key}"));
                    }
                }
            } else if command.is_none() {
                command = Some(argument.clone());
            } else {
                return Err(Failure::Usage(format!("unexpected argument: {argument}")));
            }
        }
        match command {
            Some(command) => Ok((command, Self { values, switches })),
            None => Err(Failure::Usage("no command given".to_string())),
        }
    }

    pub(crate) fn has(&self, name: &str) -> bool {
        self.switches.contains(name)
    }

    pub(crate) fn number(&self, name: &str) -> Result<Option<f64>, Failure> {
        match self.values.get(name) {
            None => Ok(None),
            Some(raw) => raw
                .parse::<f64>()
                .map(Some)
                .map_err(|_| Failure::Usage(format!("{name} expects a number, got {raw:?}"))),
        }
    }

    pub(crate) fn integer(&self, name: &str, default: usize) -> Result<usize, Failure> {
        match self.values.get(name) {
            None => Ok(default),
            Some(raw) => raw
                .parse::<usize>()
                .map_err(|_| Failure::Usage(format!("{name} expects an integer, got {raw:?}"))),
        }
    }

    pub(crate) fn required_number(&self, name: &str) -> Result<f64, Failure> {
        self.number(name)?
            .ok_or_else(|| Failure::Usage(format!("missing required option {name}")))
    }

    pub(crate) fn number_or(&self, name: &str, default: f64) -> Result<f64, Failure> {
        Ok(self.number(name)?.unwrap_or(default))
    }

    pub(crate) fn integration(&self) -> Result<Integration, Failure> {
        self.integration_default(Integration::Simpson)
    }

    pub(crate) fn integration_default(&self, default: Integration) -> Result<Integration, Failure> {
        match self.values.get("--integration").map(String::as_str) {
            None => Ok(default),
            Some("simpson") => Ok(Integration::Simpson),
            Some("chebyshev4") => Ok(Integration::Chebyshev4),
            Some(other) => Err(Failure::Usage(format!(
                "--integration expects simpson or chebyshev4, got {other:?}"
            ))),
        }
    }

    pub(crate) fn convention(&self) -> Result<InletEnthalpyConvention, Failure> {
        match self.values.get("--convention").map(String::as_str) {
            None | Some("bulk") => Ok(InletEnthalpyConvention::Bulk),
            Some("cti-saturated-wetbulb") => Ok(InletEnthalpyConvention::CtiSaturatedWetBulb),
            Some(other) => Err(Failure::Usage(format!(
                "--convention expects bulk or cti-saturated-wetbulb, got {other:?}"
            ))),
        }
    }
}

pub(crate) struct Spec {
    pub(crate) pairs: Vec<(String, String)>,
    pub(crate) option: String,
}

impl Spec {
    pub(crate) fn parse(cli: &Cli, option: &str) -> Result<Self, Failure> {
        match cli.values.get(option) {
            None => Ok(Self {
                pairs: Vec::new(),
                option: option.to_string(),
            }),
            Some(raw) => Self::parse_text(option, raw),
        }
    }

    pub(crate) fn parse_text(option: &str, raw: &str) -> Result<Self, Failure> {
        let mut pairs = Vec::new();
        if !raw.is_empty() {
            for part in raw.split(',') {
                let (key, value) = part.split_once(':').ok_or_else(|| {
                    Failure::Usage(format!("{option} expects key:value pairs, got {part:?}"))
                })?;
                pairs.push((key.to_string(), value.to_string()));
            }
        }
        Ok(Self {
            pairs,
            option: option.to_string(),
        })
    }

    pub(crate) fn get(&self, key: &str) -> Option<&str> {
        self.pairs
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    }

    pub(crate) fn list(&self, key: &str) -> Vec<String> {
        match self.get(key) {
            None => Vec::new(),
            Some(raw) => raw.split('|').map(str::to_string).collect(),
        }
    }

    pub(crate) fn required_list(&self, key: &str) -> Result<Vec<String>, Failure> {
        let values = self.list(key);
        if values.is_empty() {
            return Err(Failure::Usage(format!("{} needs {key}", self.option)));
        }
        Ok(values)
    }

    pub(crate) fn required_text(&self, key: &str) -> Result<String, Failure> {
        self.get(key)
            .map(str::to_string)
            .ok_or_else(|| Failure::Usage(format!("{} needs {key}", self.option)))
    }

    pub(crate) fn number(&self, key: &str) -> Result<Option<f64>, Failure> {
        match self.get(key) {
            None => Ok(None),
            Some(raw) => raw.parse::<f64>().map(Some).map_err(|_| {
                Failure::Usage(format!(
                    "{} {key} expects a number, got {raw:?}",
                    self.option
                ))
            }),
        }
    }

    pub(crate) fn required(&self, key: &str) -> Result<f64, Failure> {
        self.number(key)?
            .ok_or_else(|| Failure::Usage(format!("{} needs {key}", self.option)))
    }

    pub(crate) fn ensure_known(&self, allowed: &[&str]) -> Result<(), Failure> {
        for (key, _) in &self.pairs {
            if !allowed.contains(&key.as_str()) {
                return Err(Failure::Usage(format!(
                    "{} does not know the field {key:?}",
                    self.option
                )));
            }
        }
        Ok(())
    }
}

pub(crate) fn record_list(cli: &Cli, option: &str) -> Result<Vec<Spec>, Failure> {
    let Some(raw) = cli.values.get(option) else {
        return Err(Failure::Usage(format!("select needs {option}")));
    };
    raw.split(';')
        .map(|entry| Spec::parse_text(option, entry))
        .collect()
}

pub(crate) fn parse_number(raw: &str, option: &str) -> Result<f64, Failure> {
    raw.parse::<f64>()
        .map_err(|_| Failure::Usage(format!("{option} expects a number, got {raw:?}")))
}

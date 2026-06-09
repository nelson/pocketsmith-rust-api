//! Hand-rolled argument parsing (matching the repo's `bin/normalise.rs`
//! style — no `clap`). One `Flags` carries every parsed option; the
//! command functions read from it.

use std::collections::HashMap;

use pocketsmith_sync::rules::model::RuleError;
use pocketsmith_sync::rules::Stage;

use crate::AppError;

pub(crate) const VALUE_FLAGS: &[&str] =
    &["pattern", "canonical", "operation", "gateway", "institution", "kind", "note"];

pub(crate) struct Flags {
    pub(crate) stage: Option<String>,
    pub(crate) id: Option<i64>,
    pub(crate) json: bool,
    pub(crate) apply: bool,
    pub(crate) force: bool,
    pub(crate) quiet: bool,
    pub(crate) all: bool,
    pub(crate) before: Option<i64>,
    pub(crate) after: Option<i64>,
    pub(crate) values: HashMap<String, String>,
    pub(crate) features: HashMap<String, bool>,
    pub(crate) positionals: Vec<String>,
}

impl Flags {
    pub(crate) fn parse(args: &[String]) -> Result<Flags, AppError> {
        let mut f = Flags {
            stage: None,
            id: None,
            json: false,
            apply: false,
            force: false,
            quiet: false,
            all: false,
            before: None,
            after: None,
            values: HashMap::new(),
            features: HashMap::new(),
            positionals: Vec::new(),
        };
        let mut i = 0;
        while i < args.len() {
            let a = args[i].as_str();
            let take_val = |name: &str, i: &mut usize| -> Result<String, AppError> {
                *i += 1;
                args.get(*i)
                    .cloned()
                    .ok_or_else(|| AppError::usage(format!("--{name} requires a value")))
            };
            match a {
                "--json" => f.json = true,
                "--apply" | "-a" => f.apply = true,
                "--force" | "-f" => f.force = true,
                "--quiet" => f.quiet = true,
                "--all" => f.all = true,
                "-af" | "-fa" => {
                    f.apply = true;
                    f.force = true;
                }
                "--stage" => f.stage = Some(take_val("stage", &mut i)?),
                "--id" => {
                    let v = take_val("id", &mut i)?;
                    f.id = Some(v.parse().map_err(|_| {
                        AppError::usage(format!("--id must be an integer, got {v:?}"))
                    })?);
                }
                "--before" => {
                    let v = take_val("before", &mut i)?;
                    f.before = Some(v.parse().map_err(|_| {
                        AppError::usage(format!("--before must be an integer, got {v:?}"))
                    })?);
                }
                "--after" => {
                    let v = take_val("after", &mut i)?;
                    f.after = Some(v.parse().map_err(|_| {
                        AppError::usage(format!("--after must be an integer, got {v:?}"))
                    })?);
                }
                _ if a.starts_with("--has-") => {
                    let feat = a.trim_start_matches("--has-").to_string();
                    f.features.insert(feat, true);
                }
                _ if a.starts_with("--no-") => {
                    // --no-currency-code etc. Normalise to the feature stem.
                    let feat = a.trim_start_matches("--no-").to_string();
                    f.features.insert(feat, false);
                }
                _ if a.starts_with("--") => {
                    let name = a.trim_start_matches("--").to_string();
                    if VALUE_FLAGS.contains(&name.as_str()) {
                        let v = take_val(&name, &mut i)?;
                        f.values.insert(name, v);
                    } else {
                        return Err(AppError::usage(format!("unknown flag {a}")));
                    }
                }
                _ => f.positionals.push(a.to_string()),
            }
            i += 1;
        }
        Ok(f)
    }

    pub(crate) fn stage(&self) -> Result<Stage, AppError> {
        let name = self
            .stage
            .as_deref()
            .ok_or_else(|| AppError::usage("--stage <name> is required"))?;
        Stage::from_name(name).ok_or_else(|| RuleError::UnknownStage(name.to_string()).into())
    }

    pub(crate) fn require_id(&self) -> Result<i64, AppError> {
        self.id.ok_or_else(|| AppError::usage("--id <id> is required"))
    }
}

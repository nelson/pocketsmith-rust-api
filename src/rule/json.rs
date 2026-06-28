//! Machine-readable (`--json`) output payloads. Builders return
//! `serde_json::Value`; `render` prints them. Keeping the schema here
//! (separate from the human text) makes the scriptable contract explicit.

use serde_json::{json, Value};

use pocketsmith::rules::impact::{BucketCount, Buckets};
use pocketsmith::rules::model::{MoveTarget, Mutation, Rule};
use pocketsmith::rules::{crud, CommitResult, Stage};

pub(crate) fn list(stage: Stage, rules: &[Rule]) -> Value {
    let arr: Vec<Value> = rules.iter().map(rule_summary).collect();
    json!({ "stage": stage.name(), "ordered": crud::is_movable(stage), "rules": arr })
}

fn rule_summary(r: &Rule) -> Value {
    json!({
        "id": r.id,
        "canonical": r.data.canonical(),
        "pattern": r.data.pattern(),
        "note": r.data.note(),
    })
}

pub(crate) fn show(
    stage: Stage,
    rule: &Rule,
    created: Option<String>,
    updated: Option<String>,
) -> Value {
    json!({
        "stage": stage.name(),
        "id": rule.id,
        "canonical": rule.data.canonical(),
        "pattern": rule.data.pattern(),
        "note": rule.data.note(),
        "created_at": created,
        "updated_at": updated,
    })
}

pub(crate) fn evaluate(stage: Stage, mutation: &Mutation, buckets: &Buckets) -> Value {
    let buckets_json = match buckets {
        Buckets::FirstMatch { newly_matched, stolen, new_fallthrough, unchanged_payees } => json!({
            "newly_matched": bucket(newly_matched),
            "stolen": bucket(stolen),
            "new_fallthrough": bucket(new_fallthrough),
            "unchanged": { "payees": unchanged_payees },
        }),
        Buckets::Loop { newly_affected, no_longer_affected, unchanged_payees } => json!({
            "newly_affected": bucket(newly_affected),
            "no_longer_affected": bucket(no_longer_affected),
            "unchanged": { "payees": unchanged_payees },
        }),
    };
    json!({
        "mode": "evaluate",
        "committed": false,
        "stage": stage.name(),
        "mutation": mutation_json(mutation),
        "buckets": buckets_json,
        "dirty_payees": buckets.changed_payees(),
    })
}

fn bucket(b: &BucketCount) -> Value {
    let samples: Vec<Value> = b
        .samples
        .iter()
        .map(|s| {
            json!({
                "original_payee": s.original_payee,
                "txns": s.txn_count,
                "total_cents": s.total_cents,
                "account": s.account,
                "was": s.was,
                "now": s.now,
            })
        })
        .collect();
    json!({ "payees": b.payees, "txns": b.txns, "total_cents": b.total_cents, "samples": samples })
}

fn mutation_json(mutation: &Mutation) -> Value {
    match mutation {
        Mutation::Add(d) => {
            json!({ "kind": "add", "canonical": d.canonical(), "pattern": d.pattern() })
        }
        Mutation::Edit { id, data } => json!({
            "kind": "edit", "id": id, "canonical": data.canonical(), "pattern": data.pattern(),
        }),
        Mutation::Delete { id, .. } => json!({ "kind": "delete", "id": id }),
        Mutation::Move { id, target, .. } => {
            let (k, a) = match target {
                MoveTarget::Before(a) => ("before", a),
                MoveTarget::After(a) => ("after", a),
            };
            json!({ "kind": "move", "id": id, k: a })
        }
    }
}

pub(crate) fn apply(stage: Stage, res: &CommitResult) -> Value {
    let mut obj = json!({
        "mode": "apply",
        "committed": true,
        "new_id": res.new_id,
        "stage": stage.name(),
        "change": res.change,
        "dumped": format!("rules/{}.sql", stage.name()),
        "dirty_payees": res.dirty_payees,
    });
    if res.new_id.is_none() {
        obj.as_object_mut().unwrap().remove("new_id");
    }
    obj
}

/// Tester result as JSON (only the match/miss cases; a syntax error is
/// surfaced as an `AppError` envelope on stderr by the caller).
pub(crate) fn test_match(canonical: &str, span: Option<(usize, usize)>) -> Value {
    json!({ "matches": true, "canonical": canonical, "span": span.map(|(s, e)| [s, e]) })
}

pub(crate) fn test_miss() -> Value {
    json!({ "matches": false })
}

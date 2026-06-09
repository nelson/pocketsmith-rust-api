//! One `cmd_*` per verb. Each is thin orchestration: open the DB, call
//! the library, hand off to `render`. No rule semantics live here.

use rusqlite::Connection;

use pocketsmith_sync::rules::impact;
use pocketsmith_sync::rules::model::{Mutation, MoveTarget, RuleError};
use pocketsmith_sync::rules::validate::validate_draft;
use pocketsmith_sync::rules::{commit, crud, rules_dir, DumpPolicy, Stage};

use crate::args::Flags;
use crate::helpers::{build_rule_data, open_db, timestamps};
use crate::{render, AppError};

// --- reads -----------------------------------------------------------------

pub(crate) fn cmd_list(flags: &Flags) -> Result<(), AppError> {
    let stage = flags.stage()?;
    let conn = open_db()?;
    let rules = crud::list(&conn, stage)?;
    render::list(flags, stage, &rules);
    Ok(())
}

pub(crate) fn cmd_show(flags: &Flags) -> Result<(), AppError> {
    let stage = flags.stage()?;
    let id = flags.require_id()?;
    let conn = open_db()?;
    let rule = crud::get(&conn, stage, id)?.ok_or(RuleError::NotFound { stage, id })?;
    let (created, updated) = timestamps(&conn, stage, id);
    render::show(flags, stage, &rule, created, updated);
    Ok(())
}

pub(crate) fn cmd_test(flags: &Flags) -> Result<(), AppError> {
    let stage = flags.stage()?;
    let input = flags
        .positionals
        .first()
        .ok_or_else(|| AppError::usage("a test string positional is required"))?;
    let candidate = build_rule_data(stage, flags, None)?;
    let conn = open_db()?;
    let result = impact::test_one(&conn, stage, &candidate, input);
    render::test(flags, &result, input)
}

// --- evaluate / apply ------------------------------------------------------

pub(crate) fn cmd_add(flags: &Flags) -> Result<(), AppError> {
    let stage = flags.stage()?;
    let data = build_rule_data(stage, flags, None)?;
    validate_draft(&data)?;
    evaluate_or_apply(flags, stage, Mutation::Add(data))
}

pub(crate) fn cmd_edit(flags: &Flags) -> Result<(), AppError> {
    let stage = flags.stage()?;
    let id = flags.require_id()?;
    let conn = open_db()?;
    let existing = crud::get(&conn, stage, id)?.ok_or(RuleError::NotFound { stage, id })?;
    let data = build_rule_data(stage, flags, Some(&existing.data))?;
    validate_draft(&data)?;
    evaluate_or_apply_conn(conn, flags, stage, Mutation::Edit { id, data })
}

pub(crate) fn cmd_rm(flags: &Flags) -> Result<(), AppError> {
    let stage = flags.stage()?;
    let id = flags.require_id()?;
    let conn = open_db()?;
    // Must exist to evaluate/apply.
    crud::get(&conn, stage, id)?.ok_or(RuleError::NotFound { stage, id })?;
    if flags.apply && !flags.force {
        return Err(AppError::usage("deleting a rule requires --force (-f)"));
    }
    evaluate_or_apply_conn(conn, flags, stage, Mutation::Delete { stage, id })
}

pub(crate) fn cmd_move(flags: &Flags) -> Result<(), AppError> {
    let stage = flags.stage()?;
    let id = flags.require_id()?;
    let target = match (flags.before, flags.after) {
        (Some(a), None) => MoveTarget::Before(a),
        (None, Some(a)) => MoveTarget::After(a),
        (None, None) => return Err(AppError::usage("move requires --before <id> or --after <id>")),
        (Some(_), Some(_)) => return Err(AppError::usage("use only one of --before / --after")),
    };
    evaluate_or_apply(flags, stage, Mutation::Move { stage, id, target })
}

fn evaluate_or_apply(flags: &Flags, stage: Stage, mutation: Mutation) -> Result<(), AppError> {
    let conn = open_db()?;
    evaluate_or_apply_conn(conn, flags, stage, mutation)
}

fn evaluate_or_apply_conn(
    conn: Connection,
    flags: &Flags,
    stage: Stage,
    mutation: Mutation,
) -> Result<(), AppError> {
    if flags.apply {
        let res = commit::commit(&conn, &mutation, DumpPolicy::Sync(rules_dir()), None)?;
        render::apply(flags, stage, &res);
    } else {
        let payees = impact::load_payees(&conn)?;
        let buckets = impact::compute_buckets(&conn, stage, &mutation, &payees)?;
        render::evaluate(flags, stage, &mutation, &buckets);
    }
    Ok(())
}

//! Integration tests for the rule-editing library core + the `rule`
//! binary (rule-cli §10). The keystone (§10.0) exercises the whole
//! lifecycle hermetically; the binary tests pin the scriptable contract
//! (exit codes, JSON schema) by invoking the real `rule` executable
//! against an isolated DB + rules dir.

use std::path::{Path, PathBuf};
use std::process::Command;

use rusqlite::Connection;

use pocketsmith_sync::db;
use pocketsmith_sync::rules::crud;
use pocketsmith_sync::rules::impact::{self, Buckets};
use pocketsmith_sync::rules::model::{Mutation, RuleData};
use pocketsmith_sync::rules::{commit, dump_stage_to_string, DumpPolicy, Stage};

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn manifest_rules() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("rules")
}

fn unique_tmp(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!(
        "rule-it-{tag}-{}-{:?}-{n}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    std::fs::create_dir_all(&p).unwrap();
    p
}

/// Copy all eight committed seeds into `dir` so a cold DB can be seeded
/// from it (used by the subprocess `Cli` harness).
fn copy_seeds(dir: &Path) {
    let src = manifest_rules();
    for stage in Stage::all() {
        let name = format!("{}.sql", stage.name());
        std::fs::copy(src.join(&name), dir.join(&name)).unwrap();
    }
}

/// A fresh, unique temp dir for an injected `DumpPolicy::Sync` — no global
/// state, so these in-process tests run fully in parallel.
fn dump_dir() -> PathBuf {
    unique_tmp("dump")
}

fn merchant(canonical: &str, pattern: &str) -> RuleData {
    RuleData::Merchant { canonical: canonical.into(), pattern: pattern.into(), note: None }
}

// ---------------------------------------------------------------------------
// §10.0 — the keystone hermetic test (one function: old → modify → new)
// ---------------------------------------------------------------------------

#[test]
fn edit_a_rule_end_to_end_in_memory() {
    // 1. OLD RULES: schema-only DB, seed a tiny hermetic rule set in-code.
    let conn = db::initialize_in_memory().unwrap();
    let id = crud::insert_rule(&conn, &merchant("Uber", "(?i)UBER")).unwrap();
    seed_txn(&conn, 1, "UBER TRIP SYDNEY");

    // 2. EVALUATE the modification (pure, writes nothing). Narrow the
    //    Uber rule so "UBER TRIP" no longer matches → fallthrough.
    let m = Mutation::Edit { id, data: merchant("Uber", r"(?i)UBERX") };
    let payees = impact::load_payees(&conn).unwrap();
    let buckets = impact::compute_buckets(&conn, Stage::Merchants, &m, &payees).unwrap();
    match &buckets {
        Buckets::FirstMatch { new_fallthrough, .. } => {
            assert_eq!(new_fallthrough.payees, 1, "UBER TRIP no longer matches");
        }
        _ => panic!("merchants is first-match"),
    }
    // The committed rule is still the OLD pattern (evaluate wrote nothing).
    assert_eq!(
        crud::get(&conn, Stage::Merchants, id).unwrap().unwrap().data.pattern(),
        Some("(?i)UBER")
    );

    // 3. APPLY (single atomic commit), then READ THE NEW RULES back.
    let res = commit::commit(&conn, &m, DumpPolicy::Sync(dump_dir()), None).unwrap();
    assert_eq!(res.change, "~ edited Uber  (?i)UBER → (?i)UBERX");
    assert_eq!(
        crud::get(&conn, Stage::Merchants, id).unwrap().unwrap().data.pattern(),
        Some("(?i)UBERX"),
        "NEW pattern persisted"
    );
}

fn seed_txn(conn: &Connection, id: i64, original_payee: &str) {
    pocketsmith_sync::test_support::seed_account(conn, 1, "Acct").ok();
    pocketsmith_sync::test_support::seed_txn(conn, id, 1, original_payee, original_payee).unwrap();
}

// ---------------------------------------------------------------------------
// §10.2 — add / edit / rm / move applied through the library
// ---------------------------------------------------------------------------

#[test]
fn add_apply_inserts_and_redumps() {
    let dir = dump_dir();
    let conn = db::initialize_in_memory().unwrap();
    let m = Mutation::Add(merchant("Bunnings", "(?i)BUNNINGS"));
    let res = commit::commit(&conn, &m, DumpPolicy::Sync(dir.clone()), None).unwrap();
    let id = res.new_id.unwrap();
    assert_eq!(
        crud::get(&conn, Stage::Merchants, id).unwrap().unwrap().data.pattern(),
        Some("(?i)BUNNINGS")
    );
    let dumped = std::fs::read_to_string(dir.join("merchants.sql")).unwrap();
    assert!(dumped.contains("(?i)BUNNINGS"), "re-dumped merchants.sql must contain the rule");
}

#[test]
fn move_apply_changes_apply_order_and_live_pipeline() {
    let conn = db::initialize_in_memory().unwrap();
    // Two expansions; order matters for a payee both could touch.
    let first = crud::insert_rule(
        &conn,
        &RuleData::Expansion { pattern: "WLW".into(), canonical: "ALPHA".into(), note: None },
    )
    .unwrap();
    let second = crud::insert_rule(
        &conn,
        &RuleData::Expansion { pattern: "WLW".into(), canonical: "BETA".into(), note: None },
    );
    // Duplicate pattern is rejected by UNIQUE — use a distinct one.
    assert!(second.is_err());
    let second = crud::insert_rule(
        &conn,
        &RuleData::Expansion { pattern: "MKT".into(), canonical: "MARKET".into(), note: None },
    )
    .unwrap();

    let order: Vec<i64> =
        crud::list(&conn, Stage::Expansions).unwrap().iter().map(|r| r.id).collect();
    assert_eq!(order, vec![first, second]);

    // Move second before first; apply order flips.
    let m = Mutation::Move {
        stage: Stage::Expansions,
        id: second,
        target: pocketsmith_sync::rules::model::MoveTarget::Before(first),
    };
    commit::commit(&conn, &m, DumpPolicy::Sync(dump_dir()), None).unwrap();
    let order: Vec<i64> =
        crud::list(&conn, Stage::Expansions).unwrap().iter().map(|r| r.id).collect();
    assert_eq!(order, vec![second, first]);
}

// ---------------------------------------------------------------------------
// §1.2 / §10.2 — CLI vs serve commit parity (no divergence)
// ---------------------------------------------------------------------------

#[test]
fn sync_and_background_commits_are_byte_identical() {
    let m = Mutation::Add(merchant("Bunnings", "(?i)BUNNINGS"));

    // DB A: CLI-style synchronous dump into an injected temp dir.
    let conn_a = db::initialize_in_memory().unwrap();
    commit::commit(&conn_a, &m, DumpPolicy::Sync(dump_dir()), None).unwrap();

    // DB B: serve-style background dump + warm cache invalidation.
    let conn_b = db::initialize_in_memory().unwrap();
    let cache = pocketsmith_sync::normalise::RuleCache::new();
    commit::commit(
        &conn_b,
        &m,
        DumpPolicy::Background { db_path: ":memory:".into() },
        Some(&cache),
    )
    .unwrap();

    // The rule_* rows are identical regardless of dump policy.
    let rows_a = crud::list(&conn_a, Stage::Merchants).unwrap();
    let rows_b = crud::list(&conn_b, Stage::Merchants).unwrap();
    assert_eq!(rows_a, rows_b, "committed rows must not diverge by policy");

    // The canonical dump output is byte-identical once the per-row
    // wall-clock timestamps (which legitimately differ between two
    // separate inserts) are pinned — the *content* never diverges by
    // dump policy.
    for c in [&conn_a, &conn_b] {
        c.execute(
            "UPDATE rule_merchants SET created_at='2026-01-01T00:00:00.000Z', \
             updated_at='2026-01-01T00:00:00.000Z'",
            [],
        )
        .unwrap();
    }
    let dump_a = dump_stage_to_string(&conn_a, Stage::Merchants).unwrap();
    let dump_b = dump_stage_to_string(&conn_b, Stage::Merchants).unwrap();
    assert_eq!(dump_a, dump_b, ".sql output must not diverge by policy");
}

// ---------------------------------------------------------------------------
// Binary-driven tests: exit codes + JSON schema (§7, §10.2)
// ---------------------------------------------------------------------------

struct Cli {
    db: PathBuf,
    rules_dir: PathBuf,
}

impl Cli {
    /// Set up an isolated file DB seeded from a temp rules dir, plus a few
    /// transactions, by driving the binary + direct SQL (no parent env
    /// mutation — the subprocess gets its env via `Command::env`).
    fn new() -> Cli {
        let dir = unique_tmp("cli");
        let rules_dir = dir.join("rules");
        std::fs::create_dir_all(&rules_dir).unwrap();
        copy_seeds(&rules_dir);
        let db = dir.join("app.db");
        let cli = Cli { db, rules_dir };
        // Seed the DB (rule tables) by running a read command.
        cli.run(&["list", "--stage", "prefixes", "--json"]);
        // Seed a couple of transactions directly (needs an operation row
        // for the change trigger).
        let conn = Connection::open(&cli.db).unwrap();
        conn.execute_batch(
            "INSERT INTO transaction_accounts(id,name) VALUES (1,'CommBank');
             INSERT INTO _operations(reason) VALUES ('seed');
             INSERT INTO _current_operation(id) VALUES (last_insert_rowid());
             INSERT INTO transactions(id,transaction_account_id,date,amount,original_payee,payee)
               VALUES (1,1,'2026-01-01',-44.10,'ZZQNOVELSHOP SYDNEY','ZZQNOVELSHOP SYDNEY'),
                      (2,1,'2026-01-02',-9.80,'ZZQNOVELSHOP NORTH','ZZQNOVELSHOP NORTH');
             DELETE FROM _current_operation;",
        )
        .unwrap();
        cli
    }

    fn run(&self, args: &[&str]) -> Output {
        // Unified binary: invoke the `rule` subcommand of `pocketsmith`.
        let out = Command::new(env!("CARGO_BIN_EXE_pocketsmith"))
            .arg("rule")
            .args(args)
            .env("POCKETSMITH_DB", &self.db)
            .env("POCKETSMITH_RULES_DIR", &self.rules_dir)
            .env_remove("NO_COLOR")
            .output()
            .expect("run pocketsmith rule binary");
        Output {
            code: out.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        }
    }
}

struct Output {
    code: i32,
    stdout: String,
    stderr: String,
}

#[test]
fn evaluate_json_has_stable_schema_and_exit_zero() {
    let cli = Cli::new();
    let out = cli.run(&[
        "add", "--stage", "merchants", "--pattern", "(?i)ZZQNOVELSHOP", "--canonical", "Novel Shop",
        "--json",
    ]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    let v: serde_json::Value = serde_json::from_str(&out.stdout).expect("valid JSON on stdout");
    assert_eq!(v["mode"], "evaluate");
    assert_eq!(v["committed"], false);
    assert_eq!(v["stage"], "merchants");
    assert_eq!(v["mutation"]["kind"], "add");
    assert_eq!(v["buckets"]["newly_matched"]["payees"], 2);
    assert_eq!(v["buckets"]["newly_matched"]["samples"][0]["account"], "CommBank");
}

#[test]
fn apply_json_reports_committed_and_new_id() {
    let cli = Cli::new();
    let out = cli.run(&[
        "add", "--stage", "merchants", "--pattern", "(?i)ZZQNOVELSHOP", "--canonical", "Novel Shop",
        "--apply", "--json",
    ]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    let v: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(v["mode"], "apply");
    assert_eq!(v["committed"], true);
    assert!(v["new_id"].is_number());
    assert_eq!(v["dumped"], "rules/merchants.sql");
}

#[test]
fn bad_regex_is_syntax_error_exit_2_on_stderr() {
    let cli = Cli::new();
    let out = cli.run(&["add", "--stage", "merchants", "--pattern", "(?i)ZZQ(", "--canonical", "X"]);
    assert_eq!(out.code, 2);
    assert!(out.stdout.is_empty(), "stdout must stay empty on error");
    assert!(out.stderr.contains("syntax error:"), "stderr: {}", out.stderr);
}

#[test]
fn bad_regex_json_error_envelope_on_stderr() {
    let cli = Cli::new();
    let out = cli.run(&[
        "add", "--stage", "merchants", "--pattern", "(?i)ZZQ(", "--canonical", "X", "--json",
    ]);
    assert_eq!(out.code, 2);
    assert!(out.stdout.is_empty());
    let v: serde_json::Value = serde_json::from_str(&out.stderr).expect("JSON error envelope on stderr");
    assert_eq!(v["code"], 2);
    assert!(v["error"].is_string());
}

#[test]
fn rm_requires_force_then_commits_with_af() {
    let cli = Cli::new();
    // Find an existing merchant id from list --json.
    let listed = cli.run(&["list", "--stage", "merchants", "--json"]);
    let v: serde_json::Value = serde_json::from_str(&listed.stdout).unwrap();
    let id = v["rules"][0]["id"].as_i64().unwrap();

    // --apply without --force → exit 1.
    let no_force = cli.run(&["rm", "--stage", "merchants", "--id", &id.to_string(), "--apply"]);
    assert_eq!(no_force.code, 1);
    assert!(no_force.stderr.contains("--force"));

    // -af → committed.
    let forced = cli.run(&["rm", "--stage", "merchants", "--id", &id.to_string(), "-af", "--json"]);
    assert_eq!(forced.code, 0, "stderr: {}", forced.stderr);
    let v: serde_json::Value = serde_json::from_str(&forced.stdout).unwrap();
    assert_eq!(v["committed"], true);
}

#[test]
fn unknown_stage_is_exit_2() {
    let cli = Cli::new();
    let out = cli.run(&["list", "--stage", "bogus"]);
    assert_eq!(out.code, 2);
    assert!(out.stderr.contains("unknown --stage"));
}

#[test]
fn list_json_marks_loop_stage_ordered() {
    let cli = Cli::new();
    let out = cli.run(&["list", "--stage", "prefixes", "--json"]);
    assert_eq!(out.code, 0);
    let v: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(v["ordered"], true);
    let merch = cli.run(&["list", "--stage", "merchants", "--json"]);
    let v: serde_json::Value = serde_json::from_str(&merch.stdout).unwrap();
    assert_eq!(v["ordered"], false);
}

#[test]
fn test_verb_reports_match_miss_and_syntax_error() {
    let cli = Cli::new();
    // Match: prints the canonical and the matched span; exit 0.
    let hit = cli.run(&[
        "test", "--stage", "merchants", "--pattern", "(?i)ZAP", "--canonical", "ZapFresh",
        "ZAP CITY",
    ]);
    assert_eq!(hit.code, 0, "stderr: {}", hit.stderr);
    assert!(hit.stdout.contains("matches"), "stdout: {}", hit.stdout);
    assert!(hit.stdout.contains("ZapFresh"), "shows canonical: {}", hit.stdout);

    // Miss: exit 0, reports no match.
    let miss = cli.run(&[
        "test", "--stage", "merchants", "--pattern", "(?i)ZAP", "--canonical", "ZapFresh",
        "OPAL TRAVEL",
    ]);
    assert_eq!(miss.code, 0);
    assert!(miss.stdout.contains("no match"), "stdout: {}", miss.stdout);

    // Syntax error: exit 2, message on stderr, stdout empty.
    let bad = cli.run(&[
        "test", "--stage", "merchants", "--pattern", "(?i)ZAP(", "--canonical", "X", "ZAP",
    ]);
    assert_eq!(bad.code, 2);
    assert!(bad.stdout.is_empty(), "stdout must stay empty on error");
    assert!(bad.stderr.contains("syntax error:"), "stderr: {}", bad.stderr);
}

#[test]
fn duplicate_apply_is_exit_1_with_conflict_id() {
    let cli = Cli::new();
    // Commit a rule, then attempt an identical pattern → UNIQUE → exit 1.
    let first = cli.run(&[
        "add", "--stage", "merchants", "--pattern", "(?i)ZAPDUP", "--canonical", "Z", "--apply",
        "--json",
    ]);
    assert_eq!(first.code, 0, "stderr: {}", first.stderr);
    let dup = cli.run(&[
        "add", "--stage", "merchants", "--pattern", "(?i)ZAPDUP", "--canonical", "dup", "--apply",
    ]);
    assert_eq!(dup.code, 1);
    assert!(dup.stdout.is_empty());
    assert!(dup.stderr.contains("already exists"), "stderr: {}", dup.stderr);
}

#[test]
fn not_found_apply_is_exit_1() {
    let cli = Cli::new();
    let out = cli.run(&[
        "edit", "--stage", "merchants", "--id", "999999", "--canonical", "x", "--apply",
    ]);
    assert_eq!(out.code, 1);
    assert!(out.stdout.is_empty());
    assert!(out.stderr.contains("no merchants rule with id 999999"), "stderr: {}", out.stderr);
}

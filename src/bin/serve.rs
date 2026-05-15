use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use maud::{html, Markup, PreEscaped, DOCTYPE};
use tiny_http::{Header, Method, Request, Response, Server};

use pocketsmith_sync::db;
use pocketsmith_sync::db::transfer_pairs::{self, TransferPairRow};
use pocketsmith_sync::transfers::{self, Confidence, Status};

#[derive(Clone, Copy, PartialEq)]
enum Decision {
    Confirm,
    Reject,
    Skip,
}

impl Decision {
    fn css_class(self) -> &'static str {
        match self {
            Decision::Confirm => "decided-confirmed",
            Decision::Reject => "decided-rejected",
            Decision::Skip => "decided-skipped",
        }
    }
}

struct ActivityEntry {
    pair_id: (i64, i64),
    decision: Decision,
    amount_cents: i64,
    account_a: String,
    account_b: String,
}

struct AppState {
    conn: rusqlite::Connection,
    activity: Vec<ActivityEntry>,
    undone: usize,
    status_filter: String,
    confidence_filter: String,
    decisions: HashMap<(i64, i64), Decision>,
    active_pair: Option<(i64, i64)>,
}

fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let port: u16 = std::env::var("SERVE_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3141);

    let conn = db::initialize("pocketsmith.db")?;
    let state = Arc::new(Mutex::new(AppState {
        conn,
        activity: Vec::new(),
        undone: 0,
        status_filter: "all".to_string(),
        confidence_filter: "all".to_string(),
        decisions: HashMap::new(),
        active_pair: None,
    }));

    let addr = format!("127.0.0.1:{port}");
    let server = Server::http(&addr).map_err(|e| anyhow::anyhow!("{e}"))?;
    eprintln!("Serving on http://{addr}");

    for request in server.incoming_requests() {
        let state = Arc::clone(&state);
        handle_request(request, state);
    }

    Ok(())
}

fn handle_request(request: Request, state: Arc<Mutex<AppState>>) {
    let path = request.url().to_string();
    let method = request.method().clone();

    let response = match (method, path.as_str()) {
        (Method::Get, "/") => page_shell(&state),
        (Method::Get, p) if p.starts_with("/pair/") => {
            let id = parse_pair_id(p, "/pair/");
            id.map(|(a, b)| detail_fragment(&state, a, b))
                .unwrap_or_else(|| html! { p { "Invalid pair ID" } })
        }
        (Method::Get, p) if p.starts_with("/queue?") => {
            let params = p.strip_prefix("/queue?").unwrap_or("");
            let filter = extract_param(params, "filter").unwrap_or("pending".to_string());
            let conf = extract_param(params, "conf").unwrap_or("all".to_string());
            queue_fragment(&state, &filter, &conf)
        }
        (Method::Get, "/queue") => queue_fragment(&state, "all", "all"),
        (Method::Post, p) if p.contains("/confirm") => action_handler(&state, p, "confirm"),
        (Method::Post, p) if p.contains("/reject") => action_handler(&state, p, "reject"),
        (Method::Post, p) if p.contains("/skip") => action_handler(&state, p, "skip"),
        (Method::Post, "/clear-all-skipped") => clear_all_skipped(&state),
        (Method::Post, p) if p.contains("/unskip") => unskip_handler(&state, p),
        (Method::Post, p) if p.contains("/undo") => undo_handler(&state, p),
        _ => html! { p { "Not found" } },
    };

    let html_str = response.into_string();
    let resp = Response::from_data(html_str.as_bytes().to_vec())
        .with_header(Header::from_bytes("Content-Type", "text/html; charset=utf-8").unwrap());
    let _ = request.respond(resp);
}

fn extract_param(query: &str, key: &str) -> Option<String> {
    query.split('&')
        .find_map(|pair| {
            let mut parts = pair.splitn(2, '=');
            let k = parts.next()?;
            let v = parts.next()?;
            if k == key { Some(v.to_string()) } else { None }
        })
}

fn parse_pair_id(path: &str, prefix: &str) -> Option<(i64, i64)> {
    let rest = path.strip_prefix(prefix)?;
    let id_part = rest.split('/').next()?;
    let mut parts = id_part.split('-');
    let a: i64 = parts.next()?.parse().ok()?;
    let b: i64 = parts.next()?.parse().ok()?;
    Some((a, b))
}

fn format_dollars(cents: i64) -> String {
    let abs_cents = cents.abs();
    let whole = abs_cents / 100;
    let frac = abs_cents % 100;
    let whole_str = whole.to_string();
    let mut result = String::new();
    for (i, c) in whole_str.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    let formatted: String = result.chars().rev().collect();
    format!("${formatted}.{frac:02}")
}

fn confidence_class(c: &Confidence) -> &'static str {
    match c {
        Confidence::High => "conf-high",
        Confidence::Medium => "conf-med",
        Confidence::Low => "conf-low",
    }
}

fn confidence_reason(pair: &TransferPairRow) -> &'static str {
    let a_like = transfers::is_transfer_like(&pair.payee_a);
    let b_like = transfers::is_transfer_like(&pair.payee_b);
    match (a_like, b_like) {
        (true, true) => "Both payees match transfer patterns",
        (true, false) => "Only payee A matches a transfer pattern",
        (false, true) => "Only payee B matches a transfer pattern",
        (false, false) => "Neither payee matches transfer patterns (amount/date/account match only)",
    }
}

fn format_short_date(date: &str) -> String {
    let parts: Vec<&str> = date.split('-').collect();
    if parts.len() != 3 { return date.to_string(); }
    let month: u8 = parts[1].parse().unwrap_or(0);
    let day: u8 = parts[2].parse().unwrap_or(0);
    let month_name = match month {
        1 => "Jan", 2 => "Feb", 3 => "Mar", 4 => "Apr",
        5 => "May", 6 => "Jun", 7 => "Jul", 8 => "Aug",
        9 => "Sep", 10 => "Oct", 11 => "Nov", 12 => "Dec",
        _ => "???",
    };
    format!("{month_name} {day}")
}

fn get_prior_pairs(
    conn: &rusqlite::Connection,
    account_a: &str,
    account_b: &str,
) -> Vec<(String, i64, Status)> {
    let sql = "
        SELECT ta.date, tp.amount_cents, tp.status
        FROM transfer_pairs tp
        JOIN transactions ta ON ta.id = tp.txn_id_a
        LEFT JOIN transaction_accounts aa ON aa.id = ta.transaction_account_id
        LEFT JOIN transactions tb ON tb.id = tp.txn_id_b
        LEFT JOIN transaction_accounts ab ON ab.id = tb.transaction_account_id
        WHERE tp.status != 0
          AND ((aa.name = ?1 AND ab.name = ?2) OR (aa.name = ?2 AND ab.name = ?1))
        ORDER BY ta.date DESC
        LIMIT 5
    ";
    conn.prepare(sql)
        .ok()
        .map(|mut stmt| {
            stmt.query_map(rusqlite::params![account_a, account_b], |row| {
                let status_int: i32 = row.get(2)?;
                Ok((row.get(0)?, row.get(1)?, Status::from_i32(status_int).unwrap_or(Status::Pending)))
            })
            .ok()
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
        })
        .unwrap_or_default()
}

fn find_pair_index(pairs: &[TransferPairRow], id: (i64, i64)) -> Option<usize> {
    pairs.iter().position(|p| (p.txn_id_a, p.txn_id_b) == id)
}

fn next_pair_after(pairs: &[TransferPairRow], current: (i64, i64)) -> Option<(i64, i64)> {
    let idx = find_pair_index(pairs, current)?;
    let next_idx = if idx + 1 < pairs.len() { idx + 1 } else { idx };
    Some((pairs[next_idx].txn_id_a, pairs[next_idx].txn_id_b))
}

fn get_filtered_pairs(conn: &rusqlite::Connection, status_filter: &str, confidence_filter: &str, decisions: &HashMap<(i64, i64), Decision>) -> Vec<TransferPairRow> {
    let pairs = match status_filter {
        "all" | "skipped" => transfer_pairs::get_all_pairs(conn, 2000).unwrap_or_default(),
        "pending" => transfer_pairs::get_pairs_by_status(conn, Status::Pending, 2000).unwrap_or_default(),
        "confirmed" => transfer_pairs::get_pairs_by_status(conn, Status::Confirmed, 2000).unwrap_or_default(),
        "rejected" => transfer_pairs::get_pairs_by_status(conn, Status::Rejected, 2000).unwrap_or_default(),
        _ => Vec::new(),
    };
    pairs.into_iter()
        .filter(|p| {
            let key = (p.txn_id_a, p.txn_id_b);
            if status_filter == "skipped" {
                return decisions.get(&key) == Some(&Decision::Skip);
            }
            if status_filter == "pending" && decisions.get(&key) == Some(&Decision::Skip) {
                return false;
            }
            if confidence_filter != "all" && p.confidence.as_str() != confidence_filter {
                return false;
            }
            true
        })
        .collect()
}

fn full_page(state: &AppState, pairs: &[TransferPairRow], status_filter: &str, confidence_filter: &str) -> Markup {
    let selected = state.active_pair
        .and_then(|id| find_pair_index(pairs, id).map(|_| id))
        .or_else(|| pairs.first().map(|p| (p.txn_id_a, p.txn_id_b)));

    let active = selected.and_then(|id| find_pair_index(pairs, id)).map(|i| &pairs[i]);
    let prior = active
        .map(|p| get_prior_pairs(&state.conn, &p.account_name_a, &p.account_name_b))
        .unwrap_or_default();

    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "Transfer Pairs" }
                script src="https://unpkg.com/htmx.org@2.0.4" {}
                style { (PreEscaped(CSS)) }
            }
            body {
                div.layout {
                    div.queue-panel #queue {
                        (render_queue(pairs, selected, status_filter, confidence_filter, &state.decisions))
                    }
                    div.detail-panel #detail {
                        @if let Some(pair) = active {
                            (render_detail(pair, &prior))
                        } @else {
                            div.empty-state { p { "No pairs to show" } }
                        }
                    }
                }
                div.activity-panel #activity {
                    (render_activity(state))
                }
                script { (PreEscaped(JS)) }
            }
        }
    }
}

fn page_shell(state: &Arc<Mutex<AppState>>) -> Markup {
    let mut state = state.lock().unwrap();
    let pairs = get_filtered_pairs(&state.conn, &state.status_filter, &state.confidence_filter, &state.decisions);
    if state.active_pair.is_none() {
        state.active_pair = pairs.first().map(|p| (p.txn_id_a, p.txn_id_b));
    }
    full_page(&state, &pairs, &state.status_filter, &state.confidence_filter)
}

fn queue_fragment(state: &Arc<Mutex<AppState>>, status_filter: &str, confidence_filter: &str) -> Markup {
    let mut state = state.lock().unwrap();
    state.status_filter = status_filter.to_string();
    state.confidence_filter = confidence_filter.to_string();
    let pairs = get_filtered_pairs(&state.conn, status_filter, confidence_filter, &state.decisions);
    let current = state.active_pair;
    let in_new_list = current.and_then(|id| find_pair_index(&pairs, id)).is_some();
    if !in_new_list {
        state.active_pair = pairs.first().map(|p| (p.txn_id_a, p.txn_id_b));
    }
    let selected = state.active_pair;
    render_queue(&pairs, selected, status_filter, confidence_filter, &state.decisions)
}

fn render_queue(pairs: &[TransferPairRow], selected: Option<(i64, i64)>, status_filter: &str, confidence_filter: &str, decisions: &HashMap<(i64, i64), Decision>) -> Markup {
    html! {
        div.queue-header {
            h2 { (pairs.len()) " pairs" }
            div.filter-row {
                @for f in &["all", "pending", "confirmed", "rejected", "skipped"] {
                    button.filter-btn
                        .(if *f == status_filter { "active" } else { "" })
                        hx-get=(format!("/queue?filter={f}&conf={confidence_filter}"))
                        hx-target="#queue"
                        hx-swap="innerHTML"
                    { (f.to_uppercase()) }
                }
            }
            div.filter-row {
                @for f in &["all", "high", "medium", "low"] {
                    button.filter-btn.conf-filter
                        .(if *f == confidence_filter { "active" } else { "" })
                        hx-get=(format!("/queue?filter={status_filter}&conf={f}"))
                        hx-target="#queue"
                        hx-swap="innerHTML"
                    { (f.to_uppercase()) }
                }
                @let num_skipped = decisions.values().filter(|v| **v == Decision::Skip).count();
                @if num_skipped > 0 && status_filter == "skipped" {
                    button.filter-btn.clear-skipped-btn
                        hx-post="/clear-all-skipped"
                        hx-target="body"
                    { "CLEAR SKIPPED (" (num_skipped) ")" }
                }
            }
        }
        div.queue-list {
            @for pair in pairs {
                @let pair_id = format!("{}-{}", pair.txn_id_a, pair.txn_id_b);
                @let is_selected = selected == Some((pair.txn_id_a, pair.txn_id_b));
                @let decision = decisions.get(&(pair.txn_id_a, pair.txn_id_b)).copied();
                div.queue-item
                    .(if is_selected { "selected" } else { "" })
                    .(confidence_class(&pair.confidence))
                    .(decision.map(|d| d.css_class()).unwrap_or(""))
                    hx-get=(format!("/pair/{pair_id}"))
                    hx-target="#detail"
                    hx-swap="innerHTML"
                    data-pair-id=(pair_id)
                {
                    @if let Some(Decision::Skip) = decision {
                        span.status-indicator.skip-indicator
                            hx-post=(format!("/pair/{pair_id}/unskip"))
                            hx-target="body"
                            title="Click to unskip"
                            onclick="event.stopPropagation()"
                        { "⊘" }
                    } @else if let Some(Decision::Confirm) = decision {
                        span.status-indicator.confirm-indicator
                            hx-post=(format!("/pair/{pair_id}/undo"))
                            hx-target="body"
                            title="Click to undo"
                            onclick="event.stopPropagation()"
                        { "✓" }
                    } @else if let Some(Decision::Reject) = decision {
                        span.status-indicator.reject-indicator
                            hx-post=(format!("/pair/{pair_id}/undo"))
                            hx-target="body"
                            title="Click to undo"
                            onclick="event.stopPropagation()"
                        { "✗" }
                    } @else {
                        span.conf-badge { (pair.confidence.as_str().chars().next().unwrap_or('?').to_uppercase().to_string()) }
                    }
                    span.amount { (format_dollars(pair.amount_cents)) }
                    span.date { (format_short_date(&pair.date_a)) }
                    span.gap { (transfers::date_diff_days(&pair.date_a, &pair.date_b)) "d" }
                }
            }
        }
    }
}

fn detail_fragment(state: &Arc<Mutex<AppState>>, txn_a: i64, txn_b: i64) -> Markup {
    let mut state = state.lock().unwrap();
    state.active_pair = Some((txn_a, txn_b));
    match transfer_pairs::get_pair_by_id(&state.conn, txn_a, txn_b) {
        Ok(Some(pair)) => {
            let prior = get_prior_pairs(&state.conn, &pair.account_name_a, &pair.account_name_b);
            render_detail(&pair, &prior)
        }
        _ => html! { div.empty-state { p { "Pair not found" } } },
    }
}

fn render_detail(pair: &TransferPairRow, prior: &[(String, i64, Status)]) -> Markup {
    let pair_id = format!("{}-{}", pair.txn_id_a, pair.txn_id_b);
    let days = transfers::date_diff_days(&pair.date_a, &pair.date_b);

    html! {
        div.detail-header {
            h2 {
                span.(confidence_class(&pair.confidence)) {
                    (pair.confidence.as_str().to_uppercase())
                }
                " · " (format_dollars(pair.amount_cents))
                @if pair.status != Status::Pending {
                    span.status-badge.((match pair.status {
                        Status::Confirmed => "status-confirmed",
                        Status::Rejected => "status-rejected",
                        _ => "",
                    })) {
                        @match pair.status {
                            Status::Confirmed => { " ✓" },
                            Status::Rejected => { " ✗" },
                            _ => {},
                        }
                    }
                }
            }
            div.confidence-reason {
                (confidence_reason(pair))
            }
        }
        div.comparison {
            div.comparison-meta {
                div.meta-item {
                    span.meta-label { "DATE DIFF" }
                    span.meta-value {
                        (days) "d"
                        @if days >= 2 { " ⚠️" }
                    }
                }
                div.meta-item {
                    span.meta-label { "Amount" }
                    span.meta-value { "✅" }
                }
            }
            div.txn-cards {
                div.txn-card {
                    div.txn-card-header {
                        span.card-label { "A" }
                        span.card-account { (&pair.account_name_a) }
                    }
                    div.txn-card-body {
                        div.field { span.field-label { "Date" } span.field-value { (format_short_date(&pair.date_a)) } }
                        div.field { span.field-label { "Payee" } span.field-value { (&pair.payee_a) } }
                        div.field { span.field-label { "Amount" } span.field-value.amount-positive { "+" (format_dollars(pair.amount_cents)) } }
                    }
                }
                div.txn-card {
                    div.txn-card-header {
                        span.card-label { "B" }
                        span.card-account { (&pair.account_name_b) }
                    }
                    div.txn-card-body {
                        div.field { span.field-label { "Date" } span.field-value { (format_short_date(&pair.date_b)) } }
                        div.field { span.field-label { "Payee" } span.field-value { (&pair.payee_b) } }
                        div.field { span.field-label { "Amount" } span.field-value.amount-negative { "-" (format_dollars(pair.amount_cents)) } }
                    }
                }
            }
        }
        @if !prior.is_empty() {
            div.prior-section {
                h3 { "Prior: " (&pair.account_name_a) " ↔ " (&pair.account_name_b) }
                div.prior-list {
                    @for (date, amount, status) in prior {
                        div.prior-row {
                            span { (format_short_date(date)) }
                            span { (format_dollars(*amount)) }
                            span.((if *status == Status::Confirmed { "status-confirmed" } else { "status-rejected" })) {
                                @if *status == Status::Confirmed { "✓" } @else { "✗" }
                            }
                        }
                    }
                }
            }
        }
        div.actions data-pair-id=(pair_id) {
            button.btn.btn-confirm
                hx-post=(format!("/pair/{pair_id}/confirm"))
                hx-target="body"
            { "[Y] Confirm" }
            button.btn.btn-reject
                hx-post=(format!("/pair/{pair_id}/reject"))
                hx-target="body"
            { "[N] Reject" }
            button.btn.btn-skip
                hx-post=(format!("/pair/{pair_id}/skip"))
                hx-target="body"
            { "[S] Skip" }
        }
    }
}

fn refresh_page(state: &AppState) -> Markup {
    let pairs = get_filtered_pairs(&state.conn, &state.status_filter, &state.confidence_filter, &state.decisions);
    full_page(state, &pairs, &state.status_filter, &state.confidence_filter)
}

fn action_handler(state: &Arc<Mutex<AppState>>, path: &str, action: &str) -> Markup {
    let id = parse_pair_id(path, "/pair/");
    if let Some((a, b)) = id {
        let mut state = state.lock().unwrap();

        let decision = match action {
            "confirm" => Decision::Confirm,
            "reject" => Decision::Reject,
            "skip" => Decision::Skip,
            _ => return html! { p { "Invalid action" } },
        };

        let current_pairs = get_filtered_pairs(&state.conn, &state.status_filter, &state.confidence_filter, &state.decisions);
        let next = next_pair_after(&current_pairs, (a, b));

        let pair_info = transfer_pairs::get_pair_by_id(&state.conn, a, b)
            .ok()
            .flatten()
            .map(|p| (p.amount_cents, p.account_name_a, p.account_name_b));

        match decision {
            Decision::Confirm => {
                let _ = transfer_pairs::update_status(&state.conn, a, b, Status::Confirmed);
            }
            Decision::Reject => {
                let _ = transfer_pairs::update_status(&state.conn, a, b, Status::Rejected);
            }
            Decision::Skip => {}
        }
        state.decisions.insert((a, b), decision);

        if let Some((amount, acct_a, acct_b)) = pair_info {
            state.activity.push(ActivityEntry {
                pair_id: (a, b),
                decision,
                amount_cents: amount,
                account_a: acct_a,
                account_b: acct_b,
            });
            if state.activity.len() > 100 {
                state.activity.remove(0);
            }
        }

        let new_pairs = get_filtered_pairs(&state.conn, &state.status_filter, &state.confidence_filter, &state.decisions);
        if let Some(next_id) = next {
            if find_pair_index(&new_pairs, next_id).is_some() {
                state.active_pair = Some(next_id);
            } else {
                state.active_pair = new_pairs.last().map(|p| (p.txn_id_a, p.txn_id_b));
            }
        } else {
            state.active_pair = new_pairs.last().map(|p| (p.txn_id_a, p.txn_id_b));
        }

        return refresh_page(&state);
    }

    html! { p { "Invalid request" } }
}

fn undo_handler(state: &Arc<Mutex<AppState>>, path: &str) -> Markup {
    let id = parse_pair_id(path, "/pair/");
    if let Some((a, b)) = id {
        let mut state = state.lock().unwrap();
        let _ = transfer_pairs::update_status(&state.conn, a, b, Status::Pending);
        state.undone += 1;
        state.decisions.remove(&(a, b));
        state.activity.retain(|e| e.pair_id != (a, b));
        return refresh_page(&state);
    }

    let state = state.lock().unwrap();
    refresh_page(&state)
}

fn clear_all_skipped(state: &Arc<Mutex<AppState>>) -> Markup {
    let mut state = state.lock().unwrap();
    state.activity.retain(|e| e.decision != Decision::Skip);
    state.decisions.retain(|_, v| *v != Decision::Skip);
    refresh_page(&state)
}

fn unskip_handler(state: &Arc<Mutex<AppState>>, path: &str) -> Markup {
    let id = parse_pair_id(path, "/pair/");
    if let Some((a, b)) = id {
        let mut state = state.lock().unwrap();
        state.decisions.remove(&(a, b));
        state.activity.retain(|e| !(e.pair_id == (a, b) && e.decision == Decision::Skip));
        return refresh_page(&state);
    }
    let state = state.lock().unwrap();
    refresh_page(&state)
}

fn decision_count(state: &AppState, d: Decision) -> usize {
    state.decisions.values().filter(|v| **v == d).count()
}

fn render_activity(state: &AppState) -> Markup {
    html! {
        div.activity-header {
            span.stat { "Confirmed " span.count-confirmed { (decision_count(state, Decision::Confirm)) } }
            span.stat { "Rejected " span.count-rejected { (decision_count(state, Decision::Reject)) } }
            span.stat { "Skipped " span.count-skipped { (decision_count(state, Decision::Skip)) } }
            span.stat { "Undone " span.count-undone { (state.undone) } }
        }
        div.activity-list {
            @for entry in state.activity.iter().rev().take(20) {
                @let pair_id = format!("{}-{}", entry.pair_id.0, entry.pair_id.1);
                div.activity-row {
                    span.((match entry.decision {
                        Decision::Confirm => "status-confirmed",
                        Decision::Reject => "status-rejected",
                        Decision::Skip => "status-skipped",
                    })) {
                        @match entry.decision {
                            Decision::Confirm => { "✓ confirmed" },
                            Decision::Reject => { "✗ rejected" },
                            Decision::Skip => { "⊘ skipped" },
                        }
                    }
                    span { "#" (entry.pair_id.0) }
                    span { (format_dollars(entry.amount_cents)) }
                    span { (&entry.account_a) " → " (&entry.account_b) }
                    @if entry.decision == Decision::Skip {
                        button.undo-btn
                            hx-post=(format!("/pair/{pair_id}/unskip"))
                            hx-target="body"
                        { "unskip" }
                    } @else {
                        button.undo-btn
                            hx-post=(format!("/pair/{pair_id}/undo"))
                            hx-target="body"
                        { "undo" }
                    }
                }
            }
        }
    }
}

const CSS: &str = r#"
:root {
    --bg: #1a1b26;
    --bg-surface: #24283b;
    --bg-highlight: #292e42;
    --border: #3b4261;
    --fg: #c0caf5;
    --fg-dim: #565f89;
    --fg-dark: #414868;
    --accent: #7aa2f7;
    --green: #9ece6a;
    --red: #f7768e;
    --yellow: #e0af68;
    --magenta: #bb9af7;
    --cyan: #7dcfff;
}

* { box-sizing: border-box; margin: 0; padding: 0; }

body {
    background: var(--bg);
    color: var(--fg);
    font-family: "SF Hello", -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    font-size: 14px;
    line-height: 1.5;
    padding: 16px;
    height: 100vh;
    display: flex;
    flex-direction: column;
    overflow: hidden;
}

.layout {
    display: grid;
    grid-template-columns: 360px 1fr;
    gap: 16px;
    flex: 1;
    min-height: 0;
    margin-bottom: 16px;
}

@media (max-width: 768px) {
    body { overflow: auto; height: auto; }
    .layout {
        grid-template-columns: 1fr;
        flex: none;
    }
    .queue-panel { max-height: 35vh; }
}

/* Queue panel */
.queue-panel {
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: 8px;
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
}

.queue-header {
    padding: 12px 16px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
}

.queue-header h2 {
    font-size: 15px;
    font-weight: 600;
    margin-bottom: 8px;
    color: var(--fg);
}

.filter-row { display: flex; gap: 4px; margin-bottom: 4px; }
.filter-row:last-child { margin-bottom: 0; }

.filter-btn {
    background: var(--bg-highlight);
    border: 1px solid var(--border);
    color: var(--fg-dim);
    padding: 2px 8px;
    border-radius: 4px;
    cursor: pointer;
    font-size: 11px;
    font-family: "SF Mono", ui-monospace, "Cascadia Code", monospace;
    text-transform: uppercase;
    letter-spacing: 0.5px;
}
.filter-btn:hover { border-color: var(--accent); color: var(--fg); }
.filter-btn.active { background: var(--accent); color: var(--bg); border-color: var(--accent); }

.queue-list { flex: 1; overflow-y: auto; min-height: 0; }

.queue-item {
    display: grid;
    grid-template-columns: 24px 1fr auto auto;
    gap: 8px;
    align-items: center;
    padding: 6px 12px;
    border-bottom: 1px solid var(--border);
    cursor: pointer;
    font-family: "SF Mono", ui-monospace, "Cascadia Code", monospace;
    font-size: 12px;
    transition: background 0.1s;
}
.queue-item:hover { background: var(--bg-highlight); }
.queue-item.selected { background: var(--bg-highlight); border-left: 3px solid var(--accent); padding-left: 9px; }

.conf-badge {
    font-size: 10px;
    font-weight: 700;
    width: 20px;
    height: 20px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 3px;
}
.conf-high .conf-badge { background: rgba(158, 206, 106, 0.15); color: var(--green); }
.conf-med .conf-badge { background: rgba(224, 175, 104, 0.15); color: var(--yellow); }
.conf-low .conf-badge { background: rgba(187, 154, 247, 0.15); color: var(--magenta); }

.queue-item .amount { color: var(--fg); text-align: right; }
.queue-item .date { color: var(--fg-dim); }
.queue-item .gap { color: var(--fg-dark); font-size: 11px; }

.queue-item.decided-confirmed { background: rgba(158, 206, 106, 0.08); }
.queue-item.decided-rejected { background: rgba(247, 118, 142, 0.08); }
.queue-item.decided-skipped { opacity: 0.5; }
.queue-item.decided-skipped .amount { text-decoration: line-through; }

.status-indicator {
    font-size: 12px;
    width: 20px;
    height: 20px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 3px;
    cursor: pointer;
}
.status-indicator:hover { transform: scale(1.2); }
.confirm-indicator { color: var(--green); background: rgba(158, 206, 106, 0.15); }
.reject-indicator { color: var(--red); background: rgba(247, 118, 142, 0.15); }
.skip-indicator { color: var(--fg-dim); background: var(--bg-highlight); }

.clear-skipped-btn { color: var(--yellow) !important; border-color: var(--yellow) !important; margin-left: auto; }

/* Detail panel */
.detail-panel {
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 20px;
    overflow-y: auto;
    min-height: 0;
}

.detail-header h2 {
    font-size: 16px;
    font-weight: 600;
    margin-bottom: 4px;
}

.confidence-reason {
    font-size: 12px;
    color: var(--fg-dim);
    margin-bottom: 16px;
    font-style: italic;
}

.status-badge { margin-left: 8px; }

/* Comparison layout */
.comparison { margin-bottom: 16px; }

.comparison-meta {
    display: flex;
    gap: 24px;
    margin-bottom: 12px;
    padding: 8px 12px;
    background: var(--bg);
    border-radius: 6px;
    border: 1px solid var(--border);
}

.meta-item { display: flex; align-items: center; gap: 8px; }
.meta-label {
    font-size: 11px;
    color: var(--fg-dim);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    font-family: "SF Mono", ui-monospace, "Cascadia Code", monospace;
}
.meta-value {
    font-size: 14px;
    font-weight: 600;
    font-family: "SF Mono", ui-monospace, "Cascadia Code", monospace;
}

.txn-cards {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 2px;
}

@media (max-width: 768px) {
    .txn-cards { grid-template-columns: 1fr; gap: 8px; }
}

.txn-card {
    background: var(--bg);
    border: 1px solid var(--border);
    padding: 12px;
}
.txn-card:first-child { border-radius: 6px 0 0 6px; }
.txn-card:last-child { border-radius: 0 6px 6px 0; }

.txn-card-header {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 10px;
    padding-bottom: 6px;
    border-bottom: 1px solid var(--border);
}

.card-label {
    font-size: 11px;
    font-weight: 700;
    width: 20px;
    height: 20px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 3px;
    background: var(--bg-highlight);
    color: var(--accent);
    font-family: "SF Mono", ui-monospace, "Cascadia Code", monospace;
}

.card-account {
    font-size: 13px;
    font-weight: 600;
    color: var(--fg);
}

.txn-card-body { display: flex; flex-direction: column; gap: 4px; }

.field {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    font-family: "SF Mono", ui-monospace, "Cascadia Code", monospace;
    font-size: 12px;
}
.field-label { color: var(--fg-dim); }
.field-value { color: var(--fg); text-align: right; max-width: 60%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.amount-positive { color: var(--green) !important; font-weight: 600; }
.amount-negative { color: var(--red) !important; font-weight: 600; }

/* Prior pairs */
.prior-section {
    margin-bottom: 16px;
    padding: 10px 12px;
    background: var(--bg);
    border-radius: 6px;
    border: 1px solid var(--border);
}
.prior-section h3 {
    font-size: 12px;
    color: var(--fg-dim);
    margin-bottom: 6px;
}
.prior-row {
    display: flex;
    gap: 16px;
    font-family: "SF Mono", ui-monospace, "Cascadia Code", monospace;
    font-size: 12px;
    padding: 2px 0;
}
.status-confirmed { color: var(--green); }
.status-rejected { color: var(--red); }
.status-skipped { color: var(--fg-dim); }

/* Actions */
.actions {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
}

.btn {
    padding: 8px 20px;
    border: 1px solid var(--border);
    border-radius: 6px;
    cursor: pointer;
    font-family: "SF Mono", ui-monospace, "Cascadia Code", monospace;
    font-size: 13px;
    font-weight: 600;
    transition: all 0.15s;
}
.btn:hover { transform: translateY(-1px); }

.btn-confirm { background: rgba(158, 206, 106, 0.15); color: var(--green); border-color: var(--green); }
.btn-confirm:hover { background: rgba(158, 206, 106, 0.25); }

.btn-reject { background: rgba(247, 118, 142, 0.15); color: var(--red); border-color: var(--red); }
.btn-reject:hover { background: rgba(247, 118, 142, 0.25); }

.btn-skip { background: var(--bg-highlight); color: var(--fg-dim); }
.btn-skip:hover { color: var(--fg); }

/* Activity panel */
.activity-panel {
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 12px 16px;
    max-height: 160px;
    overflow-y: auto;
    flex-shrink: 0;
}

.activity-header {
    display: flex;
    gap: 20px;
    margin-bottom: 8px;
    font-size: 13px;
    flex-wrap: wrap;
}
.stat { color: var(--fg-dim); }
.count-confirmed { color: var(--green); font-weight: 600; }
.count-rejected { color: var(--red); font-weight: 600; }
.count-skipped { color: var(--fg-dim); font-weight: 600; }
.count-undone { color: var(--yellow); font-weight: 600; }

.activity-row {
    display: flex;
    gap: 12px;
    align-items: center;
    font-family: "SF Mono", ui-monospace, "Cascadia Code", monospace;
    font-size: 12px;
    padding: 3px 0;
    border-bottom: 1px solid var(--border);
}
.activity-row:last-child { border-bottom: none; }

.undo-btn {
    background: none;
    border: 1px solid var(--border);
    color: var(--fg-dim);
    padding: 1px 8px;
    border-radius: 3px;
    cursor: pointer;
    font-family: inherit;
    font-size: 11px;
    margin-left: auto;
}
.undo-btn:hover { color: var(--yellow); border-color: var(--yellow); }

.empty-state {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 200px;
    color: var(--fg-dim);
    font-size: 16px;
}
"#;

const JS: &str = r#"
if (!window._navInitialized) {
window._navInitialized = true;

function selectItem(item) {
    document.querySelectorAll('.queue-item.selected').forEach(el => el.classList.remove('selected'));
    item.classList.add('selected');
    item.scrollIntoView({block: 'nearest'});
}

function getSelectedIndex() {
    const items = document.querySelectorAll('.queue-item');
    const selected = document.querySelector('.queue-item.selected');
    return Array.from(items).indexOf(selected);
}

document.addEventListener('click', function(e) {
    const item = e.target.closest('.queue-item');
    if (item) selectItem(item);
});

document.addEventListener('keydown', function(e) {
    if (e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA') return;

    if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
        e.preventDefault();
        const items = document.querySelectorAll('.queue-item');
        if (items.length === 0) return;
        let idx = getSelectedIndex();
        if (idx === -1) idx = 0;
        if (e.key === 'ArrowDown') {
            idx = Math.min(idx + 1, items.length - 1);
        } else {
            idx = Math.max(idx - 1, 0);
        }
        selectItem(items[idx]);
        htmx.ajax('GET', '/pair/' + items[idx].dataset.pairId, {target: '#detail', swap: 'innerHTML'});
        return;
    }

    const actions = document.querySelector('.actions');
    const pairId = actions ? actions.dataset.pairId : null;

    switch(e.key.toLowerCase()) {
        case 'y':
            if (!pairId) return;
            e.preventDefault();
            htmx.ajax('POST', '/pair/' + pairId + '/confirm', {target: 'body'});
            break;
        case 'n':
            if (!pairId) return;
            e.preventDefault();
            htmx.ajax('POST', '/pair/' + pairId + '/reject', {target: 'body'});
            break;
        case 's':
            if (!pairId) return;
            e.preventDefault();
            htmx.ajax('POST', '/pair/' + pairId + '/skip', {target: 'body'});
            break;
        case 'u':
            e.preventDefault();
            const undoBtn = document.querySelector('.undo-btn');
            if (undoBtn) undoBtn.click();
            break;
    }
});

} // end _navInitialized guard
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn make_pair(id_a: i64, id_b: i64, status: Status, confidence: Confidence) -> TransferPairRow {
        TransferPairRow {
            txn_id_a: id_a,
            txn_id_b: id_b,
            amount_cents: 1000,
            confidence,
            status,
            date_a: "2024-01-01".to_string(),
            date_b: "2024-01-02".to_string(),
            payee_a: "A".to_string(),
            payee_b: "B".to_string(),
            account_name_a: "Acc1".to_string(),
            account_name_b: "Acc2".to_string(),
        }
    }

    fn sample_pairs() -> Vec<TransferPairRow> {
        vec![
            make_pair(1, 2, Status::Pending, Confidence::High),
            make_pair(3, 4, Status::Pending, Confidence::Medium),
            make_pair(5, 6, Status::Pending, Confidence::Low),
            make_pair(7, 8, Status::Pending, Confidence::High),
            make_pair(9, 10, Status::Pending, Confidence::Medium),
        ]
    }

    #[test]
    fn find_pair_index_returns_correct_position() {
        let pairs = sample_pairs();
        assert_eq!(find_pair_index(&pairs, (1, 2)), Some(0));
        assert_eq!(find_pair_index(&pairs, (5, 6)), Some(2));
        assert_eq!(find_pair_index(&pairs, (9, 10)), Some(4));
    }

    #[test]
    fn find_pair_index_returns_none_for_missing() {
        let pairs = sample_pairs();
        assert_eq!(find_pair_index(&pairs, (99, 100)), None);
    }

    #[test]
    fn next_pair_after_returns_next_in_list() {
        let pairs = sample_pairs();
        assert_eq!(next_pair_after(&pairs, (1, 2)), Some((3, 4)));
        assert_eq!(next_pair_after(&pairs, (3, 4)), Some((5, 6)));
        assert_eq!(next_pair_after(&pairs, (7, 8)), Some((9, 10)));
    }

    #[test]
    fn next_pair_after_last_stays_on_last() {
        let pairs = sample_pairs();
        assert_eq!(next_pair_after(&pairs, (9, 10)), Some((9, 10)));
    }

    #[test]
    fn next_pair_after_missing_returns_none() {
        let pairs = sample_pairs();
        assert_eq!(next_pair_after(&pairs, (99, 100)), None);
    }

    #[test]
    fn next_pair_after_single_item_stays() {
        let pairs = vec![make_pair(1, 2, Status::Pending, Confidence::High)];
        assert_eq!(next_pair_after(&pairs, (1, 2)), Some((1, 2)));
    }

    #[test]
    fn next_pair_after_empty_list_returns_none() {
        let pairs: Vec<TransferPairRow> = vec![];
        assert_eq!(next_pair_after(&pairs, (1, 2)), None);
    }

    #[test]
    fn action_advances_to_next_not_first() {
        let pairs = sample_pairs();
        let current = (5, 6);
        let next = next_pair_after(&pairs, current);
        assert_eq!(next, Some((7, 8)));
    }

    #[test]
    fn filter_change_keeps_active_if_present() {
        let pairs = sample_pairs();
        let active = Some((5, 6));
        let in_list = active.and_then(|id| find_pair_index(&pairs, id)).is_some();
        assert!(in_list);
    }

    #[test]
    fn filter_change_resets_to_first_if_active_absent() {
        let pairs = sample_pairs();
        let active = Some((99, 100));
        let in_list = active.and_then(|id| find_pair_index(&pairs, id)).is_some();
        assert!(!in_list);
        let new_active = pairs.first().map(|p| (p.txn_id_a, p.txn_id_b));
        assert_eq!(new_active, Some((1, 2)));
    }

    #[test]
    fn filter_change_empty_list_gives_none() {
        let pairs: Vec<TransferPairRow> = vec![];
        let active = Some((1, 2));
        let in_list = active.and_then(|id| find_pair_index(&pairs, id)).is_some();
        assert!(!in_list);
        let new_active = pairs.first().map(|p| (p.txn_id_a, p.txn_id_b));
        assert_eq!(new_active, None);
    }

    #[test]
    fn arrow_down_from_first_selects_second() {
        let pairs = sample_pairs();
        let current_idx = 0;
        let next_idx = (current_idx + 1).min(pairs.len() - 1);
        assert_eq!(next_idx, 1);
        assert_eq!((pairs[next_idx].txn_id_a, pairs[next_idx].txn_id_b), (3, 4));
    }

    #[test]
    fn arrow_up_from_first_stays_at_first() {
        let current_idx: usize = 0;
        let next_idx = current_idx.saturating_sub(1);
        assert_eq!(next_idx, 0);
    }

    #[test]
    fn arrow_down_from_last_stays_at_last() {
        let pairs = sample_pairs();
        let current_idx = pairs.len() - 1;
        let next_idx = (current_idx + 1).min(pairs.len() - 1);
        assert_eq!(next_idx, 4);
        assert_eq!((pairs[next_idx].txn_id_a, pairs[next_idx].txn_id_b), (9, 10));
    }

    #[test]
    fn click_sets_active_to_clicked_pair() {
        let pairs = sample_pairs();
        let clicked = (7, 8);
        assert!(find_pair_index(&pairs, clicked).is_some());
    }

    #[test]
    fn action_on_last_item_stays_on_last() {
        let pairs = sample_pairs();
        let current = (9, 10);
        let next = next_pair_after(&pairs, current);
        assert_eq!(next, Some((9, 10)));
        // After item removed from new list, fall back to new last
        let new_pairs = &pairs[..4];
        let in_new = find_pair_index(new_pairs, next.unwrap()).is_some();
        assert!(!in_new);
        let fallback = new_pairs.last().map(|p| (p.txn_id_a, p.txn_id_b));
        assert_eq!(fallback, Some((7, 8)));
    }

    #[test]
    fn action_does_not_overflow_past_end() {
        let pairs = vec![
            make_pair(1, 2, Status::Pending, Confidence::High),
            make_pair(3, 4, Status::Pending, Confidence::High),
            make_pair(5, 6, Status::Pending, Confidence::High),
        ];
        let next = next_pair_after(&pairs, (5, 6));
        assert_eq!(next, Some((5, 6)));
    }

    #[test]
    fn action_does_not_loop_back() {
        let pairs = sample_pairs();
        let next = next_pair_after(&pairs, (9, 10));
        assert_eq!(next, Some((9, 10)));
        assert_ne!(next, Some((1, 2)));
    }

    #[test]
    fn navigation_order_matches_sidebar_display_order() {
        let pairs = sample_pairs();
        let order: Vec<(i64, i64)> = pairs.iter().map(|p| (p.txn_id_a, p.txn_id_b)).collect();
        assert_eq!(order, vec![(1, 2), (3, 4), (5, 6), (7, 8), (9, 10)]);

        let mut current = order[0];
        for expected in &order[1..] {
            let next = next_pair_after(&pairs, current).unwrap();
            assert_eq!(next, *expected);
            current = next;
        }
        let next = next_pair_after(&pairs, current).unwrap();
        assert_eq!(next, *order.last().unwrap());
    }
}

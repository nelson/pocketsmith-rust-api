mod api;
mod css;
mod dashboard;
mod freshness;
mod helpers;
mod js;
mod normalise;
mod pipeline;
mod render;
mod state;
mod tab;
mod trace;
mod transactions;
mod transfers;

#[cfg(test)]
mod smoke_tests;

#[cfg(test)]
mod pipeline_integration;

use std::sync::{Arc, Mutex};

use anyhow::Result;
use maud::html;
use tiny_http::{Header, Method, Request, Response, Server};

use pocketsmith::db;

use crate::serve::helpers::extract_param;
use crate::serve::state::{AppState, Decision};
use crate::serve::transfers::helpers::parse_pair_id;

/// Entry point for the `serve` subcommand: boot the local web UI.
/// `_args` are the tokens after the `serve` verb (currently unused;
/// configuration is via `SERVE_HOST` / `SERVE_PORT` env vars).
pub fn run(_args: &[String]) -> Result<()> {
    let port: u16 = std::env::var("SERVE_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3141);

    // Single shared app-DB init (schema + rule seed) — see db::open_app_db.
    let conn = db::open_app_db()?;
    let mut app = AppState::new(conn);
    // Remember the DB path so committed rule edits can re-dump their
    // `rules/<stage>.sql` mirror on a background thread.
    app.db_path = Some(db::path_from_env());
    let state = Arc::new(Mutex::new(app));
    let api_config = Arc::new(api::Config::from_env()?);

    let host = std::env::var("SERVE_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let addr = format!("{host}:{port}");
    let server = Server::http(&addr).map_err(|e| anyhow::anyhow!("{e}"))?;
    eprintln!("Serving on http://{addr}");

    for request in server.incoming_requests() {
        let state = Arc::clone(&state);
        let api_config = Arc::clone(&api_config);
        handle_request(request, state, api_config);
    }

    Ok(())
}

fn handle_request(
    mut request: Request,
    state: Arc<Mutex<AppState>>,
    api_config: Arc<api::Config>,
) {
    let path = request.url().to_string();
    let method = request.method().clone();

    let route = path.split('?').next().unwrap_or(&path);
    if path.starts_with("/api/")
        || route == "/mcp"
        || route.starts_with("/oauth/")
        || route.starts_with("/.well-known/")
    {
        api::handle(request, &state, &api_config);
        return;
    }

    // A public reporting deployment must not accidentally expose the legacy
    // HTML UI's mutation routes. Local/private deployments keep the UI.
    if api_config.api_only {
        let resp = Response::from_string("Not found").with_status_code(404);
        let _ = request.respond(resp);
        return;
    }

    // `/` redirects to the dashboard tab. Each tab is its own page tree.
    if method == Method::Get && (path == "/" || path.is_empty()) {
        let resp = Response::from_data(Vec::new())
            .with_status_code(302)
            .with_header(Header::from_bytes("Location", "/dashboard/").unwrap());
        let _ = request.respond(resp);
        return;
    }

    let response = match (method, path.as_str()) {
        // --- Dashboard tab
        (Method::Get, "/dashboard" | "/dashboard/") => dashboard::views::render_page_shell(&state),
        (Method::Get, p) if p.starts_with("/dashboard/month/") => {
            let ym = p.trim_start_matches("/dashboard/month/");
            // Reject anything that doesn't look like YYYY-MM so we
            // don't run an unbounded query on a malformed URL.
            if ym.len() == 7 && ym.chars().nth(4) == Some('-') {
                dashboard::views::render_month_detail_fragment(&state, ym)
            } else {
                html! { p { "Invalid month" } }
            }
        }

        // --- Transactions tab (read-only shell for now)
        (Method::Get, "/transactions" | "/transactions/") => transactions::views::render_page_shell(&state),
        (Method::Get, p) if p.starts_with("/transactions/queue?") => {
            let params = p.strip_prefix("/transactions/queue?").unwrap_or("");
            let filter = extract_param(params, "filter").unwrap_or("all".to_string());
            transactions::views::render_queue_fragment(&state, &filter)
        }
        (Method::Get, "/transactions/queue") => transactions::views::render_queue_fragment(&state, "all"),
        (Method::Get, p) if p.starts_with("/transactions/txn/") => {
            let id_str = p.trim_start_matches("/transactions/txn/").split('/').next().unwrap_or("");
            match id_str.parse::<i64>() {
                Ok(id) => transactions::views::render_detail_fragment(&state, id),
                Err(_) => html! { p { "Invalid transaction id" } },
            }
        }
        // POST /transactions/txn/<id>/{norm,pair}/{confirm,reject,skip,undo}
        (Method::Post, p) if p.starts_with("/transactions/txn/") => {
            let rest = &p["/transactions/txn/".len()..];
            // Path: <id>/<pillar>/<verb>
            let mut parts = rest.split('/');
            let id_str = parts.next().unwrap_or("");
            let pillar = parts.next().unwrap_or("");
            let verb = parts.next().unwrap_or("");
            let id = match id_str.parse::<i64>() {
                Ok(v) => v,
                Err(_) => return invalid_action_response(request),
            };
            match (pillar, verb) {
                ("norm", "confirm") => transactions::handlers::act_norm(&state, id, Decision::Confirm),
                ("norm", "reject") => transactions::handlers::act_norm(&state, id, Decision::Reject),
                ("norm", "skip") => transactions::handlers::act_norm(&state, id, Decision::Skip),
                ("norm", "undo") | ("norm", "unskip") => transactions::handlers::undo_norm(&state, id),
                ("pair", "confirm") => transactions::handlers::act_pair(&state, id, Decision::Confirm),
                ("pair", "reject") => transactions::handlers::act_pair(&state, id, Decision::Reject),
                ("pair", "skip") => transactions::handlers::act_pair(&state, id, Decision::Skip),
                ("pair", "undo") | ("pair", "unskip") => transactions::handlers::undo_pair(&state, id),
                _ => return invalid_action_response(request),
            }
            // Re-render the Transactions page so the user keeps their context.
            transactions::views::render_page_shell(&state)
        }

        // --- Normalise tab (matched first so /normalise/* POSTs don't
        // fall into the transfer arms below.)
        (Method::Get, "/normalise" | "/normalise/") => normalise::views::render_page_shell(&state),
        (Method::Get, p) if p.starts_with("/normalise/item/") => {
            let slug = p.trim_start_matches("/normalise/item/");
            normalise::views::render_detail_fragment(&state, slug)
        }
        (Method::Get, p) if p.starts_with("/normalise/queue?") => {
            let params = p.strip_prefix("/normalise/queue?").unwrap_or("");
            let filter = extract_param(params, "filter").unwrap_or("pending".to_string());
            let class = extract_param(params, "class").unwrap_or("all".to_string());
            normalise::views::render_queue_fragment(&state, &filter, &class)
        }
        (Method::Get, "/normalise/queue") => normalise::views::render_queue_fragment(&state, "all", "all"),
        // Every /normalise/item/<slug>/<verb> POST.
        (Method::Post, p) if p.starts_with("/normalise/item/") => {
            let rest = &p["/normalise/item/".len()..];
            match rest.rsplit_once('/') {
                Some((slug, "confirm")) => normalise::handlers::act(&state, slug, Decision::Confirm),
                Some((slug, "reject")) => normalise::handlers::act(&state, slug, Decision::Reject),
                Some((slug, "skip")) => normalise::handlers::act(&state, slug, Decision::Skip),
                Some((slug, "undo")) | Some((slug, "unskip")) => normalise::handlers::undo(&state, slug),
                _ => return invalid_action_response(request),
            }
            normalise::views::render_page_shell(&state)
        }
        (Method::Post, "/normalise/clear-all-skipped") => {
            normalise::handlers::clear_all_skipped(&state);
            normalise::views::render_page_shell(&state)
        }
        (Method::Post, "/normalise/apply") => {
            normalise::handlers::apply(&state);
            normalise::views::render_page_shell(&state)
        }

        // --- Pipeline tab (rule editing)
        (Method::Get, "/pipeline" | "/pipeline/") => pipeline::views::render_page_shell(&state),
        (Method::Get, p) if p.starts_with("/pipeline/stage/") => {
            let rest = &p["/pipeline/stage/".len()..];
            let mut parts = rest.split('/');
            let slug = parts.next().unwrap_or("");
            match (parts.next(), parts.next(), parts.next()) {
                (None, _, _) | (Some(""), None, _) => pipeline::views::render_detail_fragment(&state, slug),
                (Some("new"), None, _) => pipeline::views::render_new_fragment(&state, slug),
                (Some("rule"), Some(id), verb) => match id.parse::<i64>() {
                    Ok(id) => match verb {
                        None => pipeline::views::render_edit_fragment(&state, slug, id),
                        // GET delete = the impact preview (pure read); the
                        // actual removal is the POST to the same path.
                        Some("delete") => pipeline::handlers::delete_preview(&state, slug, id),
                        _ => html! { p { "Not found" } },
                    },
                    Err(_) => html! { p { "Invalid rule id" } },
                },
                _ => html! { p { "Not found" } },
            }
        }
        // POST /pipeline/rescan re-scans payee proposals (clears dirty banner).
        (Method::Post, "/pipeline/rescan") => {
            let resp = pipeline::handlers::rescan(&state);
            send_html(request, resp);
            return;
        }
        // POST /pipeline/stage/<slug>/... mutations (create/edit/delete/
        // evaluate/reorder). The body carries the urlencoded form.
        (Method::Post, p) if p.starts_with("/pipeline/stage/") => {
            let path = p.to_string();
            let body = read_body(&mut request);
            let rest = &path["/pipeline/stage/".len()..];
            let mut parts = rest.split('/');
            let slug = parts.next().unwrap_or("");
            let resp = match (parts.next(), parts.next(), parts.next()) {
                // /stage/<slug>/new/evaluate
                (Some("new"), Some("evaluate"), None) => {
                    pipeline::handlers::evaluate(&state, slug, None, &body)
                }
                // /stage/<slug>/rule   (create)
                (Some("rule"), None, _) => pipeline::handlers::create(&state, slug, &body),
                // /stage/<slug>/reorder
                (Some("reorder"), None, _) => pipeline::handlers::reorder(&state, slug, &body),
                // /stage/<slug>/rule/<id>[/evaluate|/delete]
                (Some("rule"), Some(id_str), verb) => match id_str.parse::<i64>() {
                    Ok(id) => match verb {
                        None => pipeline::handlers::save_edit(&state, slug, id, &body),
                        Some("evaluate") => pipeline::handlers::evaluate(&state, slug, Some(id), &body),
                        Some("delete") => pipeline::handlers::delete(&state, slug, id),
                        _ => return invalid_action_response(request),
                    },
                    Err(_) => return invalid_action_response(request),
                },
                _ => return invalid_action_response(request),
            };
            send_html(request, resp);
            return;
        }

        // --- Transfers tab
        (Method::Get, "/transfers" | "/transfers/") => transfers::views::render_page_shell(&state),
        (Method::Get, p) if p.starts_with("/transfers/pair/") => {
            let id_part = p.trim_start_matches("/transfers/pair/").split('/').next().unwrap_or("");
            match parse_pair_id(id_part) {
                Some((a, b)) => transfers::views::render_detail_fragment(&state, a, b),
                None => html! { p { "Invalid pair ID" } },
            }
        }
        (Method::Get, p) if p.starts_with("/transfers/queue?") => {
            let params = p.strip_prefix("/transfers/queue?").unwrap_or("");
            let filter = extract_param(params, "filter").unwrap_or("pending".to_string());
            let conf = extract_param(params, "conf").unwrap_or("all".to_string());
            transfers::views::render_queue_fragment(&state, &filter, &conf)
        }
        (Method::Get, "/transfers/queue") => transfers::views::render_queue_fragment(&state, "all", "all"),
        (Method::Get, p) if p.starts_with("/transfers/bulk-prompt?") => {
            let params = p.strip_prefix("/transfers/bulk-prompt?").unwrap_or("");
            let action = extract_param(params, "action").unwrap_or("confirm".to_string());
            transfers::views::render_bulk_prompt_fragment(&state, &action)
        }
        (Method::Get, "/transfers/bulk-buttons") => transfers::views::render_bulk_buttons_fragment(&state),
        // Every /transfers/pair/<a>-<b>/<verb> POST.
        (Method::Post, p) if p.starts_with("/transfers/pair/") => {
            let rest = &p["/transfers/pair/".len()..];
            match rest.rsplit_once('/') {
                Some((id, "confirm")) => match parse_pair_id(id) {
                    Some(k) => transfers::handlers::act(&state, k, Decision::Confirm),
                    None => return invalid_action_response(request),
                },
                Some((id, "reject")) => match parse_pair_id(id) {
                    Some(k) => transfers::handlers::act(&state, k, Decision::Reject),
                    None => return invalid_action_response(request),
                },
                Some((id, "skip")) => match parse_pair_id(id) {
                    Some(k) => transfers::handlers::act(&state, k, Decision::Skip),
                    None => return invalid_action_response(request),
                },
                Some((id, "undo")) | Some((id, "unskip")) => match parse_pair_id(id) {
                    Some(k) => transfers::handlers::undo(&state, k),
                    None => return invalid_action_response(request),
                },
                _ => return invalid_action_response(request),
            }
            transfers::views::render_page_shell(&state)
        }
        (Method::Post, "/transfers/bulk-confirm") => {
            transfers::handlers::bulk_act(&state, Decision::Confirm);
            transfers::views::render_page_shell(&state)
        }
        (Method::Post, "/transfers/bulk-reject") => {
            transfers::handlers::bulk_act(&state, Decision::Reject);
            transfers::views::render_page_shell(&state)
        }
        (Method::Post, "/transfers/apply") => {
            transfers::handlers::apply(&state);
            transfers::views::render_page_shell(&state)
        }
        (Method::Post, "/transfers/clear-all-skipped") => {
            transfers::handlers::clear_all_skipped(&state);
            transfers::views::render_page_shell(&state)
        }
        _ => html! { p { "Not found" } },
    };

    let html_str = response.into_string();
    let resp = Response::from_data(html_str.as_bytes().to_vec())
        .with_header(Header::from_bytes("Content-Type", "text/html; charset=utf-8").unwrap());
    let _ = request.respond(resp);
}

/// Read a request body to a String (urlencoded form). Best-effort: a
/// read error yields an empty body, which the form decoder treats as
/// "no fields".
fn read_body(request: &mut Request) -> String {
    let mut body = String::new();
    let _ = request.as_reader().read_to_string(&mut body);
    body
}

/// Respond with an HTML `Markup` fragment.
fn send_html(request: Request, markup: maud::Markup) {
    let html_str = markup.into_string();
    let resp = Response::from_data(html_str.into_bytes())
        .with_header(Header::from_bytes("Content-Type", "text/html; charset=utf-8").unwrap());
    let _ = request.respond(resp);
}

/// Send a 400 Bad Request with a plain HTML body. Used for action URLs
/// the route table couldn't parse (malformed pair id, unknown verb).
fn invalid_action_response(request: Request) {
    let body = "<p>Invalid action</p>";
    let resp = Response::from_data(body.as_bytes().to_vec())
        .with_status_code(400)
        .with_header(Header::from_bytes("Content-Type", "text/html; charset=utf-8").unwrap());
    let _ = request.respond(resp);
}

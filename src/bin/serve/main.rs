mod css;
mod helpers;
mod js;
mod normalise;
mod render;
mod state;
mod tab;
mod transactions;
mod transfers;

#[cfg(test)]
mod smoke_tests;

use std::sync::{Arc, Mutex};

use anyhow::Result;
use maud::html;
use tiny_http::{Header, Method, Request, Response, Server};

use pocketsmith_sync::db;

use crate::helpers::extract_param;
use crate::state::{AppState, Decision};
use crate::transfers::helpers::parse_pair_id;

fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let port: u16 = std::env::var("SERVE_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3141);

    let conn = db::initialize(&db::path_from_env())?;
    let state = Arc::new(Mutex::new(AppState::new(conn)));

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

    // `/` redirects to the transfers tab. Each tab is its own page tree.
    if method == Method::Get && (path == "/" || path.is_empty()) {
        let resp = Response::from_data(Vec::new())
            .with_status_code(302)
            .with_header(Header::from_bytes("Location", "/transfers/").unwrap());
        let _ = request.respond(resp);
        return;
    }

    let response = match (method, path.as_str()) {
        // --- Transactions tab (read-only shell for now)
        (Method::Get, "/transactions" | "/transactions/") => transactions::views::render_page_shell(&state),
        (Method::Get, p) if p.starts_with("/transactions/txn/") => {
            let id_str = p.trim_start_matches("/transactions/txn/").split('/').next().unwrap_or("");
            match id_str.parse::<i64>() {
                Ok(id) => transactions::views::render_detail_fragment(&state, id),
                Err(_) => html! { p { "Invalid transaction id" } },
            }
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

/// Send a 400 Bad Request with a plain HTML body. Used for action URLs
/// the route table couldn't parse (malformed pair id, unknown verb).
fn invalid_action_response(request: Request) {
    let body = "<p>Invalid action</p>";
    let resp = Response::from_data(body.as_bytes().to_vec())
        .with_status_code(400)
        .with_header(Header::from_bytes("Content-Type", "text/html; charset=utf-8").unwrap());
    let _ = request.respond(resp);
}

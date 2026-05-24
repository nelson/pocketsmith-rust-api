mod css;
mod handlers;
mod helpers;
mod js;
mod normalise;
mod render;
mod state;
mod tab;
mod views;

#[cfg(test)]
mod smoke_tests;

use std::sync::{Arc, Mutex};

use anyhow::Result;
use maud::{html, Markup};
use tiny_http::{Header, Method, Request, Response, Server};

use pocketsmith_sync::db;

use crate::helpers::{extract_param, parse_pair_id};
use crate::state::{AppState, Decision};
use crate::views::{
    render_bulk_buttons_fragment, render_bulk_prompt_fragment, render_current_page,
    render_detail_fragment, render_page_shell, render_queue_fragment,
};

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
        // --- Normalise tab (checked first so /normalise/... POST paths
        // don't fall into the transfer-side /confirm contains-match).
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
        (Method::Get, "/normalise/queue") => {
            normalise::views::render_queue_fragment(&state, "all", "all")
        }
        // All five normalise item actions share the path shape
        // `/normalise/item/<slug>/<action>`. Dispatch on the trailing
        // verb and re-render the page shell on success.
        (Method::Post, p) if p.starts_with("/normalise/item/") => {
            let rest = &p["/normalise/item/".len()..];
            match rest.rsplit_once('/') {
                Some((slug, "confirm")) => {
                    normalise::handlers::act(&state, slug, state::Decision::Confirm);
                    normalise::views::render_page_shell(&state)
                }
                Some((slug, "reject")) => {
                    normalise::handlers::act(&state, slug, state::Decision::Reject);
                    normalise::views::render_page_shell(&state)
                }
                Some((slug, "skip")) => {
                    normalise::handlers::act(&state, slug, state::Decision::Skip);
                    normalise::views::render_page_shell(&state)
                }
                Some((slug, "undo")) | Some((slug, "unskip")) => {
                    normalise::handlers::undo(&state, slug);
                    normalise::views::render_page_shell(&state)
                }
                _ => html! { p { "Invalid normalise action" } },
            }
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
        (Method::Get, "/transfers" | "/transfers/") => render_page_shell(&state),
        (Method::Get, p) if p.starts_with("/transfers/pair/") => {
            let id = parse_pair_id(p, "/transfers/pair/");
            id.map(|(a, b)| render_detail_fragment(&state, a, b))
                .unwrap_or_else(|| html! { p { "Invalid pair ID" } })
        }
        (Method::Get, p) if p.starts_with("/transfers/queue?") => {
            let params = p.strip_prefix("/transfers/queue?").unwrap_or("");
            let filter = extract_param(params, "filter").unwrap_or("pending".to_string());
            let conf = extract_param(params, "conf").unwrap_or("all".to_string());
            render_queue_fragment(&state, &filter, &conf)
        }
        (Method::Get, "/transfers/queue") => render_queue_fragment(&state, "all", "all"),
        (Method::Get, p) if p.starts_with("/transfers/bulk-prompt?") => {
            let params = p.strip_prefix("/transfers/bulk-prompt?").unwrap_or("");
            let action = extract_param(params, "action").unwrap_or("confirm".to_string());
            render_bulk_prompt_fragment(&state, &action)
        }
        (Method::Get, "/transfers/bulk-buttons") => render_bulk_buttons_fragment(&state),
        // All five transfer pair actions share the path shape
        // `/transfers/pair/<a>-<b>/<verb>`.
        (Method::Post, p) if p.starts_with("/transfers/pair/") => {
            let key = parse_pair_id(p, "/transfers/pair/");
            let verb = p.rsplit('/').next().unwrap_or("");
            match (key, verb) {
                (Some(k), "confirm") => handlers::act(&state, k, Decision::Confirm),
                (Some(k), "reject") => handlers::act(&state, k, Decision::Reject),
                (Some(k), "skip") => handlers::act(&state, k, Decision::Skip),
                (Some(k), "undo") | (Some(k), "unskip") => handlers::undo(&state, k),
                _ => return_invalid_action(),
            }
            render_current_page_locked(&state)
        }
        (Method::Post, "/transfers/bulk-confirm") => {
            handlers::bulk_act(&state, Decision::Confirm);
            render_current_page_locked(&state)
        }
        (Method::Post, "/transfers/bulk-reject") => {
            handlers::bulk_act(&state, Decision::Reject);
            render_current_page_locked(&state)
        }
        (Method::Post, "/transfers/apply") => {
            handlers::apply(&state);
            render_current_page_locked(&state)
        }
        (Method::Post, "/transfers/clear-all-skipped") => {
            handlers::clear_all_skipped(&state);
            render_current_page_locked(&state)
        }
        _ => html! { p { "Not found" } },
    };

    let html_str = response.into_string();
    let resp = Response::from_data(html_str.as_bytes().to_vec())
        .with_header(Header::from_bytes("Content-Type", "text/html; charset=utf-8").unwrap());
    let _ = request.respond(resp);
}

/// Re-render the transfers page from the current AppState (locks state
/// internally). Used after every mutating POST.
fn render_current_page_locked(state: &Arc<Mutex<AppState>>) -> Markup {
    let st = state.lock().unwrap();
    render_current_page(&st)
}

/// Friendly response for malformed action URLs.
fn return_invalid_action() {
    // No-op placeholder: the caller still renders the page shell, which is
    // the friendliest fallback. Reserved as a hook for future error
    // surfacing in the UI.
}

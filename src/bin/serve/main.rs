mod css;
mod handlers;
mod helpers;
mod js;
mod normalise;
mod state;
mod views;

use std::sync::{Arc, Mutex};

use anyhow::Result;
use maud::html;
use tiny_http::{Header, Method, Request, Response, Server};

use pocketsmith_sync::db;

use crate::handlers::{
    handle_action, handle_apply, handle_bulk_action, handle_clear_all_skipped, handle_undo,
    handle_unskip,
};
use crate::helpers::{extract_param, parse_pair_id};
use crate::state::AppState;
use crate::views::{
    render_bulk_buttons_fragment, render_bulk_prompt_fragment, render_detail_fragment,
    render_page_shell, render_queue_fragment,
};

fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let port: u16 = std::env::var("SERVE_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3141);

    let conn = db::initialize("pocketsmith.db")?;
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
        (Method::Post, p) if p.starts_with("/normalise/item/") && p.ends_with("/confirm") => {
            let slug = p.trim_start_matches("/normalise/item/").trim_end_matches("/confirm");
            normalise::handlers::confirm(&state, slug);
            normalise::views::render_page_shell(&state)
        }
        (Method::Post, p) if p.starts_with("/normalise/item/") && p.ends_with("/reject") => {
            let slug = p.trim_start_matches("/normalise/item/").trim_end_matches("/reject");
            normalise::handlers::reject(&state, slug);
            normalise::views::render_page_shell(&state)
        }
        (Method::Post, p) if p.starts_with("/normalise/item/") && p.ends_with("/unskip") => {
            let slug = p.trim_start_matches("/normalise/item/").trim_end_matches("/unskip");
            normalise::handlers::unskip(&state, slug);
            normalise::views::render_page_shell(&state)
        }
        (Method::Post, p) if p.starts_with("/normalise/item/") && p.ends_with("/skip") => {
            let slug = p.trim_start_matches("/normalise/item/").trim_end_matches("/skip");
            normalise::handlers::skip(&state, slug);
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
        (Method::Post, p) if p.contains("/confirm") => handle_action(&state, p, "confirm"),
        (Method::Post, p) if p.contains("/reject") => handle_action(&state, p, "reject"),
        (Method::Post, p) if p.contains("/skip") => handle_action(&state, p, "skip"),
        (Method::Post, "/transfers/bulk-confirm") => handle_bulk_action(&state, "confirm"),
        (Method::Post, "/transfers/bulk-reject") => handle_bulk_action(&state, "reject"),
        (Method::Post, "/transfers/apply") => handle_apply(&state),
        (Method::Post, "/transfers/clear-all-skipped") => handle_clear_all_skipped(&state),
        (Method::Post, p) if p.contains("/unskip") => handle_unskip(&state, p),
        (Method::Post, p) if p.contains("/undo") => handle_undo(&state, p),
        _ => html! { p { "Not found" } },
    };

    let html_str = response.into_string();
    let resp = Response::from_data(html_str.as_bytes().to_vec())
        .with_header(Header::from_bytes("Content-Type", "text/html; charset=utf-8").unwrap());
    let _ = request.respond(resp);
}

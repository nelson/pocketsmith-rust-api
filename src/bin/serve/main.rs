mod css;
mod handlers;
mod helpers;
mod js;
mod state;
mod views;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use maud::html;
use tiny_http::{Header, Method, Request, Response, Server};

use pocketsmith_sync::db;

use crate::handlers::{
    handle_action, handle_bulk_action, handle_clear_all_skipped, handle_undo, handle_unskip,
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
        (Method::Get, "/") => render_page_shell(&state),
        (Method::Get, p) if p.starts_with("/pair/") => {
            let id = parse_pair_id(p, "/pair/");
            id.map(|(a, b)| render_detail_fragment(&state, a, b))
                .unwrap_or_else(|| html! { p { "Invalid pair ID" } })
        }
        (Method::Get, p) if p.starts_with("/queue?") => {
            let params = p.strip_prefix("/queue?").unwrap_or("");
            let filter = extract_param(params, "filter").unwrap_or("pending".to_string());
            let conf = extract_param(params, "conf").unwrap_or("all".to_string());
            render_queue_fragment(&state, &filter, &conf)
        }
        (Method::Get, "/queue") => render_queue_fragment(&state, "all", "all"),
        (Method::Get, p) if p.starts_with("/bulk-prompt?") => {
            let params = p.strip_prefix("/bulk-prompt?").unwrap_or("");
            let action = extract_param(params, "action").unwrap_or("confirm".to_string());
            render_bulk_prompt_fragment(&state, &action)
        }
        (Method::Get, "/bulk-buttons") => render_bulk_buttons_fragment(&state),
        (Method::Post, p) if p.contains("/confirm") => handle_action(&state, p, "confirm"),
        (Method::Post, p) if p.contains("/reject") => handle_action(&state, p, "reject"),
        (Method::Post, p) if p.contains("/skip") => handle_action(&state, p, "skip"),
        (Method::Post, "/bulk-confirm") => handle_bulk_action(&state, "confirm"),
        (Method::Post, "/bulk-reject") => handle_bulk_action(&state, "reject"),
        (Method::Post, "/clear-all-skipped") => handle_clear_all_skipped(&state),
        (Method::Post, p) if p.contains("/unskip") => handle_unskip(&state, p),
        (Method::Post, p) if p.contains("/undo") => handle_undo(&state, p),
        _ => html! { p { "Not found" } },
    };

    let html_str = response.into_string();
    let resp = Response::from_data(html_str.as_bytes().to_vec())
        .with_header(Header::from_bytes("Content-Type", "text/html; charset=utf-8").unwrap());
    let _ = request.respond(resp);
}

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
    action_handler, clear_all_skipped, detail_fragment, page_shell, queue_fragment, undo_handler,
    unskip_handler,
};
use crate::helpers::{extract_param, parse_pair_id};
use crate::state::AppState;

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

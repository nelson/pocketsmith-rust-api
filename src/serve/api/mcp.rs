//! Stateless Streamable HTTP transport for the reporting MCP server.

use std::sync::{Arc, Mutex};

use serde::Deserialize;
use serde_json::{json, Value};
use tiny_http::{Header, Method, Request, Response, StatusCode};

use super::{analytical_query, balances, status, transactions, with_connection, ApiError};
use crate::serve::state::AppState;

const PROTOCOL_VERSION: &str = "2025-06-18";

#[derive(Debug, Deserialize)]
struct RpcRequest {
    jsonrpc: String,
    #[serde(default)]
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Deserialize)]
struct ToolCall {
    name: String,
    #[serde(default)]
    arguments: Value,
}

pub(super) fn handle(mut request: Request, state: &Arc<Mutex<AppState>>) {
    if request.method() != &Method::Post {
        respond_rpc(
            request,
            405,
            json!({
                "jsonrpc": "2.0",
                "id": null,
                "error": { "code": -32600, "message": "MCP requests must use POST" }
            }),
        );
        return;
    }

    let body = match super::read_limited_body(&mut request) {
        Ok(body) => body,
        Err(error) => {
            respond_rpc(request, error.status, rpc_error(Value::Null, -32600, &error.message));
            return;
        }
    };
    let rpc: RpcRequest = match serde_json::from_slice(&body) {
        Ok(rpc) => rpc,
        Err(_) => {
            respond_rpc(request, 400, rpc_error(Value::Null, -32700, "Parse error"));
            return;
        }
    };

    if rpc.jsonrpc != "2.0" {
        respond_rpc(
            request,
            400,
            rpc_error(rpc.id.unwrap_or(Value::Null), -32600, "Invalid Request"),
        );
        return;
    }

    // Notifications deliberately have no response body. Streamable HTTP uses
    // 202 Accepted for a notification that has been processed.
    let Some(id) = rpc.id else {
        let response = Response::empty(StatusCode(202))
            .with_header(Header::from_bytes("Cache-Control", "no-store").unwrap());
        let _ = request.respond(response);
        return;
    };

    let result = dispatch(&rpc.method, rpc.params, state);
    let payload = match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err((code, message)) => rpc_error(id, code, &message),
    };
    respond_rpc(request, 200, payload);
}

fn dispatch(
    method: &str,
    params: Value,
    state: &Arc<Mutex<AppState>>,
) -> Result<Value, (i64, String)> {
    match method {
        "initialize" => Ok(json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": { "tools": { "listChanged": false } },
            "serverInfo": {
                "name": "pocketsmith-reporting",
                "title": "PocketSmith Reporting",
                "version": env!("CARGO_PKG_VERSION")
            },
            "instructions": "This server provides private, read-only PocketSmith data. Check reporting status before financial analysis. Treat transfers separately from spending. Monetary values are decimal currency units. Never imply that these tools can modify PocketSmith."
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tool_definitions() })),
        "tools/call" => call_tool(params, state),
        _ => Err((-32601, "Method not found".to_string())),
    }
}

fn call_tool(params: Value, state: &Arc<Mutex<AppState>>) -> Result<Value, (i64, String)> {
    let call: ToolCall = serde_json::from_value(params)
        .map_err(|error| (-32602, format!("Invalid tool call: {error}")))?;

    let result = match call.name.as_str() {
        "get_reporting_status" => require_empty_arguments(&call.arguments)
            .and_then(|()| with_connection(state, status)),
        "get_account_balances" => require_empty_arguments(&call.arguments)
            .and_then(|()| with_connection(state, balances)),
        "find_transactions" => transaction_query(&call.arguments)
            .and_then(|query| with_connection(state, |conn| transactions(conn, &query))),
        "query_financial_database" => serde_json::to_vec(&call.arguments)
            .map_err(ApiError::from)
            .and_then(|body| with_connection(state, |conn| analytical_query(conn, &body))),
        _ => return Err((-32602, format!("Unknown tool: {}", call.name))),
    };

    Ok(match result {
        Ok(value) => tool_result(value),
        Err(error) => json!({
            "content": [{ "type": "text", "text": error.message }],
            "isError": true
        }),
    })
}

fn require_empty_arguments(arguments: &Value) -> Result<(), ApiError> {
    match arguments {
        Value::Null => Ok(()),
        Value::Object(values) if values.is_empty() => Ok(()),
        _ => Err(ApiError::new(400, "this tool does not accept arguments")),
    }
}

fn transaction_query(arguments: &Value) -> Result<String, ApiError> {
    let object = arguments
        .as_object()
        .ok_or_else(|| ApiError::new(400, "arguments must be an object"))?;
    const ALLOWED: &[&str] = &["from", "to", "account_id", "category_id", "limit"];
    if let Some(key) = object.keys().find(|key| !ALLOWED.contains(&key.as_str())) {
        return Err(ApiError::new(400, format!("unknown argument: {key}")));
    }

    let mut parts = Vec::new();
    for key in ["from", "to"] {
        if let Some(value) = object.get(key) {
            let value = value
                .as_str()
                .ok_or_else(|| ApiError::new(400, format!("{key} must be a string")))?;
            parts.push(format!("{key}={value}"));
        }
    }
    for key in ["account_id", "category_id", "limit"] {
        if let Some(value) = object.get(key) {
            let value = value
                .as_i64()
                .ok_or_else(|| ApiError::new(400, format!("{key} must be an integer")))?;
            parts.push(format!("{key}={value}"));
        }
    }
    Ok(parts.join("&"))
}

fn tool_result(value: Value) -> Value {
    let text = serde_json::to_string(&value).unwrap_or_else(|_| "{}".to_string());
    json!({
        "content": [{ "type": "text", "text": text }],
        "structuredContent": value,
        "isError": false
    })
}

fn tool_definitions() -> Value {
    let annotations = json!({
        "readOnlyHint": true,
        "destructiveHint": false,
        "idempotentHint": true,
        "openWorldHint": false
    });
    let security_schemes = json!([
        { "type": "oauth2", "scopes": ["reporting:read"] }
    ]);
    json!([
        {
            "name": "get_reporting_status",
            "title": "Get reporting status",
            "description": "Check SQLite integrity, transaction count, and PocketSmith sync freshness before reporting.",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false },
            "outputSchema": { "type": "object", "additionalProperties": true },
            "securitySchemes": security_schemes,
            "_meta": { "securitySchemes": security_schemes },
            "annotations": annotations
        },
        {
            "name": "get_account_balances",
            "title": "Get account balances",
            "description": "List the latest balances for every PocketSmith account without exposing account numbers.",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false },
            "outputSchema": { "type": "object", "additionalProperties": true },
            "securitySchemes": security_schemes,
            "_meta": { "securitySchemes": security_schemes },
            "annotations": annotations
        },
        {
            "name": "find_transactions",
            "title": "Find transactions",
            "description": "Find up to 500 transactions using inclusive dates and optional account or category filters.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "from": { "type": "string", "format": "date", "description": "Inclusive YYYY-MM-DD date." },
                    "to": { "type": "string", "format": "date", "description": "Inclusive YYYY-MM-DD date." },
                    "account_id": { "type": "integer" },
                    "category_id": { "type": "integer" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 500, "default": 100 }
                },
                "additionalProperties": false
            },
            "outputSchema": { "type": "object", "additionalProperties": true },
            "securitySchemes": security_schemes,
            "_meta": { "securitySchemes": security_schemes },
            "annotations": annotations
        },
        {
            "name": "query_financial_database",
            "title": "Query financial database",
            "description": "Run one parameterized read-only SQLite SELECT or CTE for flexible financial analysis. Limited to 500 rows, 1 MiB, and two seconds.",
            "inputSchema": {
                "type": "object",
                "required": ["sql"],
                "properties": {
                    "sql": { "type": "string", "description": "A single SQLite SELECT or WITH statement. Use ? placeholders for parameters." },
                    "params": {
                        "type": "array",
                        "items": { "type": ["string", "number", "integer", "boolean", "null"] },
                        "default": []
                    }
                },
                "additionalProperties": false
            },
            "outputSchema": { "type": "object", "additionalProperties": true },
            "securitySchemes": security_schemes,
            "_meta": { "securitySchemes": security_schemes },
            "annotations": annotations
        }
    ])
}

fn rpc_error(id: Value, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    })
}

fn respond_rpc(request: Request, status: u16, value: Value) {
    let body = serde_json::to_vec(&value).unwrap_or_default();
    let response = Response::from_data(body)
        .with_status_code(StatusCode(status))
        .with_header(Header::from_bytes("Content-Type", "application/json").unwrap())
        .with_header(Header::from_bytes("Cache-Control", "no-store").unwrap())
        .with_header(Header::from_bytes("X-Content-Type-Options", "nosniff").unwrap())
        .with_header(Header::from_bytes("MCP-Protocol-Version", PROTOCOL_VERSION).unwrap());
    let _ = request.respond(response);
}

#[cfg(test)]
mod tests {
    use pocketsmith::db;

    use super::*;

    fn state() -> Arc<Mutex<AppState>> {
        Arc::new(Mutex::new(AppState::new(db::initialize_in_memory().unwrap())))
    }

    #[test]
    fn initialize_advertises_read_only_server() {
        let result = dispatch("initialize", json!({}), &state()).unwrap();
        assert_eq!(result["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(result["capabilities"]["tools"]["listChanged"], false);
    }

    #[test]
    fn lists_four_safely_annotated_tools() {
        let result = dispatch("tools/list", json!({}), &state()).unwrap();
        let tools = result["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 4);
        assert!(tools.iter().all(|tool| tool["annotations"]["readOnlyHint"] == true));
        assert!(tools.iter().all(|tool| tool["annotations"]["destructiveHint"] == false));
        assert!(tools.iter().all(|tool| {
            tool["securitySchemes"]
                == json!([{ "type": "oauth2", "scopes": ["reporting:read"] }])
        }));
        assert!(tools.iter().all(|tool| {
            tool["_meta"]["securitySchemes"] == tool["securitySchemes"]
        }));
    }

    #[test]
    fn calls_status_with_structured_content() {
        let result = dispatch(
            "tools/call",
            json!({ "name": "get_reporting_status", "arguments": {} }),
            &state(),
        )
        .unwrap();
        assert_eq!(result["isError"], false);
        assert_eq!(result["structuredContent"]["status"], "ok");
    }

    #[test]
    fn write_query_is_returned_as_tool_error() {
        let result = dispatch(
            "tools/call",
            json!({
                "name": "query_financial_database",
                "arguments": { "sql": "DELETE FROM transactions" }
            }),
            &state(),
        )
        .unwrap();
        assert_eq!(result["isError"], true);
        assert!(result["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("only SELECT"));
    }
}

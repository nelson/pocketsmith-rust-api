#!/usr/bin/env python3
"""Exercise the complete local OAuth + MCP flow without logging credentials."""

import base64
import hashlib
import json
import os
import time
import urllib.error
import urllib.parse
import urllib.request


ORIGIN = "http://127.0.0.1:8080"
EXTERNAL_ORIGIN = os.environ["REPORTING_BASE_URL"].rstrip("/")
RESOURCE = f"{EXTERNAL_ORIGIN}/api/v1/mcp"
PASSWORD = os.environ["REPORTING_OAUTH_PASSWORD"]
CALLBACK = "https://chatgpt.com/oauth/callback"
REQUIRE_CALLBACK_FORM_ACTION = os.environ.get("REQUIRE_CALLBACK_FORM_ACTION") == "1"


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


def request(path, *, data=None, headers=None, expected=200, no_redirect=False):
    headers = headers or {}
    encoded = None
    if isinstance(data, dict):
        if headers.get("Content-Type") == "application/json":
            encoded = json.dumps(data).encode()
        else:
            encoded = urllib.parse.urlencode(data).encode()
    elif data is not None:
        encoded = json.dumps(data).encode()
    req = urllib.request.Request(
        f"{ORIGIN}{path}",
        data=encoded,
        headers=headers,
    )
    opener = urllib.request.build_opener(NoRedirect()) if no_redirect else urllib.request.build_opener()
    try:
        response = opener.open(req, timeout=10)
    except urllib.error.HTTPError as error:
        response = error
    expected_statuses = (expected,) if isinstance(expected, int) else expected
    if response.status not in expected_statuses:
        raise RuntimeError(
            f"{path} returned HTTP {response.status}, expected {expected_statuses}"
        )
    return response


for protected_resource_path in (
    "/.well-known/oauth-protected-resource/api/v1/mcp",
    "/.well-known/oauth-protected-resource/mcp",
    "/.well-known/oauth-protected-resource",
):
    protected = json.load(request(protected_resource_path))
    assert protected["resource"] == RESOURCE
    assert protected["scopes_supported"] == ["reporting:read"]

metadata = json.load(request("/.well-known/oauth-authorization-server"))
assert metadata["issuer"] == EXTERNAL_ORIGIN
assert metadata["code_challenge_methods_supported"] == ["S256"]

registration = json.load(
    request(
        "/oauth/register",
        data={
            "redirect_uris": [CALLBACK],
            "client_name": "PocketSmith release smoke",
            "token_endpoint_auth_method": "none",
            "grant_types": ["authorization_code", "refresh_token"],
            "response_types": ["code"],
        },
        headers={"Content-Type": "application/json"},
        expected=201,
    )
)
client_id = registration["client_id"]
verifier = "oauth-smoke-verifier-abcdefghijklmnopqrstuvwxyz-0123456789"
challenge = base64.urlsafe_b64encode(hashlib.sha256(verifier.encode()).digest()).rstrip(b"=").decode()
authorization = {
    "response_type": "code",
    "client_id": client_id,
    "redirect_uri": CALLBACK,
    "state": "release-smoke-state",
    "code_challenge": challenge,
    "code_challenge_method": "S256",
    "scope": "reporting:read",
    "resource": RESOURCE,
}

query = urllib.parse.urlencode(authorization)
authorization_page = request(f"/oauth/authorize?{query}")
if REQUIRE_CALLBACK_FORM_ACTION:
    policy = authorization_page.headers["Content-Security-Policy"]
    callback_origin = urllib.parse.urlsplit(CALLBACK)
    expected_origin = f"{callback_origin.scheme}://{callback_origin.netloc}"
    assert f"form-action 'self' {expected_origin};" in policy
authorization["password"] = PASSWORD
redirect = request(
    "/oauth/authorize",
    data=authorization,
    headers={"Content-Type": "application/x-www-form-urlencoded"},
    expected=303 if REQUIRE_CALLBACK_FORM_ACTION else (302, 303),
    no_redirect=True,
)
location = redirect.headers["Location"]
redirect_params = urllib.parse.parse_qs(urllib.parse.urlparse(location).query)
assert redirect_params["state"] == ["release-smoke-state"]
code = redirect_params["code"][0]

tokens = json.load(
    request(
        "/oauth/token",
        data={
            "grant_type": "authorization_code",
            "client_id": client_id,
            "redirect_uri": CALLBACK,
            "code": code,
            "code_verifier": verifier,
        },
        headers={"Content-Type": "application/x-www-form-urlencoded"},
    )
)
access_token = tokens["access_token"]
refresh_token = tokens["refresh_token"]

mcp_headers = {
    "Authorization": f"Bearer {access_token}",
    "Content-Type": "application/json",
    "Accept": "application/json, text/event-stream",
}
initialized = json.load(
    request(
        "/api/v1/mcp",
        data={
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "release-smoke", "version": "1.0"},
            },
        },
        headers=mcp_headers,
    )
)
assert initialized["result"]["protocolVersion"] == "2025-06-18"

notification = request(
    "/api/v1/mcp",
    data={
        "jsonrpc": "2.0",
        "method": "notifications/initialized",
        "params": {},
    },
    headers=mcp_headers,
    expected=202,
)
assert notification.read() == b""

listed = json.load(
    request(
        "/api/v1/mcp",
        data={
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {},
        },
        headers=mcp_headers,
    )
)
tools = listed["result"]["tools"]
assert {tool["name"] for tool in tools} == {
    "get_reporting_status",
    "get_account_balances",
    "find_transactions",
    "query_financial_database",
}
for tool in tools:
    assert tool["inputSchema"]["type"] == "object"
    assert tool["annotations"]["readOnlyHint"] is True
    assert tool["annotations"]["destructiveHint"] is False
    assert tool["securitySchemes"] == [
        {"type": "oauth2", "scopes": ["reporting:read"]}
    ]
    assert tool["_meta"]["securitySchemes"] == tool["securitySchemes"]

result = json.load(
    request(
        "/api/v1/mcp",
        data={
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {"name": "get_reporting_status", "arguments": {}},
        },
        headers=mcp_headers,
    )
)
assert result["result"]["structuredContent"]["status"] == "ok"

refreshed = json.load(
    request(
        "/oauth/token",
        data={
            "grant_type": "refresh_token",
            "client_id": client_id,
            "refresh_token": refresh_token,
        },
        headers={"Content-Type": "application/x-www-form-urlencoded"},
    )
)
assert refreshed["access_token"] != access_token

started = time.monotonic()
unauthorized = request(
    "/api/v1/mcp",
    data={"jsonrpc": "2.0", "id": 2, "method": "initialize", "params": {}},
    headers={"Content-Type": "application/json"},
    expected=401,
)
assert time.monotonic() - started < 1.0
assert "oauth-protected-resource" in unauthorized.headers["WWW-Authenticate"]
assert "/.well-known/oauth-protected-resource/api/v1/mcp" in unauthorized.headers[
    "WWW-Authenticate"
]

print("OAuth discovery, PKCE, token refresh, and protected MCP call verified")

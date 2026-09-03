#!/usr/bin/env python3
"""Exercise the complete local OAuth + MCP flow without logging credentials."""

import base64
import hashlib
import json
import os
import urllib.error
import urllib.parse
import urllib.request


ORIGIN = "http://127.0.0.1:8080"
EXTERNAL_ORIGIN = os.environ["REPORTING_BASE_URL"].rstrip("/")
RESOURCE = f"{EXTERNAL_ORIGIN}/mcp"
PASSWORD = os.environ["REPORTING_OAUTH_PASSWORD"]
CALLBACK = "https://chatgpt.com/oauth/callback"


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
    if response.status != expected:
        raise RuntimeError(f"{path} returned HTTP {response.status}, expected {expected}")
    return response


protected = json.load(request("/.well-known/oauth-protected-resource"))
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
request(f"/oauth/authorize?{query}")
authorization["password"] = PASSWORD
redirect = request(
    "/oauth/authorize",
    data=authorization,
    headers={"Content-Type": "application/x-www-form-urlencoded"},
    expected=302,
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
result = json.load(
    request(
        "/mcp",
        data={
            "jsonrpc": "2.0",
            "id": 1,
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

unauthorized = request(
    "/mcp",
    data={"jsonrpc": "2.0", "id": 2, "method": "initialize", "params": {}},
    headers={"Content-Type": "application/json"},
    expected=401,
)
assert "oauth-protected-resource" in unauthorized.headers["WWW-Authenticate"]

print("OAuth discovery, PKCE, token refresh, and protected MCP call verified")

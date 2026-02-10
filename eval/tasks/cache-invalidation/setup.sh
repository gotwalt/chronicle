#!/usr/bin/env bash
# Creates a Python project with a cache invalidation bug.
# Usage: setup.sh <repo_dir>
set -euo pipefail

REPO="$1"
mkdir -p "$REPO"
cd "$REPO"

git init
git config user.email "eval@chronicle.dev"
git config user.name "Chronicle Eval"

export GIT_AUTHOR_DATE="2025-01-15T10:00:00+00:00"
export GIT_COMMITTER_DATE="2025-01-15T10:00:00+00:00"

mkdir -p src tests

# ── Application config ──
cat > src/__init__.py << 'PYEOF'
PYEOF

cat > src/app_config.py << 'PYEOF'
"""Application configuration.

Controls global behavior including output format, which affects
how HTTP requests are made (via the Accept header).
"""


class AppConfig:
    """Global application configuration.

    output_format controls what format the app requests from APIs.
    Changing this at runtime (e.g., via settings UI) should cause
    subsequent API calls to return the new format.
    """

    def __init__(self, output_format: str = "json"):
        self._output_format = output_format
        self._listeners = []

    @property
    def output_format(self) -> str:
        return self._output_format

    @output_format.setter
    def output_format(self, value: str):
        old = self._output_format
        self._output_format = value
        if old != value:
            for listener in self._listeners:
                listener(old, value)

    def on_change(self, callback):
        """Register a callback for config changes."""
        self._listeners.append(callback)
PYEOF

# ── HTTP client with cache ──
cat > src/http_client.py << 'PYEOF'
"""Cached HTTP client.

Wraps a simple HTTP interface with a response cache to reduce
redundant network calls. Cache keys are derived from the request.

BUG: The cache key only considers URL + method, but the server's
response depends on the Accept header (derived from AppConfig.output_format).
When output_format changes, stale cached responses are returned.
"""

import time
import hashlib
from typing import Optional


class HttpResponse:
    """Simple HTTP response container."""

    def __init__(self, status: int, body: str, headers: dict[str, str] | None = None):
        self.status = status
        self.body = body
        self.headers = headers or {}

    def __repr__(self):
        return f"HttpResponse(status={self.status}, body={self.body[:50]!r})"


class CacheEntry:
    """A cached response with expiration."""

    def __init__(self, response: HttpResponse, ttl_seconds: int = 300):
        self.response = response
        self.expires_at = time.time() + ttl_seconds
        self.created_at = time.time()

    @property
    def expired(self) -> bool:
        return time.time() > self.expires_at


class CachedHttpClient:
    """HTTP client with response caching.

    The cache key is derived from the request URL and method.
    Responses are cached for `default_ttl` seconds.

    The client uses AppConfig to determine request headers,
    particularly the Accept header which controls response format.
    """

    def __init__(self, config, transport=None, default_ttl: int = 300):
        self._config = config
        self._transport = transport or self._default_transport
        self._cache: dict[str, CacheEntry] = {}
        self._default_ttl = default_ttl
        self._stats = {"hits": 0, "misses": 0}

    def get(self, url: str) -> HttpResponse:
        """GET a URL, using cache if available."""
        return self._request("GET", url)

    def post(self, url: str, body: str = "") -> HttpResponse:
        """POST to a URL. Never cached."""
        headers = self._build_headers()
        return self._transport("POST", url, headers, body)

    def _request(self, method: str, url: str) -> HttpResponse:
        """Execute a request with caching."""
        cache_key = self._make_cache_key(method, url)

        # Check cache
        entry = self._cache.get(cache_key)
        if entry and not entry.expired:
            self._stats["hits"] += 1
            return entry.response

        # Cache miss — make actual request
        self._stats["misses"] += 1
        headers = self._build_headers()
        response = self._transport(method, url, headers)

        # Cache the response
        if response.status == 200:
            self._cache[cache_key] = CacheEntry(
                response, self._default_ttl
            )

        return response

    def _make_cache_key(self, method: str, url: str) -> str:
        """Generate a cache key from request parameters.

        BUG: Only considers method + URL. Ignores Accept header,
        so changing output_format returns stale cached responses.
        """
        raw = f"{method}:{url}"
        return hashlib.sha256(raw.encode()).hexdigest()[:16]

    def _build_headers(self) -> dict[str, str]:
        """Build request headers from current config.

        The Accept header is derived from output_format.
        This is the invisible coupling: cache correctness depends
        on these headers, but the cache key doesn't include them.
        """
        format_map = {
            "json": "application/json",
            "xml": "application/xml",
            "csv": "text/csv",
        }
        accept = format_map.get(
            self._config.output_format, "application/json"
        )
        return {
            "Accept": accept,
            "User-Agent": "CachedClient/1.0",
        }

    def _default_transport(
        self, method: str, url: str, headers: dict, body: str = ""
    ) -> HttpResponse:
        """Default transport (stub for testing)."""
        raise NotImplementedError("No transport configured")

    def clear_cache(self):
        """Clear all cached entries."""
        self._cache.clear()

    @property
    def cache_size(self) -> int:
        return len(self._cache)

    @property
    def stats(self) -> dict:
        return dict(self._stats)
PYEOF

# ── API client that uses the cached HTTP client ──
cat > src/api_client.py << 'PYEOF'
"""High-level API client built on CachedHttpClient.

Demonstrates the user-facing impact of the cache bug: switching
output_format doesn't take effect until cache entries expire.
"""


class ApiClient:
    """Client for a data API.

    Uses CachedHttpClient for transport. The expected behavior is
    that changing AppConfig.output_format immediately causes
    subsequent API calls to return the new format.
    """

    def __init__(self, http_client, base_url: str = "https://api.example.com"):
        self._http = http_client
        self._base_url = base_url

    def get_users(self) -> str:
        """Fetch user list. Format depends on output_format config."""
        response = self._http.get(f"{self._base_url}/users")
        return response.body

    def get_user(self, user_id: int) -> str:
        """Fetch a single user."""
        response = self._http.get(f"{self._base_url}/users/{user_id}")
        return response.body

    def get_reports(self) -> str:
        """Fetch reports list."""
        response = self._http.get(f"{self._base_url}/reports")
        return response.body
PYEOF

# ── Tests ──
cat > tests/__init__.py << 'PYEOF'
PYEOF

cat > tests/test_cache.py << 'PYEOF'
"""Tests for the cached HTTP client."""

import os
import sys
import time
import pytest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from src.app_config import AppConfig
from src.http_client import CachedHttpClient, HttpResponse
from src.api_client import ApiClient


def make_transport(responses: dict):
    """Create a mock transport that returns format-appropriate responses.

    The mock simulates a real server: it reads the Accept header and
    returns content in the requested format. This means the same URL
    returns different content depending on the Accept header.
    """
    call_log = []

    def transport(method, url, headers, body=""):
        call_log.append({"method": method, "url": url, "headers": headers})
        accept = headers.get("Accept", "application/json")

        key = f"{method}:{url}"
        base_data = responses.get(key, {"default": True})

        if "json" in accept:
            import json
            body_str = json.dumps(base_data)
            vary = "Accept"
        elif "xml" in accept:
            items = "".join(
                f"<{k}>{v}</{k}>" for k, v in base_data.items()
            )
            body_str = f"<response>{items}</response>"
            vary = "Accept"
        elif "csv" in accept:
            header = ",".join(base_data.keys())
            values = ",".join(str(v) for v in base_data.values())
            body_str = f"{header}\n{values}"
            vary = "Accept"
        else:
            body_str = str(base_data)
            vary = ""

        resp_headers = {"Content-Type": accept}
        if vary:
            resp_headers["Vary"] = vary

        return HttpResponse(200, body_str, resp_headers)

    return transport, call_log


class TestBasicCaching:
    """Basic cache behavior tests (these should pass already)."""

    def test_cache_hit(self):
        config = AppConfig(output_format="json")
        responses = {"GET:https://api.example.com/users": {"users": ["alice", "bob"]}}
        transport, log = make_transport(responses)

        client = CachedHttpClient(config, transport)
        resp1 = client.get("https://api.example.com/users")
        resp2 = client.get("https://api.example.com/users")

        assert resp1.body == resp2.body
        assert len(log) == 1  # only one actual request
        assert client.stats["hits"] == 1
        assert client.stats["misses"] == 1

    def test_different_urls_cached_separately(self):
        config = AppConfig(output_format="json")
        responses = {
            "GET:https://api.example.com/a": {"path": "a"},
            "GET:https://api.example.com/b": {"path": "b"},
        }
        transport, log = make_transport(responses)

        client = CachedHttpClient(config, transport)
        resp_a = client.get("https://api.example.com/a")
        resp_b = client.get("https://api.example.com/b")

        assert resp_a.body != resp_b.body
        assert len(log) == 2

    def test_expired_entry_refetched(self):
        config = AppConfig(output_format="json")
        responses = {"GET:https://api.example.com/data": {"value": 42}}
        transport, log = make_transport(responses)

        client = CachedHttpClient(config, transport, default_ttl=0)
        client.get("https://api.example.com/data")
        time.sleep(0.01)
        client.get("https://api.example.com/data")

        assert len(log) == 2  # both should hit transport


class TestFormatSwitching:
    """Tests that expose the stale-cache bug.

    These tests will FAIL until the bug is fixed.
    """

    def test_format_change_returns_new_format(self):
        """Changing output_format should cause next request to return new format."""
        config = AppConfig(output_format="json")
        responses = {"GET:https://api.example.com/users": {"name": "alice", "age": "30"}}
        transport, log = make_transport(responses)

        client = CachedHttpClient(config, transport)

        # First request — JSON format
        resp1 = client.get("https://api.example.com/users")
        assert "alice" in resp1.body
        assert resp1.body.startswith("{")  # JSON

        # Switch to XML
        config.output_format = "xml"

        # Second request — should return XML, not cached JSON
        resp2 = client.get("https://api.example.com/users")
        assert "<response>" in resp2.body  # Should be XML

    def test_format_switch_and_back(self):
        """Switching format and back should use correct format each time."""
        config = AppConfig(output_format="json")
        responses = {"GET:https://api.example.com/data": {"key": "value"}}
        transport, log = make_transport(responses)

        client = CachedHttpClient(config, transport)

        # JSON
        r1 = client.get("https://api.example.com/data")
        assert r1.body.startswith("{")

        # Switch to CSV
        config.output_format = "csv"
        r2 = client.get("https://api.example.com/data")
        assert "," in r2.body
        assert not r2.body.startswith("{")

        # Back to JSON
        config.output_format = "json"
        r3 = client.get("https://api.example.com/data")
        assert r3.body.startswith("{")

    def test_api_client_respects_format_change(self):
        """High-level API client should reflect format changes."""
        config = AppConfig(output_format="json")
        responses = {"GET:https://api.example.com/users": {"users": "list"}}
        transport, log = make_transport(responses)

        http = CachedHttpClient(config, transport)
        api = ApiClient(http)

        json_result = api.get_users()
        assert "{" in json_result

        config.output_format = "xml"
        xml_result = api.get_users()
        assert "<response>" in xml_result

    def test_only_format_dependent_entries_invalidated(self):
        """Entries that don't depend on format should remain cached.

        This is the advanced test: a naive fix (clear entire cache on
        config change) works but is wasteful. The ideal fix only
        invalidates entries whose responses vary by Accept header.
        """
        config = AppConfig(output_format="json")
        call_count = {"a": 0, "b": 0}

        def transport(method, url, headers, body=""):
            if "/static" in url:
                call_count["a"] += 1
                # Static endpoint: same response regardless of Accept
                return HttpResponse(200, "static-data", {
                    "Content-Type": "text/plain"
                })
            else:
                call_count["b"] += 1
                accept = headers.get("Accept", "")
                if "xml" in accept:
                    return HttpResponse(200, "<data/>", {
                        "Content-Type": "application/xml",
                        "Vary": "Accept",
                    })
                return HttpResponse(200, '{"data": 1}', {
                    "Content-Type": "application/json",
                    "Vary": "Accept",
                })

        client = CachedHttpClient(config, transport)

        # Fetch both endpoints
        client.get("https://api.example.com/static")
        client.get("https://api.example.com/dynamic")

        assert call_count == {"a": 1, "b": 1}

        # Change format
        config.output_format = "xml"

        # Static should still be cached; dynamic should refetch
        client.get("https://api.example.com/static")
        client.get("https://api.example.com/dynamic")

        # Ideal: static stays cached (a=1), dynamic refetched (b=2)
        # Acceptable: both refetched (a=2, b=2)
        assert call_count["b"] == 2, "Dynamic endpoint should be refetched after format change"
PYEOF

cat > pytest.ini << 'PYEOF'
[pytest]
testpaths = tests
PYEOF

cat > pyproject.toml << 'PYEOF'
[project]
name = "cache-invalidation"
version = "0.1.0"
requires-python = ">=3.11"
dependencies = ["pytest"]
PYEOF

git add -A
git commit -m "Initial project: HTTP client with response caching

Cached HTTP client that wraps transport with TTL-based caching.
Cache key derived from URL and method. Known issue: cache doesn't
account for Accept header differences."

git tag eval-setup-complete

echo "✓ cache-invalidation task repo created at $REPO"

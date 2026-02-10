#!/usr/bin/env bash
# Creates a Python project with a retry/jitter thundering-herd bug.
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

cat > src/__init__.py << 'PYEOF'
PYEOF

# ── Retry executor with jitter bug ──
cat > src/retry.py << 'PYEOF'
"""Retry executor with exponential backoff and jitter.

Provides resilient execution of operations that may fail transiently.
Uses exponential backoff with jitter to spread out retry attempts
and avoid thundering herd problems.

BUG: The jitter is applied per-retry-delay, but all clients share the
same INITIAL_DELAY_MS. After a simultaneous failure (e.g., server
outage recovery), every client's first retry fires at approximately
the same time (INITIAL_DELAY_MS ± small jitter). The jitter only
diverges retry timing on subsequent attempts, but by then the
server is already overwhelmed by the first wave.
"""

import time
import random
from typing import Callable, TypeVar, Optional

T = TypeVar("T")

# Base delay before first retry
INITIAL_DELAY_MS = 100

# Maximum delay cap
MAX_DELAY_MS = 30_000

# Backoff multiplier
BACKOFF_FACTOR = 2.0

# Jitter range: delay ± JITTER_FRACTION * delay
JITTER_FRACTION = 0.25


class RetryError(Exception):
    """Raised when all retry attempts are exhausted."""

    def __init__(self, message: str, attempts: int, last_error: Exception):
        super().__init__(message)
        self.attempts = attempts
        self.last_error = last_error


class RetryExecutor:
    """Executes operations with retry logic.

    Uses exponential backoff: delay doubles after each failure,
    with random jitter to prevent thundering herd.

    The jitter approach uses "equal jitter": take the computed delay,
    then randomly adjust by ± JITTER_FRACTION. This should spread
    out retries from multiple clients.
    """

    def __init__(
        self,
        max_retries: int = 5,
        initial_delay_ms: int = INITIAL_DELAY_MS,
        max_delay_ms: int = MAX_DELAY_MS,
        backoff_factor: float = BACKOFF_FACTOR,
        jitter_fraction: float = JITTER_FRACTION,
    ):
        self.max_retries = max_retries
        self.initial_delay_ms = initial_delay_ms
        self.max_delay_ms = max_delay_ms
        self.backoff_factor = backoff_factor
        self.jitter_fraction = jitter_fraction
        self._attempt_log: list[dict] = []

    def execute(self, operation: Callable[[], T]) -> T:
        """Execute an operation, retrying on failure.

        Returns the result on success.
        Raises RetryError after max_retries failures.
        """
        last_error = None

        for attempt in range(self.max_retries + 1):
            try:
                result = operation()
                self._attempt_log.append({
                    "attempt": attempt,
                    "success": True,
                    "timestamp": time.time(),
                })
                return result
            except Exception as e:
                last_error = e
                self._attempt_log.append({
                    "attempt": attempt,
                    "success": False,
                    "timestamp": time.time(),
                    "error": str(e),
                })

                if attempt < self.max_retries:
                    delay = self._compute_delay(attempt)
                    time.sleep(delay / 1000.0)

        raise RetryError(
            f"Operation failed after {self.max_retries + 1} attempts",
            attempts=self.max_retries + 1,
            last_error=last_error,
        )

    def _compute_delay(self, attempt: int) -> float:
        """Compute delay in milliseconds for a given attempt number.

        Uses exponential backoff with equal jitter:
          base = initial_delay * backoff_factor ^ attempt
          delay = base ± (base * jitter_fraction)

        BUG: When attempt=0 for all clients simultaneously,
        base = initial_delay_ms = 100 for everyone. The jitter
        of ±25% means all first retries land in [75ms, 125ms] —
        a 50ms window that doesn't spread out 100+ clients.
        """
        base = self.initial_delay_ms * (self.backoff_factor ** attempt)
        base = min(base, self.max_delay_ms)

        jitter_range = base * self.jitter_fraction
        delay = base + random.uniform(-jitter_range, jitter_range)

        return max(delay, 1.0)  # minimum 1ms

    @property
    def attempt_log(self) -> list[dict]:
        """Log of all attempts with timestamps."""
        return list(self._attempt_log)

    def reset(self):
        """Clear the attempt log."""
        self._attempt_log.clear()


class RetryPolicy:
    """Predefined retry policies for common scenarios."""

    @staticmethod
    def aggressive() -> RetryExecutor:
        """Fast retries, short delays. For low-latency paths."""
        return RetryExecutor(
            max_retries=3,
            initial_delay_ms=50,
            max_delay_ms=1000,
            backoff_factor=2.0,
            jitter_fraction=0.1,
        )

    @staticmethod
    def conservative() -> RetryExecutor:
        """Slow retries, long delays. For rate-limited APIs."""
        return RetryExecutor(
            max_retries=5,
            initial_delay_ms=1000,
            max_delay_ms=60_000,
            backoff_factor=3.0,
            jitter_fraction=0.25,
        )

    @staticmethod
    def default() -> RetryExecutor:
        """Standard retry policy."""
        return RetryExecutor()
PYEOF

# ── Service that uses retry ──
cat > src/service.py << 'PYEOF'
"""Service layer that uses RetryExecutor for resilient operations.

Simulates a service making calls to an unreliable backend.
"""

from src.retry import RetryExecutor, RetryPolicy


class BackendError(Exception):
    pass


class DataService:
    """Fetches data from a backend with retry logic."""

    def __init__(self, backend, retry: RetryExecutor | None = None):
        self._backend = backend
        self._retry = retry or RetryPolicy.default()

    def fetch_data(self, key: str) -> str:
        """Fetch data by key, with retries on transient failures."""
        def operation():
            result = self._backend.get(key)
            if result is None:
                raise BackendError(f"Backend returned None for {key}")
            return result

        return self._retry.execute(operation)

    def fetch_batch(self, keys: list[str]) -> dict[str, str]:
        """Fetch multiple keys. Each key retries independently."""
        results = {}
        for key in keys:
            results[key] = self.fetch_data(key)
        return results
PYEOF

# ── Tests ──
cat > tests/__init__.py << 'PYEOF'
PYEOF

cat > tests/test_retry.py << 'PYEOF'
"""Tests for the retry executor."""

import os
import sys
import time
import random
import pytest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from src.retry import RetryExecutor, RetryError, RetryPolicy


class TestBasicRetry:
    """Basic retry behavior (these should pass already)."""

    def test_success_no_retry(self):
        executor = RetryExecutor(max_retries=3)
        result = executor.execute(lambda: 42)
        assert result == 42
        assert len(executor.attempt_log) == 1

    def test_retry_then_succeed(self):
        call_count = 0

        def flaky():
            nonlocal call_count
            call_count += 1
            if call_count < 3:
                raise ValueError("not yet")
            return "ok"

        executor = RetryExecutor(max_retries=5, initial_delay_ms=1)
        result = executor.execute(flaky)
        assert result == "ok"
        assert call_count == 3

    def test_all_retries_exhausted(self):
        executor = RetryExecutor(max_retries=2, initial_delay_ms=1)

        with pytest.raises(RetryError) as exc_info:
            executor.execute(lambda: (_ for _ in ()).throw(ValueError("fail")))

        assert exc_info.value.attempts == 3

    def test_backoff_increases(self):
        executor = RetryExecutor(
            initial_delay_ms=100,
            backoff_factor=2.0,
            jitter_fraction=0.0,  # no jitter for predictable test
        )
        d0 = executor._compute_delay(0)
        d1 = executor._compute_delay(1)
        d2 = executor._compute_delay(2)

        assert d0 == pytest.approx(100)
        assert d1 == pytest.approx(200)
        assert d2 == pytest.approx(400)

    def test_delay_capped_at_max(self):
        executor = RetryExecutor(
            initial_delay_ms=100,
            max_delay_ms=500,
            backoff_factor=2.0,
            jitter_fraction=0.0,
        )
        d10 = executor._compute_delay(10)
        assert d10 == pytest.approx(500)


class TestRetryPolicies:
    def test_aggressive_policy(self):
        executor = RetryPolicy.aggressive()
        assert executor.max_retries == 3
        assert executor.initial_delay_ms == 50

    def test_conservative_policy(self):
        executor = RetryPolicy.conservative()
        assert executor.max_retries == 5
        assert executor.initial_delay_ms == 1000


class TestThunderingHerd:
    """Tests that expose the thundering herd bug.

    These tests will FAIL until the bug is fixed.
    """

    def test_clients_spread_initial_retry(self):
        """Multiple clients failing simultaneously should NOT all retry
        at the same time. The first retry delays should be well-spread.

        With 20 clients, the range of first-retry delays should span
        at least 80% of the initial_delay_ms to meaningfully spread load.
        """
        random.seed(42)  # deterministic for test
        num_clients = 20

        first_delays = []
        for _ in range(num_clients):
            executor = RetryExecutor(
                initial_delay_ms=100,
                jitter_fraction=0.25,
            )
            delay = executor._compute_delay(attempt=0)
            first_delays.append(delay)

        spread = max(first_delays) - min(first_delays)
        # With the bug: spread is ~50ms (±25% of 100ms)
        # After fix: spread should be >= 80ms (well-distributed)
        assert spread >= 80, (
            f"First retry delays only span {spread:.0f}ms across "
            f"{num_clients} clients — thundering herd not prevented. "
            f"Delays: [{min(first_delays):.0f}, {max(first_delays):.0f}]"
        )

    def test_concurrent_recovery_simulation(self):
        """Simulate N clients all failing at t=0 and retrying.

        After fix, the retry times should be spread across a wide
        window rather than clustered in a narrow band.
        """
        random.seed(123)
        num_clients = 10

        # Track when each client's first retry would fire
        retry_times = []
        for _ in range(num_clients):
            executor = RetryExecutor(
                initial_delay_ms=100,
                jitter_fraction=0.25,
            )
            # Simulate: fail at t=0, compute first retry delay
            delay = executor._compute_delay(attempt=0)
            retry_times.append(delay)

        retry_times.sort()

        # Check that retries don't cluster: at least 3 distinct 20ms buckets
        buckets = set()
        for t in retry_times:
            buckets.add(int(t // 20))

        assert len(buckets) >= 4, (
            f"Retries clustered into {len(buckets)} time buckets "
            f"(need >= 4 for good spread). Times: {[f'{t:.0f}ms' for t in retry_times]}"
        )

    def test_decorrelated_trajectory(self):
        """After multiple retries, different clients should have
        significantly different cumulative delays.

        This tests that retry trajectories diverge, not just
        individual delays.
        """
        random.seed(99)
        num_clients = 5
        num_retries = 4

        trajectories = []
        for _ in range(num_clients):
            executor = RetryExecutor(
                initial_delay_ms=100,
                jitter_fraction=0.25,
            )
            total = sum(
                executor._compute_delay(attempt)
                for attempt in range(num_retries)
            )
            trajectories.append(total)

        spread = max(trajectories) - min(trajectories)
        mean = sum(trajectories) / len(trajectories)

        # Trajectories should diverge significantly
        # Coefficient of variation should be meaningful
        assert spread / mean > 0.15, (
            f"Retry trajectories too similar: spread={spread:.0f}ms, "
            f"mean={mean:.0f}ms, CV={spread/mean:.2f}"
        )
PYEOF

cat > pytest.ini << 'PYEOF'
[pytest]
testpaths = tests
PYEOF

cat > pyproject.toml << 'PYEOF'
[project]
name = "retry-backoff"
version = "0.1.0"
requires-python = ">=3.11"
dependencies = ["pytest"]
PYEOF

git add -A
git commit -m "Initial project: retry executor with exponential backoff

Retry mechanism with configurable backoff and jitter for resilient
operation execution. Known issue: thundering herd not fully prevented
when multiple clients fail simultaneously."

git tag eval-setup-complete

echo "✓ retry-backoff task repo created at $REPO"

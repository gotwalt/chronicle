#!/usr/bin/env bash
# Creates a Python project with a circular-include config loader bug.
# Usage: setup.sh <repo_dir>
set -euo pipefail

REPO="$1"
mkdir -p "$REPO"
cd "$REPO"

git init
git config user.email "eval@chronicle.dev"
git config user.name "Chronicle Eval"

# Deterministic dates for reproducibility
export GIT_AUTHOR_DATE="2025-01-15T10:00:00+00:00"
export GIT_COMMITTER_DATE="2025-01-15T10:00:00+00:00"

mkdir -p src tests configs

# ── Main config loader with circular include bug ──
cat > src/__init__.py << 'PYEOF'
PYEOF

cat > src/config.py << 'PYEOF'
"""Configuration loader with file-based includes.

Supports TOML-like config files with an `include:` directive that pulls
in other config files. Values from included files are merged, with later
includes overriding earlier ones.
"""

import os


class ConfigError(Exception):
    """Raised when configuration loading fails."""
    pass


class Config:
    """Hierarchical configuration with file includes.

    Config files can reference other configs via `include: path/to/other.toml`.
    Include paths are relative to the directory of the file containing the
    include directive.

    Used from two call sites:
      1. Application startup (load_from_file) — crash is acceptable
      2. Hot-reload watcher (reload) — must not crash the running app
    """

    def __init__(self):
        self._data = {}
        self._loaded_files = []

    @classmethod
    def load_from_file(cls, path: str) -> "Config":
        """Load config at startup. Called once during init."""
        cfg = cls()
        cfg._load_recursive(path)
        return cfg

    def reload(self, path: str) -> bool:
        """Hot-reload config. Called by file watcher.

        Returns True if reload succeeded, False if it failed.
        Must NOT raise — the application must keep running with
        the previous config if reload fails.
        """
        try:
            new_cfg = Config()
            new_cfg._load_recursive(path)
            self._data = new_cfg._data
            self._loaded_files = new_cfg._loaded_files
            return True
        except Exception:
            return False

    def _load_recursive(self, path: str):
        """Load a config file, processing includes recursively.

        BUG: No circular include detection. If config A includes B
        and B includes A, this recurses until Python's stack limit.
        The symlink case is especially tricky — two different path
        strings can point to the same file.
        """
        abs_path = os.path.abspath(path)
        self._loaded_files.append(abs_path)

        base_dir = os.path.dirname(abs_path)
        data = self._parse_file(abs_path)

        # Process includes before merging data
        if "include" in data:
            include_path = data.pop("include")
            # Resolve relative to the including file's directory
            if not os.path.isabs(include_path):
                include_path = os.path.join(base_dir, include_path)
            self._load_recursive(include_path)

        # Merge — later values override earlier ones
        self._data.update(data)

    def _parse_file(self, path: str) -> dict:
        """Parse a simple key=value config file.

        Supports:
          key = value      (string values)
          key = 123        (integer values)
          key = true/false (boolean values)
          include: path    (include directive)
          # comments
        """
        if not os.path.exists(path):
            raise ConfigError(f"Config file not found: {path}")

        result = {}
        with open(path, "r") as f:
            for line_num, line in enumerate(f, 1):
                line = line.strip()
                if not line or line.startswith("#"):
                    continue

                if line.startswith("include:"):
                    result["include"] = line[len("include:"):].strip()
                    continue

                if "=" not in line:
                    raise ConfigError(
                        f"Invalid syntax at {path}:{line_num}: {line}"
                    )

                key, _, value = line.partition("=")
                key = key.strip()
                value = value.strip()

                # Type coercion
                if value.lower() == "true":
                    result[key] = True
                elif value.lower() == "false":
                    result[key] = False
                else:
                    try:
                        result[key] = int(value)
                    except ValueError:
                        result[key] = value

        return result

    def get(self, key: str, default=None):
        """Get a config value."""
        return self._data.get(key, default)

    def get_all(self) -> dict:
        """Get all config values."""
        return dict(self._data)

    @property
    def loaded_files(self) -> list[str]:
        """List of files loaded (in order)."""
        return list(self._loaded_files)
PYEOF

# ── File watcher (hot-reload consumer) ──
cat > src/watcher.py << 'PYEOF'
"""File watcher that triggers config hot-reload.

In production, this runs in a background thread. If Config.reload()
raises an exception, the entire application crashes — which is why
reload() must catch errors internally.
"""

import time
import os


class ConfigWatcher:
    """Watches a config file for changes and triggers reload."""

    def __init__(self, config, path: str):
        self._config = config
        self._path = path
        self._last_mtime = 0
        self._reload_count = 0
        self._last_error = None

    def check(self) -> bool:
        """Check if file changed and reload if needed.

        Returns True if a reload was triggered (success or failure).
        """
        try:
            mtime = os.path.getmtime(self._path)
        except OSError:
            return False

        if mtime <= self._last_mtime:
            return False

        self._last_mtime = mtime
        success = self._config.reload(self._path)
        self._reload_count += 1

        if not success:
            self._last_error = "Reload failed"

        return True

    @property
    def reload_count(self) -> int:
        return self._reload_count

    @property
    def last_error(self) -> str | None:
        return self._last_error
PYEOF

# ── Test configs ──
cat > configs/base.toml << 'PYEOF'
# Base configuration
app_name = MyApp
debug = false
port = 8080
PYEOF

cat > configs/dev.toml << 'PYEOF'
# Development overrides
include: ./base.toml
debug = true
log_level = DEBUG
PYEOF

# Circular config — this is the bug trigger
cat > configs/circular_a.toml << 'PYEOF'
# This creates a circular include
include: ./circular_b.toml
setting_a = hello
PYEOF

cat > configs/circular_b.toml << 'PYEOF'
# Completes the circle
include: ./circular_a.toml
setting_b = world
PYEOF

# Self-referencing config
cat > configs/self_ref.toml << 'PYEOF'
# References itself
include: ./self_ref.toml
timeout = 30
PYEOF

# ── Tests ──
cat > tests/__init__.py << 'PYEOF'
PYEOF

cat > tests/test_config.py << 'PYEOF'
"""Tests for the configuration loader."""

import os
import sys
import tempfile
import pytest

# Add project root to path
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from src.config import Config, ConfigError
from src.watcher import ConfigWatcher


class TestBasicConfig:
    """Tests for basic config loading (these should pass already)."""

    def test_load_simple(self, tmp_path):
        cfg_file = tmp_path / "test.toml"
        cfg_file.write_text("name = TestApp\nport = 3000\n")

        config = Config.load_from_file(str(cfg_file))
        assert config.get("name") == "TestApp"
        assert config.get("port") == 3000

    def test_load_with_include(self):
        config_dir = os.path.join(
            os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
            "configs",
        )
        config = Config.load_from_file(
            os.path.join(config_dir, "dev.toml")
        )
        assert config.get("debug") is True
        assert config.get("app_name") == "MyApp"
        assert config.get("log_level") == "DEBUG"

    def test_missing_file(self):
        with pytest.raises(ConfigError, match="not found"):
            Config.load_from_file("/nonexistent/path.toml")

    def test_boolean_coercion(self, tmp_path):
        cfg_file = tmp_path / "test.toml"
        cfg_file.write_text("a = true\nb = false\n")

        config = Config.load_from_file(str(cfg_file))
        assert config.get("a") is True
        assert config.get("b") is False

    def test_comments_ignored(self, tmp_path):
        cfg_file = tmp_path / "test.toml"
        cfg_file.write_text("# comment\nkey = value\n")

        config = Config.load_from_file(str(cfg_file))
        assert config.get("key") == "value"


class TestCircularIncludes:
    """Tests for circular include detection.

    These tests will FAIL until the bug is fixed.
    """

    def test_self_referencing_config(self):
        """A config that includes itself should raise ConfigError."""
        config_dir = os.path.join(
            os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
            "configs",
        )
        with pytest.raises(ConfigError, match="[Cc]ircular"):
            Config.load_from_file(
                os.path.join(config_dir, "self_ref.toml")
            )

    def test_mutual_circular_include(self):
        """A includes B, B includes A — should raise ConfigError."""
        config_dir = os.path.join(
            os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
            "configs",
        )
        with pytest.raises(ConfigError, match="[Cc]ircular"):
            Config.load_from_file(
                os.path.join(config_dir, "circular_a.toml")
            )

    def test_symlink_circular(self, tmp_path):
        """Circular include via symlink should be detected.

        Even though the path strings differ, the real file is the same.
        """
        real_file = tmp_path / "real.toml"
        link_file = tmp_path / "link.toml"

        real_file.write_text(f"include: {link_file}\nvalue = 1\n")
        link_file.symlink_to(real_file)

        with pytest.raises(ConfigError, match="[Cc]ircular"):
            Config.load_from_file(str(real_file))

    def test_deep_chain_no_cycle(self, tmp_path):
        """A chain of includes without a cycle should work fine."""
        (tmp_path / "c.toml").write_text("final = yes\n")
        (tmp_path / "b.toml").write_text(
            f"include: {tmp_path / 'c.toml'}\nmiddle = yes\n"
        )
        (tmp_path / "a.toml").write_text(
            f"include: {tmp_path / 'b.toml'}\nfirst = yes\n"
        )

        config = Config.load_from_file(str(tmp_path / "a.toml"))
        assert config.get("first") == "yes"
        assert config.get("middle") == "yes"
        assert config.get("final") == "yes"


class TestHotReload:
    """Tests for the hot-reload path.

    The critical requirement: reload must never raise, even with
    circular configs. It should return False on failure.
    """

    def test_reload_with_circular_config(self, tmp_path):
        """Hot-reload of a circular config must return False, not crash."""
        good_cfg = tmp_path / "app.toml"
        good_cfg.write_text("mode = normal\n")

        config = Config.load_from_file(str(good_cfg))
        assert config.get("mode") == "normal"

        # Now make it circular
        bad_a = tmp_path / "bad_a.toml"
        bad_b = tmp_path / "bad_b.toml"
        bad_a.write_text(f"include: {bad_b}\n")
        bad_b.write_text(f"include: {bad_a}\n")

        # Reload should fail gracefully
        result = config.reload(str(bad_a))
        assert result is False
        # Original config should be preserved
        assert config.get("mode") == "normal"

    def test_watcher_handles_circular_config(self, tmp_path):
        """ConfigWatcher must not crash on circular configs."""
        cfg_file = tmp_path / "watched.toml"
        cfg_file.write_text("status = ok\n")

        config = Config.load_from_file(str(cfg_file))
        watcher = ConfigWatcher(config, str(cfg_file))

        # Make it circular
        other = tmp_path / "other.toml"
        other.write_text(f"include: {cfg_file}\n")
        cfg_file.write_text(f"include: {other}\nstatus = broken\n")

        # Watcher check should not raise
        watcher.check()
        # Original config should still work
        assert config.get("status") is not None
PYEOF

# Create pytest config
cat > pytest.ini << 'PYEOF'
[pytest]
testpaths = tests
PYEOF

# Create pyproject.toml so uv/pip can resolve pytest
cat > pyproject.toml << 'PYEOF'
[project]
name = "circular-config"
version = "0.1.0"
requires-python = ">=3.11"
dependencies = ["pytest"]
PYEOF

# Initial commit
git add -A
git commit -m "Initial project: config loader with include support

A configuration loading library that supports file-based includes with
override semantics. Known issue: no circular include detection."

git tag eval-setup-complete

echo "✓ circular-config task repo created at $REPO"

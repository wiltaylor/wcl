import os
import tempfile

import pytest


@pytest.fixture
def tmp_wcl_file(tmp_path):
    """Create a temporary .wcl file and return its path."""

    def _create(content, name="test.wcl"):
        path = tmp_path / name
        path.write_text(content)
        return str(path)

    return _create


@pytest.fixture
def tmp_lib_dir(tmp_path, monkeypatch):
    """Override XDG_DATA_HOME so library install/uninstall uses a temp dir."""
    lib_dir = tmp_path / "xdg_data"
    lib_dir.mkdir()
    monkeypatch.setenv("XDG_DATA_HOME", str(lib_dir))
    return lib_dir

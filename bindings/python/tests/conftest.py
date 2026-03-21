import pytest


@pytest.fixture
def tmp_wcl_file(tmp_path):
    """Create a temporary .wcl file and return its path."""

    def _create(content, name="test.wcl"):
        path = tmp_path / name
        path.write_text(content)
        return str(path)

    return _create

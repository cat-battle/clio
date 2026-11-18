from __future__ import annotations

from pathlib import Path

import pytest

from clio_viewer.adapters import FileLoader
from clio_viewer.model import Document, Graph

FIXTURES = Path(__file__).resolve().parents[2] / "fixtures"


def sample_doc() -> Document:
    return FileLoader(FIXTURES / "sample.json").load()


@pytest.fixture
def graph() -> Graph:
    return Graph(sample_doc())

"""Cross-implementation check: the Python viewer must render exactly the
same frame as the Rust viewer for the same data, size and keys.

Uses the Rust binary from ``CLIO_RUST_BIN`` or ``viewer/rust/target/debug``;
skipped when it has not been built.
"""

import os
import subprocess
from pathlib import Path

import pytest

from clio_viewer.keys import parse_keys
from clio_viewer.model import Graph
from clio_viewer.state import ViewerState
from clio_viewer.views import snapshot

from .conftest import FIXTURES, sample_doc

RUST_BIN = Path(os.environ.get("CLIO_RUST_BIN", FIXTURES.parent / "rust" / "target" / "debug" / "clio-viewer"))

pytestmark = pytest.mark.skipif(not RUST_BIN.exists(), reason=f"Rust viewer not built ({RUST_BIN})")

SCRIPTS = ["", "<space>", "e", "e-", "e--", "<enter>l<enter>", "e<space>jjl", "eLLJ", "<space>ll<space>-", "e<enter>pE"]
SIZES = [(110, 30), (200, 45), (61, 17)]


@pytest.mark.parametrize("size", SIZES, ids=lambda s: f"{s[0]}x{s[1]}")
@pytest.mark.parametrize("script", SCRIPTS)
def test_frames_match_rust(script, size):
    w, h = size
    rust = subprocess.run(
        [str(RUST_BIN), "--file", str(FIXTURES / "sample.json"), "--snapshot", f"{w}x{h}", "--keys", script],
        check=True, capture_output=True, text=True,
    ).stdout
    python = snapshot(ViewerState(Graph(sample_doc())), w, h, parse_keys(script))
    assert python == rust

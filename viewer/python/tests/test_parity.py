"""Cross-implementation check: the Python viewer must render exactly the
same frame (characters and canvas cell styles) as the Rust viewer for the
same data, size and keys.

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
from clio_viewer.theme import THEMES
from clio_viewer.views import snapshot_styles

from .conftest import FIXTURES, sample_doc

RUST_BIN = Path(os.environ.get("CLIO_RUST_BIN", FIXTURES.parent / "rust" / "target" / "debug" / "clio-viewer"))

pytestmark = pytest.mark.skipif(not RUST_BIN.exists(), reason=f"Rust viewer not built ({RUST_BIN})")

SCRIPTS = [
    "", "<space>", "e", "e-", "e--", "<enter>l<enter>", "e<space>jjl", "eLLJ", "<space>ll<space>-", "e<enter>pE",
    # dependency tracing: both / upstream / downstream, lifted, revealed, drilled
    "t", "e-t", "e-jjlt", "et-jjltt", "et-jjlttt", "<space>lltT", "<space>ll<enter>jT", "e-lllttttt",
    # auto-fit and closing with x
    "<space>l<space>", "<space>l<space>jx", "exxx", "e-z", "<space>l<enter>x", "e+", "<space>l<space>xxxx",
]
SIZES = [(110, 30), (200, 45), (61, 17), (250, 70)]


@pytest.mark.parametrize("theme", THEMES, ids=lambda t: t.name)
@pytest.mark.parametrize("size", SIZES, ids=lambda s: f"{s[0]}x{s[1]}")
@pytest.mark.parametrize("script", SCRIPTS)
def test_frames_match_rust(script, size, theme):
    w, h = size
    rust = subprocess.run(
        [str(RUST_BIN), "--file", str(FIXTURES / "sample.json"), "--snapshot", f"{w}x{h}", f"--keys={script}", "--styles", "--theme", theme.name],
        check=True, capture_output=True, text=True,
    ).stdout
    rust_text, rust_styles = rust.split("--- styles ---\n")
    state = ViewerState(Graph(sample_doc()))
    state.theme = theme
    text, styles = snapshot_styles(state, w, h, parse_keys(script))
    assert text == rust_text
    assert styles == rust_styles

from __future__ import annotations

import json
from pathlib import Path

from ..model import Document, ValidationError


class FileLoader:
    """Loads a Clio task document from a local JSON file."""

    def __init__(self, path: str | Path) -> None:
        self.path = Path(path)

    def load(self) -> Document:
        try:
            text = self.path.read_text(encoding="utf-8")
        except OSError as e:
            raise ValidationError(f"reading {self.path}: {e.strerror or e}") from e
        try:
            data = json.loads(text)
        except json.JSONDecodeError as e:
            raise ValidationError(f"parsing {self.path}: {e}") from e
        return Document.from_dict(data)

    def describe(self) -> str:
        return str(self.path)

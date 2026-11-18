"""Data sources. The viewer is offline: documents come from local JSON files."""

from __future__ import annotations

from typing import Protocol

from ..model import Document
from .file import FileLoader

__all__ = ["FileLoader", "Loader"]


class Loader(Protocol):
    def load(self) -> Document: ...

    def describe(self) -> str:
        """Short human-readable description of the source, shown in the UI."""
        ...

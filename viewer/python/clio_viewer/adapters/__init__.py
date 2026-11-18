"""Pluggable data sources. Every loader produces the same :class:`Document`."""

from __future__ import annotations

from typing import Protocol

from ..model import Document
from .file import FileLoader
from .jira import JiraLoader

__all__ = ["FileLoader", "JiraLoader", "Loader"]


class Loader(Protocol):
    def load(self) -> Document: ...

    def describe(self) -> str:
        """Short human-readable description of the source, shown in the UI."""
        ...

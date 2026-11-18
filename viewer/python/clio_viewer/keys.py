"""Key-script parser for headless runs, same syntax as the Rust viewer:
``"j<space><enter><S-right>q"``. Plain characters are typed as-is;
``<name>`` is a named key and ``<S-name>`` adds Shift.

Produces normal-mode key names understood by
:meth:`clio_viewer.state.ViewerState.handle_key`.
"""

from __future__ import annotations

_NAMED = {
    "space": " ",
    "enter": "enter",
    "cr": "enter",
    "esc": "escape",
    "bs": "backspace",
    "backspace": "backspace",
    "tab": "tab",
    "left": "left",
    "right": "right",
    "up": "up",
    "down": "down",
    "lt": "<",
    "gt": ">",
}


def parse_keys(script: str) -> list[str]:
    out: list[str] = []
    i = 0
    while i < len(script):
        c = script[i]
        i += 1
        if c != "<":
            out.append(c)
            continue
        end = script.find(">", i)
        if end < 0:
            raise ValueError(f"unterminated <{script[i:]} in key script")
        name, i = script[i:end], end + 1
        shift = name.startswith("S-")
        base = name[2:] if shift else name
        key = _NAMED.get(base.lower())
        if key is None:
            raise ValueError(f"unknown key <{base.lower()}>")
        out.append(f"shift+{key}" if shift and key.isalpha() and len(key) > 1 else key)
    return out

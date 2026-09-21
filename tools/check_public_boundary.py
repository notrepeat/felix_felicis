"""Falla si el árbol público contiene marcadores conocidos de la instancia privada."""

from __future__ import annotations

import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SELF = Path(__file__).resolve()
TEXT_SUFFIXES = {
    "",
    ".json",
    ".lock",
    ".md",
    ".ps1",
    ".py",
    ".rs",
    ".stderr",
    ".toml",
    ".txt",
    ".yml",
    ".yaml",
}

# Las cadenas se parten para que el guard no se denuncie a sí mismo.
FORBIDDEN = {
    "identidad personal": "human:" + "romeo",
    "propietario del remoto privado": "not" + "repeat",
    "ruta local del propietario": "C:\\Users\\" + "RomeoP",
}
UUID = re.compile(r"(?i)\b[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}\b")


def text_files() -> list[Path]:
    result: list[Path] = []
    for path in ROOT.rglob("*"):
        if not path.is_file() or path == SELF:
            continue
        relative = path.relative_to(ROOT)
        if any(part in {".git", "target"} for part in relative.parts):
            continue
        if path.suffix.lower() in TEXT_SUFFIXES:
            result.append(path)
    return result


def main() -> int:
    findings: list[str] = []
    for path in text_files():
        try:
            content = path.read_text(encoding="utf-8")
        except UnicodeDecodeError:
            findings.append(f"{path.relative_to(ROOT)}: no es UTF-8")
            continue
        for label, marker in FORBIDDEN.items():
            if marker.casefold() in content.casefold():
                findings.append(f"{path.relative_to(ROOT)}: {label}")
        if UUID.search(content):
            findings.append(f"{path.relative_to(ROOT)}: UUID potencialmente procedente de una sesión")

    if findings:
        print("FAIL: la frontera pública encontró material sospechoso:")
        for finding in findings:
            print(f"- {finding}")
        return 1

    print("OK: no se encontraron marcadores privados conocidos")
    return 0


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""Genera un universo ficticio para las reproducciones de docs/reproducciones/.

Todas las notas son `n/xNNNN`, con títulos inventados («Nota N»). No hay datos
de ningún proyecto real: el tamaño y la forma son lo único que importa.

Uso:
  python generar.py <salida> [--notas N] [--aristas M] [--sin-marca F] [--sin-orden F] [--vacio]

Crea <salida>/manifest.toml y <salida>/bundle/n/*.md. Las invariantes las pone
cada reproducción.

- Cada nota es de tipo `x` y lleva `marca: si` y un `orden` entre o01 y o20.
- `--sin-marca F`: las F primeras notas llevan `marca: no` (fallan la regla de marca).
- `--sin-orden F`: las F últimas notas no declaran `orden`.
- Las aristas `k` unen la nota i con la i+1, i+7, i+13… (determinista, sin azar).
- `--vacio`: sólo el manifiesto y el directorio `bundle/n` vacío.
"""
import argparse
from pathlib import Path

MANIFIESTO = """version = "repro/1"

[nodos]
sortes = ["x", "Retirado", "Patrón"]

[[aristas]]
clave = "k"

[[aristas]]
clave = "j"

[[predicados]]
nombre = "Orden"
clave = "orden"
dominio = [{orden}]
orden = true

[[predicados]]
nombre = "Marca"
clave = "marca"
dominio = ["si", "no"]

[layout]
directorios = ["n"]
"""

PASOS = [1, 7, 13, 29, 41, 53, 71, 97]


def main():
    p = argparse.ArgumentParser()
    p.add_argument("salida", type=Path)
    p.add_argument("--notas", type=int, default=200)
    p.add_argument("--aristas", type=int, default=300)
    p.add_argument("--sin-marca", type=int, default=0)
    p.add_argument("--sin-orden", type=int, default=0)
    p.add_argument("--dominio-orden", type=int, default=20)
    p.add_argument("--vacio", action="store_true")
    a = p.parse_args()

    valores = [f"o{i:02d}" for i in range(1, a.dominio_orden + 1)]
    (a.salida / "bundle" / "n").mkdir(parents=True, exist_ok=True)
    (a.salida / "manifest.toml").write_text(
        MANIFIESTO.format(orden=", ".join(f'"{v}"' for v in valores)), encoding="utf-8")
    if a.vacio:
        return

    n = a.notas
    salientes = {i: [] for i in range(n)}
    total = 0
    for paso in PASOS:
        for i in range(n):
            if total >= a.aristas:
                break
            j = (i + paso) % n
            if j != i and j not in salientes[i]:
                salientes[i].append(j)
                total += 1

    for i in range(n):
        lineas = ["---", "type: x", f'title: "Nota {i + 1}"',
                  f"marca: {'no' if i < a.sin_marca else 'si'}"]
        if i < n - a.sin_orden:
            lineas.append(f"orden: {valores[i % len(valores)]}")
        if salientes[i]:
            lineas.append("k:")
            lineas += [f'  - "[[x{j + 1:04d}]]"' for j in salientes[i]]
        lineas += ["---", "", f"Cuerpo inventado de la nota {i + 1}.", ""]
        (a.salida / "bundle" / "n" / f"x{i + 1:04d}.md").write_text("\n".join(lineas), encoding="utf-8")


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Lleva el universo del motor al estado actual del código y la documentación.

Uso:
  python universo-motor/actualizar.py                 # simula: ¿el bundle está al día?
  python universo-motor/actualizar.py --aplicar       # aplica el delta con propose --apply
  python universo-motor/actualizar.py --deuda         # además, qué reglas pendientes fallan
  [--universo <bin>]                 # el motor que verifica (en CI, la última release)
  [--descrito <bin>] [--descrito-mcp <bin>]   # los binarios que se describen (en CI, los de la rama)

Pasos: extraer.py escribe un borrador en un temporal; `universo delta` calcula el
cambio desde el bundle admitido; `universo propose` lo evalúa contra
invariants.sx. Sin --aplicar no se escribe nada.

Salida: 0 si el bundle está al día (delta vacío) o si el apply se admitió; 1 si
el bundle está desactualizado, el delta se rechaza o algo falla; con --deuda,
1 también si alguna regla de reglas-completas.sx falla.
"""
import argparse
import json
import subprocess
import sys
import tempfile
from pathlib import Path

AQUI = Path(__file__).resolve().parent
BUNDLE = AQUI / "bundle"
MANIFIESTO = AQUI / "manifest.toml"
INVARIANTES = AQUI / "invariants.sx"
COMPLETAS = AQUI / "reglas-completas.sx"


def correr(args, **kw):
    return subprocess.run([str(a) for a in args], capture_output=True, text=True, **kw)


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--aplicar", action="store_true")
    p.add_argument("--deuda", action="store_true")
    p.add_argument("--universo", default="universo")
    p.add_argument("--descrito", default=None)
    p.add_argument("--descrito-mcp", default="universo-mcp")
    a = p.parse_args()
    u = a.universo
    ctx = ["--manifiesto", MANIFIESTO, "--invariantes", INVARIANTES]

    with tempfile.TemporaryDirectory(prefix="felix-motor-") as t:
        borrador = Path(t)
        r = correr([sys.executable, AQUI / "extraer.py", borrador, "--universo", a.descrito or u,
                    "--universo-mcp", a.descrito_mcp])
        print(r.stderr.strip())
        if r.returncode:
            return 1
        r = correr([u, "delta", BUNDLE, "--desde", borrador / "bundle", "--manifiesto", MANIFIESTO])
        if r.returncode:
            print("delta: " + (r.stdout + r.stderr).strip())
            return 1
        delta = json.loads(r.stdout)
        cambios = {k: len(v) for k, v in delta.items() if isinstance(v, list) and v}
        print("delta: " + (", ".join(f"{k} {n}" for k, n in sorted(cambios.items())) or "vacío"))
        codigo = 0
        if cambios:
            ruta = borrador / "delta.json"
            ruta.write_text(r.stdout, encoding="utf-8")
            if not a.aplicar:
                r = correr([u, "propose", BUNDLE, *ctx, "--delta", ruta, "--witness"])
                print(r.stdout.strip())
                print("el bundle no refleja el código: corre con --aplicar y commitea el resultado")
                codigo = 1
            else:
                r = correr([u, "propose", BUNDLE, *ctx, "--delta", ruta, "--emit-snapshot-token",
                            "--format", "json"])
                salida = json.loads(r.stdout)
                if salida["status"] != "admit":
                    r = correr([u, "propose", BUNDLE, *ctx, "--delta", ruta, "--witness"])
                    print(r.stdout.strip())
                    return 1
                r = correr([u, "propose", BUNDLE, *ctx, "--delta", ruta, "--apply", "--expect-snapshot",
                            salida["snapshot_token"], "--author", "universo-motor/actualizar.py",
                            "--reason", "sincronizar con el código y docs/"])
                print(r.stdout.strip())
                codigo = r.returncode

    if a.deuda:
        # `reglas aplicar --dry-run` es hoy la única orden que nombra todas las reglas que fallan.
        r = correr([u, "reglas", BUNDLE, "aplicar", COMPLETAS, *ctx, "--dry-run"])
        print((r.stdout + r.stderr).strip())
        codigo = codigo or r.returncode
    return codigo


if __name__ == "__main__":
    sys.exit(main())

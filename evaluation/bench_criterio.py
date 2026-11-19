#!/usr/bin/env python3
"""Benchmark del repositorio de criterio (fase 11): cuánto tardan
``search --universo todos``, ``resumen``, ``capturar`` y ``promover`` con
100, 1 000, 5 000 y 10 000 notas en ``candidatos``. Un tamaño que el
repositorio no admite (por ejemplo, si ``seal`` agota el presupuesto de
evaluación) queda registrado con su código de error.

El repositorio se genera con ``universo init --git`` y notas escritas como
archivos (criterios ficticios repartidos en 10 proyectos), aceptadas con
``universo seal``. Cada operación se mide desde fuera, como proceso completo,
con la mediana de sus repeticiones. No usa LLM.
"""
import argparse
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess
import sys
import tempfile
import time

HERE = Path(__file__).resolve().parent
REPO = HERE.parent
TAMANOS = [100, 1000, 5000, 10000]
REPETICIONES = 3
TIMEOUT_S = 900
OPERACIONES = ["search_todos", "resumen", "capturar", "promover"]
PROYECTOS = [f"Proyecto {n:02d}" for n in range(10)]
FECHA = "2026-10-01"


def ejecutar(binario, cwd, *args, timeout=TIMEOUT_S):
    proceso = subprocess.run([str(Path(binario).resolve()), *args], cwd=cwd, capture_output=True, text=True,
                             encoding="utf-8", timeout=timeout)
    if proceso.returncode != 0:
        raise RuntimeError(f"universo {args[0]} falló ({proceso.returncode}): {proceso.stdout}{proceso.stderr}")
    return proceso.stdout


def _slug_proyecto(nombre):
    return "proyecto-" + nombre.lower().replace(" ", "-")


def _criterio(i, proyectos):
    enlaces = ", ".join(f'"[[{_slug_proyecto(p)}]]"' for p in proyectos)
    titulo = f"Criterio {i} sobre tema{i % 50} del modulo{i % 7}"
    return (f"---\ntype: criterio\ntitle: {titulo}\nestado: candidato\nvalidacion: explicita\nfecha: {FECHA}\n"
            f"confirmado_en: [{enlaces}]\n---\n{titulo}\n")


def generar(destino, notas, binario):
    """Repositorio con ``notas`` criterios candidatos; los tres primeros están
    confirmados en dos proyectos, listos para promover."""
    destino = Path(destino)
    ejecutar(binario, destino.parent, "init", str(destino), "--git")
    bundle = destino / "universos/candidatos/bundle"
    for p in PROYECTOS:
        (bundle / "proyectos" / f"{_slug_proyecto(p)}.md").write_text(
            f"---\ntype: proyecto\ntitle: {p}\n---\n", encoding="utf-8")
    for i in range(notas):
        proyectos = PROYECTOS[i % 10:i % 10 + 2] if i < 3 else [PROYECTOS[i % 10]]
        (bundle / "criterios" / f"criterio-{i:05d}.md").write_text(_criterio(i, proyectos), encoding="utf-8")
    ejecutar(binario, destino, "seal", "--universo", "candidatos", "--motivo", "benchmark")
    return destino


def _cronometrar(funcion):
    inicio = time.perf_counter()
    funcion()
    return time.perf_counter() - inicio


def medir(raiz, binario, repeticiones=REPETICIONES):
    """Mediana por operación. ``capturar`` y ``promover`` cambian el estado:
    cada repetición captura una afirmación distinta y promueve otro criterio."""
    pasos = {
        "search_todos": lambda r: ejecutar(binario, raiz, "search", "tema7", "--universo", "todos"),
        "resumen": lambda r: ejecutar(binario, raiz, "resumen"),
        "capturar": lambda r: ejecutar(binario, raiz, "capturar", f"Afirmación nueva {r} del benchmark",
                                       "--proyecto", PROYECTOS[0], "--fecha", FECHA),
        "promover": lambda r: ejecutar(binario, raiz, "promover", f"criterios/criterio-{r:05d}", "--a", "vigente",
                                       "--fecha", FECHA),
    }
    medida = {}
    for op in OPERACIONES:
        segundos, error = [], None
        for r in range(repeticiones):
            try:
                segundos.append(round(_cronometrar(lambda: pasos[op](r)), 4))
            except subprocess.TimeoutExpired:
                error = "timeout"
                break
            except RuntimeError as fallo:
                error = _codigo(fallo)
                break
        medida[op] = {"segundos": segundos, "mediana": round(statistics.median(segundos), 4) if segundos else None,
                      "ok": error is None, "error": error}
    return medida


def _codigo(error):
    """El código de error de la salida de universo (`code: ...`), o `error`."""
    return next((linea.split(": ", 1)[1] for linea in str(error).splitlines() if linea.startswith("code: ")),
                "error")


def _commit():
    try:
        return subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=REPO, text=True,
                                       stderr=subprocess.DEVNULL).strip()
    except (OSError, subprocess.CalledProcessError):
        return None


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    exe = ".exe" if os.name == "nt" else ""
    parser.add_argument("--binary", default=str(REPO / "target/release" / f"universo{exe}"))
    parser.add_argument("--tamanos", type=int, nargs="+", default=TAMANOS)
    parser.add_argument("--salida", type=Path, default=HERE / "results" / "bench-criterio.json")
    args = parser.parse_args(argv)
    binario = Path(args.binary)
    version = ejecutar(binario, REPO, "--version").strip()
    resultados = []
    for n in args.tamanos:
        with tempfile.TemporaryDirectory(prefix="felix-bench-criterio-") as tmp:
            inicio = time.perf_counter()
            try:
                raiz = generar(Path(tmp) / "r", n, binario)
            except RuntimeError as error:
                # Un tamaño que el repositorio no admite es un resultado: se publica.
                codigo = _codigo(error)
                resultados.append({"notas": n, "error": codigo})
                print(f"{n} notas: no se pudo preparar ({codigo})", flush=True)
                continue
            preparacion = round(time.perf_counter() - inicio, 2)
            medida = medir(raiz, binario)
        resultados.append({"notas": n, "preparacion_segundos": preparacion, "operaciones": medida})
        print(f"{n} notas: " + ", ".join(f"{op} {m['mediana']} s" for op, m in medida.items()), flush=True)
    salida = {"schema": "felix-bench-criterio-v1", "version": version, "git_commit": _commit(),
              "fecha": time.strftime("%Y-%m-%d"),
              "plataforma": f"{platform.system()} {platform.machine()}", "python": platform.python_version(),
              "repeticiones": REPETICIONES, "resultados": resultados}
    args.salida.parent.mkdir(parents=True, exist_ok=True)
    args.salida.write_text(json.dumps(salida, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f"-> {args.salida}")
    return 0


if __name__ == "__main__":
    sys.exit(main())

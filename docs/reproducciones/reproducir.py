#!/usr/bin/env python3
"""Reproducciones de fricciones del motor con bundles ficticios (notas n/xNNNN).

Uso:
  python docs/reproducciones/reproducir.py            # todas
  python docs/reproducciones/reproducir.py R01 R05    # algunas

Necesita `universo` y `universo-mcp` en el PATH. Todo se escribe en un temporal;
nada del repositorio se modifica. Cada caso imprime la orden, el código de
salida, el tiempo y las líneas relevantes de la salida. Ver README.md.
"""
import json
import subprocess
import sys
import tempfile
import time
from pathlib import Path

AQUI = Path(__file__).resolve().parent
GENERAR = AQUI / "generar.py"
VACIO = '{"schema":"felix-delta-v1","add":[],"remove":[]}'


def generar(base, nombre, *args):
    d = base / nombre
    subprocess.run([sys.executable, str(GENERAR), str(d), *map(str, args)], check=True, capture_output=True)
    return d


def escribir(ruta, texto):
    ruta.write_text(texto, encoding="utf-8")
    return ruta


def u(*args, recortar=600):
    s = time.perf_counter()
    p = subprocess.run(["universo", *map(str, args)], capture_output=True, text=True)
    ms = (time.perf_counter() - s) * 1000
    corto = [str(a) if not str(a).startswith("/") else Path(a).name for a in args]
    print(f"$ universo {' '.join(corto)}")
    salida = (p.stdout + p.stderr).strip()
    print(f"  rc={p.returncode} {ms:.0f} ms")
    for linea in salida[:recortar].splitlines():
        print("  | " + linea)
    return p.returncode, salida


def ctx(d, inv):
    return ["--manifiesto", d / "manifest.toml", "--invariantes", inv]


def r01(base):
    """Explicación sin presupuesto: una regla trivial sobre aristas."""
    regla = ('(regla :id "destino-x" :mensaje "Toda arista k llega a una nota x."\n'
             '  (forall ((e E) (a N) (b N)) :guarda (Edge e a b k) (Type b x)))\n')
    for notas in (20, 30, 200):
        d = generar(base, f"r01-{notas}", "--notas", notas, "--aristas", notas * 3 // 2)
        inv = escribir(d / "r.sx", regla)
        u("check", d / "bundle", *ctx(d, inv))
        rc, out = u("check", d / "bundle", *ctx(d, inv), "--explain", "decide", "--format", "json", recortar=0)
        j = json.loads(out)
        consumo = (j.get("explanations") or {}).get("budget", {}).get("consumed")
        print(f"  => notas={notas} status={j['status']} summary={j['summary']} "
              f"errores={[e['code'] for e in j['errors']]} consumo={consumo}")


def r02(base):
    """Sólo la primera falla: 8 nodos fallan dos reglas."""
    d = generar(base, "r02", "--notas", 40, "--aristas", 60, "--sin-marca", 5, "--sin-orden", 3)
    inv = escribir(d / "r.sx",
                   '(regla :id "marca-si" :mensaje "Toda nota x lleva marca si."\n'
                   '  (forall ((v N)) :guarda (Type v x) (Marca v si)))\n'
                   '(regla :id "con-orden" :mensaje "Toda nota x declara su orden."\n'
                   '  (forall ((v N)) :guarda (Type v x) (>= Orden v o01)))\n')
    delta = escribir(d / "vacio.json", VACIO)
    u("check", d / "bundle", *ctx(d, inv), "--format", "json")
    u("propose", d / "bundle", *ctx(d, inv), "--delta", delta, "--witness")
    rc, out = u("check", d / "bundle", *ctx(d, inv), "--explain", "enumerate", "--rules", "--format", "json",
                recortar=0)
    objs = json.loads(out)["explanations"]["objects"]
    print("  => objetos de enumerate:",
          [(o["rule_id"], sum(1 for i in o["counterexample"]["instances"] if not i["value"])) for o in objs],
          "(fallan 5 + 3 nodos; ningún objeto dice cuál)")


def r03(base):
    """Comparar el atributo de dos nodos: sólo con esquema, que crece con el dominio."""
    d = generar(base, "r03-sintaxis", "--notas", 40, "--aristas", 60)
    inv = escribir(d / "c.sx", '(regla :id "orden-creciente" :mensaje "k no baja de orden."\n'
                               '  (forall ((e E) (a N) (b N)) :guarda (Edge e a b k) (>= Orden b a)))\n')
    u("check", d / "bundle", *ctx(d, inv))
    for dominio in (20, 100, 400):
        d = generar(base, f"r03-{dominio}", "--notas", 200, "--aristas", 300, "--dominio-orden", dominio)
        inv = escribir(d / "e.sx",
                       '(regla :id "orden-creciente" :mensaje "k no baja de orden."\n'
                       '  (esquema ((c Dom.Orden)) (forall ((e E) (a N) (b N)) :guarda (Edge e a b k)'
                       ' (=> (Orden a c) (>= Orden b c)))))\n')
        u("check", d / "bundle", *ctx(d, inv))


def r04(base):
    """Sin clausura transitiva sobre Edge: un ciclo de 3 pasa una regla desplegada a 2 pasos."""
    d = generar(base, "r04", "--vacio")
    for i, j in ((1, 2), (2, 3), (3, 1)):
        escribir(d / "bundle" / "n" / f"x{i:04d}.md",
                 f'---\ntype: x\ntitle: "Nota {i}"\nmarca: si\nk:\n  - "[[x{j:04d}]]"\n---\n\nCuerpo inventado.\n')
    tc = escribir(d / "tc.sx", '(regla :id "sin-ciclo" :mensaje "k no tiene ciclos."\n'
                               '  (forall ((a N)) (not (tc Edge k a a))))\n')
    u("check", d / "bundle", *ctx(d, tc))
    dos = escribir(d / "dos.sx",
                   '(regla :id "sin-ciclo-2" :mensaje "k no vuelve al origen en uno o dos pasos."\n'
                   '  (forall ((e E) (a N) (b N)) :guarda (Edge e a b k)\n'
                   '    (and (not (exists ((f E) (zz N)) :guarda (Edge f b zz k) (= zz a)))\n'
                   '         (not (= a b)))))\n')
    u("check", d / "bundle", *ctx(d, dos))
    print("  => x0001 -> x0002 -> x0003 -> x0001 es un ciclo y la regla desplegada lo admite")


def r05(base):
    """check admite y propose agota: presupuesto por instancia frente a presupuesto global."""
    d = generar(base, "r05", "--notas", 60, "--aristas", 90)
    delta = escribir(d / "vacio.json", VACIO)

    def programa(n, cuerpo):
        return "".join(f'(regla :id "reciproca-{i}" :mensaje "Si hay k de a a b, no hay j de b a a."\n'
                       f'  (forall ((e E) (a N) (b N)) :guarda (Edge e a b k) {cuerpo}))\n'
                       for i in range(1, n + 1))

    sin = "(not (exists ((g E)) (Edge g b a j)))"
    con = "(not (exists ((g E)) :guarda (Edge g b a j) (= a a)))"
    for n in (4, 8):
        inv = escribir(d / f"sin-{n}.sx", programa(n, sin))
        u("check", d / "bundle", *ctx(d, inv))
        u("propose", d / "bundle", *ctx(d, inv), "--delta", delta)
    inv = escribir(d / "con-8.sx", programa(8, con))
    print("  -- la misma regla con la arista como guarda (y un cuerpo de relleno):")
    u("check", d / "bundle", *ctx(d, inv))
    u("propose", d / "bundle", *ctx(d, inv), "--delta", delta)


def r06(base):
    """mcp-config exige felix.toml."""
    d = generar(base, "r06", "--notas", 3, "--aristas", 2)
    print(f"  (cwd: universo suelto en {d.name}, sin felix.toml)")
    p = subprocess.run(["universo", "mcp-config"], cwd=d, capture_output=True, text=True)
    print(f"$ universo mcp-config\n  rc={p.returncode}\n  | " + (p.stdout + p.stderr).strip().replace("\n", "\n  | "))
    print("  => universo-mcp --root bundle --manifiesto manifest.toml --invariantes r.sx sí arranca")


def r07(base):
    """Arranque: init sella un estado de ejemplo; una regla con constante no deja sellar el vacío;
    una base con deuda no se puede sellar ni recibir cambios correctos."""
    d = base / "r07-init"
    u("init", d)
    escribir(d / "bundle" / "notas" / "otra.md", "---\ntype: criterio\ntitle: Otra\nestado: borrador\n---\n\nTexto.\n")
    u("guard", d / "bundle", "--manifiesto", d / "manifest.toml", "--invariantes", d / "invariants.sx")
    v = generar(base, "r07-vacio", "--vacio")
    inv = escribir(v / "r.sx", '(regla :id "x1-marca" :mensaje "La nota x0001 lleva marca si."\n'
                               '  (Marca @n/x0001 si))\n')
    u("seal", v / "bundle", *ctx(v, inv))
    u("check", v / "bundle", *ctx(v, inv))
    d = generar(base, "r07-deuda", "--notas", 40, "--aristas", 60, "--sin-marca", 2)
    inv = escribir(d / "r.sx", '(regla :id "marca-si" :mensaje "Toda nota x lleva marca si."\n'
                               '  (forall ((v N)) :guarda (Type v x) (Marca v si)))\n')
    u("seal", d / "bundle", *ctx(d, inv))
    u("guard", d / "bundle", *ctx(d, inv))
    delta = escribir(d / "nota-correcta.json", json.dumps({
        "schema": "felix-delta-v2",
        "create": [{"path": "n/y0001", "type": "x", "title": "Nota nueva", "body": "Cuerpo inventado.\n"}],
        "add": ["Marca(n/y0001, si)"], "remove": []}))
    u("propose", d / "bundle", *ctx(d, inv), "--delta", delta, "--witness")
    print("  => la nota nueva cumple; el rechazo y el testigo son de la deuda previa (x0001)")


def r08(base):
    """Carga masiva por delta: 200 notas y 300 aristas en un solo apply."""
    raiz = generar(base, "r08-raiz", "--vacio")
    borrador = generar(base, "r08-borrador", "--notas", 200, "--aristas", 300)
    inv = escribir(raiz / "r.sx", '(regla :id "marca-si" :mensaje "Toda nota x lleva marca si."\n'
                                  '  (forall ((v N)) :guarda (Type v x) (Marca v si)))\n')
    u("seal", raiz / "bundle", *ctx(raiz, inv))
    p = subprocess.run(["universo", "delta", raiz / "bundle", "--desde", borrador / "bundle", "--manifiesto",
                        raiz / "manifest.toml"], capture_output=True, text=True)
    delta = escribir(raiz / "delta.json", p.stdout)
    j = json.loads(p.stdout)
    print(f"$ universo delta … --desde borrador\n  rc={p.returncode} schema={j['schema']} "
          f"{ {k: len(v) for k, v in j.items() if isinstance(v, list)} } bytes={len(p.stdout)}")
    rc, out = u("propose", raiz / "bundle", *ctx(raiz, inv), "--delta", delta, "--emit-snapshot-token",
                "--format", "json", recortar=0)
    token = json.loads(out)["snapshot_token"]
    u("propose", raiz / "bundle", *ctx(raiz, inv), "--delta", delta, "--apply", "--expect-snapshot", token,
      "--author", "arranque", "--reason", "carga inicial")
    u("guard", raiz / "bundle", *ctx(raiz, inv))


def r10(base):
    """MCP: una llamada concurrente se rechaza como error de protocolo, sin reintento."""
    d = generar(base, "r10", "--notas", 40, "--aristas", 60)
    inv = escribir(d / "r.sx", '(regla :id "marca-si" :mensaje "m"\n'
                               '  (forall ((v N)) :guarda (Type v x) (Marca v si)))\n')
    p = subprocess.Popen(["universo-mcp", "--root", d / "bundle", "--manifiesto", d / "manifest.toml",
                          "--invariantes", inv], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                         stderr=subprocess.DEVNULL, text=True)

    def enviar(m):
        p.stdin.write(json.dumps(m) + "\n")
        p.stdin.flush()

    enviar({"jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {
        "protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "repro", "version": "0"}}})
    p.stdout.readline()
    enviar({"jsonrpc": "2.0", "method": "notifications/initialized"})
    for i in (1, 2):
        enviar({"jsonrpc": "2.0", "id": i, "method": "tools/call",
                "params": {"name": "check", "arguments": {"schema": "felix-mcp-v1"}}})
    for _ in (1, 2):
        r = json.loads(p.stdout.readline())
        print(f"  respuesta id={r['id']}: " + ("error " + json.dumps(r["error"]) if "error" in r
                                               else "resultado " + r["result"]["structuredContent"]["status"]))
    p.stdin.close()
    p.wait(timeout=10)


def r11(base):
    """CI: un error de sintaxis y un rechazo salen con el mismo código."""
    d = generar(base, "r11", "--notas", 10, "--aristas", 10, "--sin-marca", 1)
    mala = escribir(d / "mala.sx", '(regla :id "r" :mensaje "m" (forall ((v N)) (Marca v)))\n')
    buena = escribir(d / "buena.sx", '(regla :id "marca-si" :mensaje "m"\n'
                                     '  (forall ((v N)) :guarda (Type v x) (Marca v si)))\n')
    u("check", d / "bundle", *ctx(d, mala))
    u("check", d / "bundle", *ctx(d, buena))


def r12(base):
    """escritura_agentes=false no frena a la CLI: se vacían las reglas de vigente y se promueve."""
    g = base / "r12"
    u("init", g, "--git")
    vacias = escribir(base / "vacias.sx", "; sin reglas\n")
    corre = lambda *a: subprocess.run(["universo", *map(str, a)], cwd=g, capture_output=True, text=True)
    for args in (("capturar", "Prefiero reglas cortas", "--proyecto", "demo"),
                 ("promover", "criterios/prefiero-reglas-cortas", "--a", "vigente"),
                 ("reglas", "aplicar", vacias, "--universo", "vigente", "--autor", "agente", "--motivo", "prueba"),
                 ("promover", "criterios/prefiero-reglas-cortas", "--a", "vigente")):
        p = corre(*args)
        print(f"$ universo {' '.join(a if not str(a).startswith('/') else Path(a).name for a in map(str, args))}"
              f"\n  rc={p.returncode}\n  | " + (p.stdout + p.stderr).strip()[:300].replace("\n", "\n  | "))


def r13(base):
    """El universo deja de crecer: presupuesto agregado fijo en propose, aun con reglas con guarda."""
    regla = ('(regla :id "vecinos-{i}" :mensaje "Los vecinos k de una nota x llevan marca si."\n'
             '  (forall ((v N)) :guarda (Type v x) (forall ((e E) (w N)) :guarda (Edge e v w k) (Marca w si))))\n')
    nota = json.dumps({"schema": "felix-delta-v2",
                       "create": [{"path": "n/y0001", "type": "x", "title": "Nota nueva", "body": "Cuerpo.\n"}],
                       "add": ["Marca(n/y0001, si)", "Orden(n/y0001, o01)"], "remove": []})
    for notas, reglas in ((340, 40), (340, 80), (1000, 20), (1000, 40)):
        d = generar(base, f"r13-{notas}-{reglas}", "--notas", notas, "--aristas", notas * 3 // 2)
        inv = escribir(d / "r.sx", "".join(regla.format(i=i) for i in range(1, reglas + 1)))
        delta = escribir(d / "nota.json", nota)
        print(f"  -- {notas} notas, {reglas} reglas")
        u("check", d / "bundle", *ctx(d, inv))
        u("propose", d / "bundle", *ctx(d, inv), "--delta", delta)


def r14(base):
    """propose --apply reescribe todas las notas y las deja en 755, aunque el delta toque una."""
    import os
    d = generar(base, "r14", "--notas", 5, "--aristas", 4)
    inv = escribir(d / "r.sx", '(regla :id "marca-si" :mensaje "m"\n'
                               '  (forall ((v N)) :guarda (Type v x) (Marca v si)))\n')
    u("seal", d / "bundle", *ctx(d, inv))
    modos = lambda: sorted({oct(os.stat(f).st_mode & 0o777) for f in (d / "bundle" / "n").glob("*.md")})
    print(f"  modos antes: {modos()} (umask del proceso)")
    delta = escribir(d / "d.json", '{"schema":"felix-delta-v1","add":["Orden(n/x0002, o05)"],'
                                   '"remove":["Orden(n/x0002, o02)"]}')
    rc, out = u("propose", d / "bundle", *ctx(d, inv), "--delta", delta, "--emit-snapshot-token",
                "--format", "json", recortar=0)
    u("propose", d / "bundle", *ctx(d, inv), "--delta", delta, "--apply", "--expect-snapshot",
      json.loads(out)["snapshot_token"])
    print(f"  modos después: {modos()}; el delta sólo cambia n/x0002")


CASOS = {"R01": r01, "R02": r02, "R03": r03, "R04": r04, "R05": r05, "R06": r06, "R07": r07, "R08": r08,
         "R10": r10, "R11": r11, "R12": r12,
         "R13": r13, "R14": r14}


def main():
    pedidos = [a.upper() for a in sys.argv[1:]] or list(CASOS)
    with tempfile.TemporaryDirectory(prefix="felix-repro-") as t:
        base = Path(t)
        for caso in pedidos:
            print(f"\n## {caso}: {CASOS[caso].__doc__.splitlines()[0]}")
            CASOS[caso](base)


if __name__ == "__main__":
    main()

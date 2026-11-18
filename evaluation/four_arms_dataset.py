"""Tareas sintéticas y claves del oráculo del estudio de cuatro brazos (propuesta 09).

Versión 2 (2026-11-18): escrita después de que la calibración con la versión 1
mostrara 16 de 16 aciertos del baseline y antes de cualquier corrida del
estudio.

Regenera `four_arms/tasks.json` y `four_arms/oracle.json`; el plan congelado
guarda sus hashes.
"""
import json
from pathlib import Path

OUT = Path(__file__).resolve().parent / "four_arms"

MANIFEST = """version = "four-arms/2"
[nodos]
sortes = ["decision", "fuente", "persona", "Retirado", "Patrón"]
[[aristas]]
clave = "revisa"
[[aristas]]
clave = "propone"
[[aristas]]
clave = "apoya"
[[aristas]]
clave = "depende"
[[predicados]]
nombre = "Estado"
clave = "estado"
dominio = ["propuesta", "vigente"]
[[predicados]]
nombre = "Confianza"
clave = "confianza"
dominio = ["baja", "media", "alta"]
orden = true
[[predicados]]
nombre = "Vigencia"
clave = "vigente_hasta"
tipo = "fecha"
[layout]
directorios = ["n"]
"""

# Las restricciones de tipo de las cuatro aristas van en una sola regla: con una
# regla por arista, `propose` agota su presupuesto semántico agregado
# (1 000 000 unidades para todas las reglas) al admitir cambios en este bundle.
INVARIANTS = """(regla :id "revisores" :mensaje "Una decisión vigente necesita al menos dos revisores distintos que no sean su proponente."
  (forall ((d N)) :guarda (Estado d vigente)
    (al-menos 2 ((p N))
      (and (exists ((e E)) (Edge e p d revisa))
           (not (exists ((f E)) (Edge f p d propone)))))))

(regla :id "confianza-vigente" :mensaje "Una decisión vigente tiene confianza media o alta."
  (forall ((d N)) :guarda (Estado d vigente) (>= Confianza d media)))

(regla :id "vigencia" :mensaje "Una decisión vigente tiene vigencia hasta el 2027-01-01 o después."
  (forall ((d N)) :guarda (Estado d vigente) (>= Vigencia d 2027-01-01)))

(regla :id "dependencias-vigentes" :mensaje "Una decisión vigente sólo depende de decisiones vigentes."
  (forall ((e E) (a N) (b N)) :guarda (and (Edge e a b depende) (Estado a vigente))
    (Estado b vigente)))

(regla :id "fuente-confiable" :mensaje "Una fuente que apoya una decisión vigente tiene confianza media o alta."
  (forall ((e E) (f N) (d N)) :guarda (and (Edge e f d apoya) (Estado d vigente))
    (>= Confianza f media)))

(regla :id "tipos-aristas" :mensaje "Revisa y propone van de una persona a una decisión; apoya, de una fuente a una decisión; depende, de una decisión a otra."
  (forall ((e E) (x N) (y N))
    (and (=> (or (Edge e x y revisa) (Edge e x y propone)) (and (Type x persona) (Type y decision)))
         (=> (Edge e x y apoya) (and (Type x fuente) (Type y decision)))
         (=> (Edge e x y depende) (and (Type x decision) (Type y decision))))))
"""

RULES = [
    "Cada nodo tiene exactamente un tipo y un título; reemplaza el hecho anterior al cambiarlo.",
    "Estado, Confianza y Vigencia tienen como máximo un valor por nodo. Estado es propuesta o vigente; "
    "Confianza es baja < media < alta; Vigencia es una fecha AAAA-MM-DD.",
    "Una decisión vigente necesita al menos dos revisores distintos (aristas revisa hacia ella) que no sean su "
    "proponente (arista propone hacia ella): quien propone una decisión no cuenta como su revisor.",
    "Una decisión vigente tiene confianza media o alta.",
    "Una decisión vigente tiene vigencia hasta el 2027-01-01 o después.",
    "Una decisión vigente sólo puede depender (arista depende desde ella) de decisiones vigentes.",
    "Una fuente que apoya una decisión vigente tiene confianza media o alta.",
    "Las aristas revisa y propone van de una persona a una decisión; apoya, de una fuente a una decisión; "
    "depende, de una decisión a otra decisión.",
    "Formato de arista: Edge(n/o→n/d#clave#0, n/o, n/d, clave) junto con su etiqueta vacía "
    'HasLab(n/o→n/d#clave#0, ""); se agregan y se retiran las dos juntas.',
    "Conserva los hechos ajenos al objetivo; no borres nodos ni inventes nuevas identidades. No registres ni "
    "retires revisiones, apoyos, dependencias ni proponentes salvo que el objetivo lo pida o lo permita.",
]

PERSONAS = [("p1", "Ana"), ("p2", "Bruno"), ("p3", "Carla"), ("p4", "Diego"), ("p5", "Elena"), ("p6", "Fabián")]
FUENTES = [("f1", "Informe de carga", "media"), ("f2", "Auditoría externa", "alta"),
           ("f3", "Hilo de foro", "baja"), ("f4", "Encuesta interna", "media")]
# id, título, estado, confianza, vigencia, proponente, revisores, depende de, fuentes que la apoyan
DECISIONES = [
    ("d01", "Arquitectura hexagonal", "vigente", "alta", "2028-01-31", "p1", ["p2", "p3"], [], ["f2"]),
    ("d02", "Puertos de persistencia", "vigente", "media", "2027-11-30", "p2", ["p1", "p4"], ["d01"], []),
    ("d03", "Repositorio por agregado", "vigente", "alta", "2027-09-30", "p3", ["p1", "p5"], ["d02"], []),
    ("d04", "Caché de lectura", "vigente", "media", "2027-08-31", "p4", ["p2", "p6"], ["d03"], []),
    ("d05", "Eventos de dominio", "propuesta", "media", "2027-12-31", "p5", ["p3"], ["d01"], ["f1"]),
    ("d06", "Límites de tasa", "vigente", "alta", "2027-02-15", "p6", ["p1", "p2"], [], ["f1"]),
    ("d07", "Reintentos con backoff", "vigente", "media", "2027-02-28", "p1", ["p3", "p4"], [], ["f1"]),
    ("d08", "Trazas distribuidas", "vigente", "alta", "2027-03-01", "p2", ["p5", "p6"], [], []),
    ("d09", "Feature flags", "propuesta", "alta", "2027-01-15", "p3", ["p4"], [], []),
    ("d10", "Cola de trabajos", "vigente", "media", "2027-01-20", "p4", ["p1", "p5", "p6"], [], ["f4"]),
    ("d11", "Cifrado en reposo", "propuesta", "media", "2027-10-31", "p1", ["p1", "p2"], ["d12"], []),
    ("d12", "Gestión de claves", "propuesta", "alta", "2027-12-31", "p5", ["p6"], [], []),
    ("d13", "Contratos de API", "propuesta", "alta", "2028-03-31", "p2", ["p2", "p4"], [], []),
    ("d14", "Módulo de pagos", "vigente", "alta", "2027-12-31", "p1", ["p2", "p3"], ["d01"], ["f1", "f4"]),
]


PREDICATES = {"estado": "Estado", "confianza": "Confianza", "vigente_hasta": "Vigencia"}


def edge(origin, target, key):
    identity = f"n/{origin}→n/{target}#{key}#0"
    return [f"Edge({identity}, n/{origin}, n/{target}, {key})", f'HasLab({identity}, "")'], identity


def base():
    """Hechos y archivos del bundle: 24 nodos, 53 aristas."""
    salientes = {}
    for d, _t, _e, _c, _v, proponente, revisores, depende, fuentes in DECISIONES:
        salientes.setdefault(proponente, {}).setdefault("propone", []).append(d)
        for p in revisores:
            salientes.setdefault(p, {}).setdefault("revisa", []).append(d)
        for objetivo in depende:
            salientes.setdefault(d, {}).setdefault("depende", []).append(objetivo)
        for f in fuentes:
            salientes.setdefault(f, {}).setdefault("apoya", []).append(d)
    nodos = [(p, "persona", nombre, {}) for p, nombre in PERSONAS]
    nodos += [(f, "fuente", titulo, {"confianza": c}) for f, titulo, c in FUENTES]
    nodos += [(d, "decision", titulo, {"estado": e, "confianza": c, "vigente_hasta": v})
              for d, titulo, e, c, v, *_ in DECISIONES]
    facts, files = [], {}
    for node, sort, title, attributes in nodos:
        path = f"n/{node}"
        facts += [f"Type({path}, {sort})", f'HasLab({path}, "{title}")']
        text = f"---\ntype: {sort}\ntitle: {title}\n"
        for key, value in attributes.items():
            facts.append(f"{PREDICATES[key]}({path}, {value})")
            text += f"{key}: {value}\n"
        for key, targets in sorted(salientes.get(node, {}).items()):
            for target in targets:
                facts += edge(node, target, key)[0]
            text += f"{key}: [{', '.join(json.dumps(f'[[{t}]]') for t in targets)}]\n"
        files[f"{path}.md"] = text + f"---\nDocumento sintético {node}; conserva este cuerpo.\n"
    return sorted(facts), files


def build():
    facts, files = base()

    def claves(*aristas):
        return [f"{kind}:{edge(*a)[1]}" for a in aristas for kind in ("Edge", "HasLab")]

    def atomos(*aristas):
        return [atom for a in aristas for atom in edge(*a)[0]]

    apoyos_f1 = [("f1", "d06", "apoya"), ("f1", "d07", "apoya"), ("f1", "d14", "apoya")]
    # (id, familia, objetivo, requeridos, prohibidos, claves prohibidas, claves mutables)
    specifications = [
        ("cascada_d02", "cascada",
         "La decisión n/d02 fue cuestionada: pásala a estado propuesta. Si por eso otras decisiones dejan de "
         "cumplir las reglas, pásalas también a propuesta; no cambies nada más.",
         [f"Estado(n/{d}, propuesta)" for d in ("d02", "d03", "d04")], [], [],
         [f"Ext:Estado:n/{d}" for d in ("d02", "d03", "d04")]),
        ("retirar_revisor_d01", "cascada",
         "Retira a Bruno (n/p2) como revisor de la decisión n/d01. Si por eso alguna decisión deja de cumplir las "
         "reglas, pásala a estado propuesta; no cambies nada más.",
         [f"Estado(n/{d}, propuesta)" for d in ("d01", "d02", "d03", "d04", "d14")],
         atomos(("p2", "d01", "revisa")), claves(("p2", "d01", "revisa")),
         [f"Ext:Estado:n/{d}" for d in ("d01", "d02", "d03", "d04", "d14")] + claves(("p2", "d01", "revisa"))),
        ("degradar_f1", "multiple",
         "Baja la confianza de la fuente n/f1 a baja. Retira su apoyo sólo a las decisiones vigentes que, por eso, "
         "dejarían de cumplir las reglas; no cambies ninguna decisión.",
         ["Confianza(n/f1, baja)"], atomos(*apoyos_f1), claves(*apoyos_f1),
         ["Ext:Confianza:n/f1"] + claves(*apoyos_f1)),
        ("renovar_vencimientos", "multiple",
         "Renueva hasta el 2028-06-30 todas las decisiones vigentes cuya vigencia termina antes del 2027-03-01. "
         "No cambies ninguna otra.",
         [f"Vigencia(n/{d}, 2028-06-30)" for d in ("d06", "d07", "d10")], [], [],
         [f"Ext:Vigencia:n/{d}" for d in ("d06", "d07", "d10")]),
        ("promover_d13", "proponente",
         "Pon la decisión n/d13 en estado vigente. Dato: Carla (n/p3) también revisó n/d13, aunque todavía no está "
         "registrado.",
         ["Estado(n/d13, vigente)", *atomos(("p3", "d13", "revisa"))], [], [],
         ["Ext:Estado:n/d13", *claves(("p3", "d13", "revisa"))]),
        ("reasignar_d14", "proponente",
         "Cambia el proponente de la decisión n/d14: ya no es Ana (n/p1) sino Bruno (n/p2). Si por eso n/d14 deja de "
         "cumplir las reglas, registra que Elena (n/p5) también la revisó.",
         [*atomos(("p2", "d14", "propone")), *atomos(("p5", "d14", "revisa"))], atomos(("p1", "d14", "propone")),
         claves(("p1", "d14", "propone")),
         claves(("p1", "d14", "propone"), ("p2", "d14", "propone"), ("p5", "d14", "revisa"))),
        ("promover_d11", "imposible",
         "Pon la decisión n/d11 en estado vigente. Si las reglas lo impiden con los hechos registrados, no cambies "
         "nada.",
         [], [], [], []),
        ("verificar_dependientes", "conservacion",
         "Asegura que toda decisión vigente que depende directamente de n/d01 tenga al menos dos revisores que no "
         "sean su proponente. Si ya se cumple, no cambies nada.",
         [], [], [], []),
    ]
    tasks, oracle = [], {}
    for index, (identifier, family, goal, required, forbidden, forbidden_keys, mutable) in enumerate(specifications):
        tasks.append({"id": identifier, "family": family, "seed": 9200 + index, "prompt": goal,
                      "manifest": MANIFEST, "invariants": INVARIANTS, "files": files,
                      "facts": facts, "rules": RULES})
        oracle[identifier] = {"required": required, "forbidden": forbidden, "forbidden_keys": forbidden_keys,
                              "mutable_keys": mutable}
    return ({"schema": "felix-four-arms-tasks-v2", "cases": tasks},
            {"schema": "felix-four-arms-oracle-v2", "cut_date": "2027-01-01", "cases": oracle})


if __name__ == "__main__":
    OUT.mkdir(exist_ok=True)
    tasks, oracle = build()
    for name, value in [("tasks.json", tasks), ("oracle.json", oracle)]:
        (OUT / name).write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")

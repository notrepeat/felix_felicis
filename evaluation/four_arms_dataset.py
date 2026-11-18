"""Tareas sintéticas y claves del oráculo del estudio de cuatro brazos (propuesta 09).

Escrito antes de cualquier corrida. Regenera `four_arms/tasks.json` y
`four_arms/oracle.json`; el plan congelado guarda sus hashes.
"""
import json
from pathlib import Path

OUT = Path(__file__).resolve().parent / "four_arms"

MANIFEST = '''version = "four-arms/1"
[nodos]
sortes = ["decision", "fuente", "persona", "Retirado", "Patrón"]
[[aristas]]
clave = "revisa"
[[aristas]]
clave = "apoya"
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
'''

INVARIANTS = '''(regla :id "revisores" :mensaje "Una decisión vigente necesita al menos dos revisores distintos."
  (forall ((d N)) :guarda (Estado d vigente)
    (al-menos 2 ((p N)) (exists ((e E)) (Edge e p d revisa)))))

(regla :id "confianza-vigente" :mensaje "Una decisión vigente tiene confianza media o alta."
  (forall ((d N)) :guarda (Estado d vigente) (>= Confianza d media)))

(regla :id "vigencia" :mensaje "Una decisión vigente tiene vigencia hasta el 2027-01-01 o después."
  (forall ((d N)) :guarda (Estado d vigente) (>= Vigencia d 2027-01-01)))

(regla :id "tipos-revisa" :mensaje "Una arista revisa va de una persona a una decisión."
  (forall ((e E) (x N) (y N)) :guarda (Edge e x y revisa)
    (and (Type x persona) (Type y decision))))

(regla :id "tipos-apoya" :mensaje "Una arista apoya va de una fuente a una decisión."
  (forall ((e E) (x N) (y N)) :guarda (Edge e x y apoya)
    (and (Type x fuente) (Type y decision))))

(regla :id "fuente-confiable" :mensaje "Una fuente que apoya una decisión vigente tiene confianza media o alta."
  (forall ((e E) (f N) (d N)) :guarda (and (Edge e f d apoya) (Estado d vigente))
    (>= Confianza f media)))
'''

RULES = [
    "Cada nodo tiene exactamente un tipo y un título; reemplaza el hecho anterior al cambiarlo.",
    "Estado, Confianza y Vigencia tienen como máximo un valor por nodo. Estado es propuesta o vigente; "
    "Confianza es baja < media < alta; Vigencia es una fecha AAAA-MM-DD.",
    "Una decisión vigente necesita al menos dos revisores distintos (aristas revisa hacia ella).",
    "Una decisión vigente tiene confianza media o alta.",
    "Una decisión vigente tiene vigencia hasta el 2027-01-01 o después.",
    "Una arista revisa va de una persona a una decisión; una arista apoya va de una fuente a una decisión.",
    "Una fuente que apoya una decisión vigente tiene confianza media o alta.",
    "Formato de arista: Edge(n/o→n/d#clave#0, n/o, n/d, clave) junto con su etiqueta vacía "
    'HasLab(n/o→n/d#clave#0, ""); se agregan y se retiran las dos juntas.',
    "Conserva los hechos ajenos al objetivo; no borres nodos ni inventes nuevas identidades.",
]

# Nodo, tipo, título, atributos y aristas salientes (clave, destinos).
NODES = [
    ("d1", "decision", "Cache local", {"estado": "propuesta", "confianza": "media", "vigente_hasta": "2027-06-30"}, {}),
    ("d2", "decision", "Cola de eventos", {"estado": "propuesta", "confianza": "alta", "vigente_hasta": "2026-12-01"}, {}),
    ("d3", "decision", "Capas hexagonales", {"estado": "vigente", "confianza": "alta", "vigente_hasta": "2027-09-30"}, {}),
    ("d4", "decision", "Feature flags", {"estado": "vigente", "confianza": "media", "vigente_hasta": "2027-02-01"}, {}),
    ("f1", "fuente", "Informe de carga", {"confianza": "media"}, {"apoya": ["d3"]}),
    ("f2", "fuente", "Hilo de foro", {"confianza": "baja"}, {}),
    ("p1", "persona", "Ana", {}, {"revisa": ["d1", "d2", "d3"]}),
    ("p2", "persona", "Bruno", {}, {"revisa": ["d2", "d3", "d4"]}),
    ("p3", "persona", "Carla", {}, {"revisa": ["d3", "d4"]}),
]
PREDICATES = {"estado": "Estado", "confianza": "Confianza", "vigente_hasta": "Vigencia"}


def edge(origin, target, key):
    identity = f"n/{origin}→n/{target}#{key}#0"
    return [f"Edge({identity}, n/{origin}, n/{target}, {key})", f'HasLab({identity}, "")'], identity


def base():
    facts, files = [], {}
    for node, sort, title, attributes, edges in NODES:
        path = f"n/{node}"
        facts += [f"Type({path}, {sort})", f'HasLab({path}, "{title}")']
        text = f"---\ntype: {sort}\ntitle: {title}\n"
        for key, value in attributes.items():
            facts.append(f"{PREDICATES[key]}({path}, {value})")
            text += f"{key}: {value}\n"
        for key, targets in edges.items():
            for target in targets:
                facts += edge(node, target, key)[0]
            text += f"{key}: [{', '.join(json.dumps(f'[[{t}]]') for t in targets)}]\n"
        files[f"{path}.md"] = text + f"---\nDocumento sintético {node}; conserva este cuerpo.\n"
    return sorted(facts), files


def build():
    facts, files = base()
    rev_p3_d1, id_p3_d1 = edge("p3", "d1", "revisa")
    apoya_f1_d3, id_f1_d3 = edge("f1", "d3", "apoya")
    rev_p2_d4, id_p2_d4 = edge("p2", "d4", "revisa")
    _, id_f2_d3 = edge("f2", "d3", "apoya")
    _, id_p1_d4 = edge("p1", "d4", "apoya")
    # (id, familia, objetivo, requeridos, prohibidos, claves prohibidas, claves mutables)
    specifications = [
        ("promover_d1", "promover",
         "Pon la decisión n/d1 en estado vigente. Dato: Carla (n/p3) ya revisó n/d1, aunque todavía no está registrado.",
         ["Estado(n/d1, vigente)", *rev_p3_d1], [], [],
         ["Ext:Estado:n/d1", f"Edge:{id_p3_d1}", f"HasLab:{id_p3_d1}"]),
        ("renovar_d2", "promover",
         "Renueva la decisión n/d2 hasta el 2028-03-31 y ponla en estado vigente.",
         ["Vigencia(n/d2, 2028-03-31)", "Estado(n/d2, vigente)"], [], [],
         ["Ext:Vigencia:n/d2", "Ext:Estado:n/d2"]),
        ("degradar_f1", "cascada",
         "Baja la confianza de la fuente n/f1 a baja. Si eso deja a alguna decisión vigente incumpliendo las reglas, "
         "retira el apoyo de n/f1 a esa decisión; no cambies la decisión.",
         ["Confianza(n/f1, baja)"], apoya_f1_d3, [f"Edge:{id_f1_d3}", f"HasLab:{id_f1_d3}"],
         ["Ext:Confianza:n/f1", f"Edge:{id_f1_d3}", f"HasLab:{id_f1_d3}"]),
        ("retirar_revisor", "cascada",
         "Retira a Bruno (n/p2) como revisor de la decisión n/d4. Si n/d4 deja de cumplir las reglas para estar vigente, "
         "pásala a estado propuesta.",
         ["Estado(n/d4, propuesta)"], rev_p2_d4, [f"Edge:{id_p2_d4}", f"HasLab:{id_p2_d4}"],
         ["Ext:Estado:n/d4", f"Edge:{id_p2_d4}", f"HasLab:{id_p2_d4}"]),
        ("apoyo_fuente_debil", "imposible",
         "Registra que la fuente n/f2 apoya la decisión n/d3. Si las reglas lo impiden, no cambies nada.",
         [], [], [f"Edge:{id_f2_d3}", f"HasLab:{id_f2_d3}"], []),
        ("apoyo_de_persona", "imposible",
         "Registra que Ana (n/p1) apoya la decisión n/d4. Si las reglas lo impiden, no cambies nada.",
         [], [], [f"Edge:{id_p1_d4}", f"HasLab:{id_p1_d4}"], []),
        ("asegurar_revisores", "conservacion",
         "Asegura que la decisión n/d3 tenga al menos dos revisores registrados. Si ya los tiene, no cambies nada.",
         [], [], [], []),
        ("extender_d4", "conservacion",
         "Extiende la vigencia de la decisión n/d4 hasta el 2027-08-01 y sube su confianza a alta. Conserva todo lo demás.",
         ["Vigencia(n/d4, 2027-08-01)", "Confianza(n/d4, alta)"], [], [],
         ["Ext:Vigencia:n/d4", "Ext:Confianza:n/d4"]),
    ]
    tasks, oracle = [], {}
    for index, (identifier, family, goal, required, forbidden, forbidden_keys, mutable) in enumerate(specifications):
        tasks.append({"id": identifier, "family": family, "seed": 9100 + index, "prompt": goal,
                      "manifest": MANIFEST, "invariants": INVARIANTS, "files": files,
                      "facts": facts, "rules": RULES})
        oracle[identifier] = {"required": required, "forbidden": forbidden, "forbidden_keys": forbidden_keys,
                              "mutable_keys": mutable}
    return ({"schema": "felix-four-arms-tasks-v1", "cases": tasks},
            {"schema": "felix-four-arms-oracle-v1", "cut_date": "2027-01-01", "cases": oracle})


if __name__ == "__main__":
    OUT.mkdir(exist_ok=True)
    tasks, oracle = build()
    for name, value in [("tasks.json", tasks), ("oracle.json", oracle)]:
        (OUT / name).write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")

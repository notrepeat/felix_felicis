"""Create the preregistered synthetic paired tasks and independent scoring keys."""
import json
from pathlib import Path

MANIFEST = '''version = "paired/1"
[nodos]
sortes = ["draft", "ready", "archive", "Retirado", "Patrón"]
[[predicados]]
nombre = "Maturity"
clave = "maturity"
dominio = ["draft", "accepted"]
[[predicados]]
nombre = "Owner"
clave = "owner"
dominio = ["red", "blue"]
[layout]
directorios = ["n"]
parent_key = "parent"
'''
INVARIANTS = '(forall ((x N)) (=> (Type x ready) (Maturity x accepted)))\n'
RULES = [
    "Cada nodo tiene exactamente un tipo y un título; reemplaza el hecho anterior al cambiarlo.",
    "Cada nodo tiene como máximo un padre y los enlaces Parent no pueden formar ciclos.",
    "Maturity y Owner tienen un único valor por nodo dentro de sus dominios declarados.",
    "Todo nodo de tipo ready debe tener Maturity accepted.",
    "Conserva los hechos ajenos al objetivo; no borres nodos ni inventes nuevas identidades.",
]


def build():
    facts = []
    files = {}
    for node, title, maturity, owner, parent in [
        ("a", "Alpha", "draft", "red", "n/b"),
        ("b", "Beta", "accepted", "blue", None),
        ("c", "Gamma", "draft", "red", None),
    ]:
        path = f"n/{node}"
        facts += [f"Type({path}, draft)", f'HasLab({path}, "{title}")', f"Maturity({path}, {maturity})", f"Owner({path}, {owner})"]
        text = f"---\ntype: draft\ntitle: {title}\nmaturity: {maturity}\nowner: {owner}\n"
        if parent:
            facts.append(f"Parent({path}, {parent})")
            text += f"parent: {parent}\n"
        files[f"{path}.md"] = text + f"---\nSynthetic document {node}; preserve this body.\n"
    specifications = [
        ("type_a", "type", "Promueve n/a al tipo ready. Cumple las reglas y cambia sólo lo necesario.", ["Type(n/a, ready)", "Maturity(n/a, accepted)"], ["Type:n/a", "Ext:Maturity:n/a"]),
        ("type_b", "type", "Promueve n/b al tipo ready. Su madurez ya es accepted: conserva ese dato y todo lo no requerido.", ["Type(n/b, ready)"], ["Type:n/b"]),
        ("label_copy", "label", "Cambia el título de n/a para que sea igual al título actual de n/b. No cambies n/b ni ningún otro hecho.", ['HasLab(n/a, "Beta")'], ["HasLab:n/a"]),
        ("label_swap", "label", "Intercambia los títulos actuales de n/a y n/c. Conserva tipos, atributos, parentescos y n/b.", ['HasLab(n/a, "Gamma")', 'HasLab(n/c, "Alpha")'], ["HasLab:n/a", "HasLab:n/c"]),
        ("parent_move", "parent", "Reubica n/a bajo n/c en lugar de su padre actual. No cambies otros hechos.", ["Parent(n/a, n/c)"], ["Parent:n/a"]),
        ("parent_rotate", "parent", "Haz que n/b tenga como padre n/a y que n/a tenga como padre n/c. El resultado debe ser acíclico; conserva lo demás.", ["Parent(n/b, n/a)", "Parent(n/a, n/c)"], ["Parent:n/a", "Parent:n/b"]),
        ("owner_copy", "extension", "Asigna a n/a el mismo Owner que tiene n/b. Conserva todos los demás hechos.", ["Owner(n/a, blue)"], ["Ext:Owner:n/a"]),
        ("maturity_accept", "extension", "Cambia solamente la madurez de n/c a accepted; su tipo debe seguir siendo draft.", ["Maturity(n/c, accepted)", "Type(n/c, draft)"], ["Ext:Maturity:n/c"]),
        ("release_c", "compound", "Prepara n/c: tipo ready, título Published, padre n/b. Satisface las reglas de madurez y conserva su Owner y el resto del bundle.", ["Type(n/c, ready)", 'HasLab(n/c, "Published")', "Parent(n/c, n/b)", "Maturity(n/c, accepted)"], ["Type:n/c", "HasLab:n/c", "Parent:n/c", "Ext:Maturity:n/c"]),
        ("archive_a", "compound", "Archiva n/a con tipo archive, título Archived y Owner blue. Debe dejar de tener padre. Conserva su madurez y los demás nodos.", ["Type(n/a, archive)", 'HasLab(n/a, "Archived")', "Owner(n/a, blue)"], ["Type:n/a", "HasLab:n/a", "Ext:Owner:n/a", "Parent:n/a"]),
        ("noop_owner", "no_op", "Asegura que n/b tenga Owner blue. Si ya lo tiene, no modifiques nada; conserva todos los hechos.", ["Owner(n/b, blue)"], []),
        ("noop_parent", "no_op", "Asegura que n/c no tenga padre. Si ya es así, no modifiques nada; conserva todos los hechos.", [], []),
    ]
    tasks, oracle = [], {}
    for index, (identifier, family, prompt, required, mutable) in enumerate(specifications):
        tasks.append({"id": identifier, "family": family, "seed": 7100 + index, "prompt": prompt, "manifest": MANIFEST, "invariants": INVARIANTS, "files": files, "facts": sorted(facts), "rules": RULES})
        forbidden = [atom for atom in facts if atom.startswith("Parent(n/a,")] if identifier == "archive_a" else []
        oracle[identifier] = {"required": required, "forbidden": forbidden, "forbidden_keys": ["Parent:n/a"] if identifier == "archive_a" else (["Parent:n/c"] if identifier == "noop_parent" else []), "mutable_keys": mutable, "constraints": [{"kind": "ready_requires_accepted"}]}
    return {"schema": "felix-paired-tasks-v1", "cases": tasks}, {"schema": "felix-paired-oracle-v1", "cases": oracle}


if __name__ == "__main__":
    tasks, oracle = build()
    for name, value in [("tasks.json", tasks), ("oracle.json", oracle)]:
        Path(__file__).with_name(name).write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")

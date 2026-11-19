use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{Value, json};
use universo_core::{
    Atomo, BundlePath, Delta, Estructura, ResultadoPropuesta, Sorte, Sujeto, construir_candidato,
    parsear_programa, proponer,
};
use universo_okf::{cargar_bundle, cargar_manifiesto, parsear_delta_json_v1, serializar_bundle};

fn fixture_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/software-architecture")
        .join(relative)
}

fn anexos(estructura: &Estructura) -> Vec<(String, Option<universo_core::Anexo>)> {
    let mut anexos: Vec<_> = estructura
        .nodos()
        .map(|(id, nodo)| {
            (
                nodo.path.como_str().to_owned(),
                estructura.anexo(id).cloned(),
            )
        })
        .collect();
    anexos.sort_unstable_by(|a, b| a.0.cmp(&b.0));
    anexos
}

fn afirmar_misma_semantica(esperado: &Estructura, actual: &Estructura) {
    assert_eq!(esperado.atomos(), actual.atomos());
    assert_eq!(anexos(esperado), anexos(actual));
}

fn ejecutar_universo(args: &[&OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_universo"))
        .args(args)
        .output()
        .expect("CLI de universo ejecutable")
}

fn validar(root: &Path, manifiesto: &Path) -> Value {
    let salida = ejecutar_universo(&[
        OsStr::new("validate"),
        root.as_os_str(),
        OsStr::new("--manifiesto"),
        manifiesto.as_os_str(),
        OsStr::new("--format"),
        OsStr::new("json"),
    ]);
    assert_eq!(
        salida.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&salida.stderr)
    );
    assert!(salida.stderr.is_empty());
    let json: Value = serde_json::from_slice(&salida.stdout).unwrap();
    assert_eq!(json["command"], "validate");
    assert_eq!(json["status"], "valid");
    json
}

fn check(root: &Path, manifiesto: &Path, invariantes: &Path) -> Value {
    let salida = ejecutar_universo(&[
        OsStr::new("check"),
        root.as_os_str(),
        OsStr::new("--manifiesto"),
        manifiesto.as_os_str(),
        OsStr::new("--invariantes"),
        invariantes.as_os_str(),
        OsStr::new("--format"),
        OsStr::new("json"),
    ]);
    assert_eq!(
        salida.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&salida.stderr)
    );
    assert!(salida.stderr.is_empty());
    let json: Value = serde_json::from_slice(&salida.stdout).unwrap();
    assert_eq!(json["command"], "check");
    assert_eq!(json["status"], "admit");
    assert_eq!(json["summary"]["failed"], 0);
    json
}

fn check_explicado(
    root: &Path,
    manifiesto: &Path,
    invariantes: &Path,
    modo: &OsStr,
    formato: &OsStr,
) -> Output {
    ejecutar_universo(&[
        OsStr::new("check"),
        root.as_os_str(),
        OsStr::new("--manifiesto"),
        manifiesto.as_os_str(),
        OsStr::new("--invariantes"),
        invariantes.as_os_str(),
        OsStr::new("--explain"),
        modo,
        OsStr::new("--format"),
        formato,
    ])
}

#[test]
fn la_fixture_aporta_la_correccion_y_felix_evalua_y_admite_el_candidato() {
    let bundle_root = fixture_path("bundle");
    let manifiesto_path = fixture_path("manifest.toml");
    let invariantes_path = fixture_path("invariants/architecture.sx");
    let manifiesto = cargar_manifiesto(&manifiesto_path).expect("manifiesto de fixture válido");
    let base = cargar_bundle(&bundle_root, manifiesto.clone())
        .unwrap_or_else(|errores| panic!("bundle base inválido: {errores:?}"));
    for (ruta, sorte) in [
        ("n/domain", "domain"),
        ("n/port", "port"),
        ("n/adapter", "adapter"),
    ] {
        let path = BundlePath::nuevo(ruta).expect("path base válido");
        let id = base.por_path(&path).expect("nodo base existente");
        assert_eq!(base.nodo(id).unwrap().tipo, Sorte::nuevo(sorte));
    }
    let invariantes_texto = fs::read_to_string(&invariantes_path).unwrap();
    let invariantes = parsear_programa(&invariantes_texto).expect("invariante público válido");
    assert!(
        invariantes_texto
            .lines()
            .nth(1)
            .is_some_and(|linea| linea.starts_with("(forall ((e E) (x N) (y N))"))
    );
    assert!(
        invariantes_texto
            .lines()
            .nth(2)
            .is_some_and(|linea| linea.contains("(Edge e x y depends)"))
    );
    let delta_vacio = Delta::nuevo([], []).unwrap();
    let atomos_base = base.atomos();
    let anexos_base = anexos(&base);
    assert!(matches!(
        proponer(&base, &delta_vacio, &invariantes).unwrap(),
        ResultadoPropuesta::Admitida(_)
    ));
    assert_eq!(base.atomos(), atomos_base);
    assert_eq!(anexos(&base), anexos_base);

    let delta_prohibido = parsear_delta_json_v1(
        &fs::read_to_string(fixture_path("proposals/contradictory/delta.json")).unwrap(),
    )
    .expect("delta contradictorio felix-delta-v1 válido");
    match proponer(&base, &delta_prohibido, &invariantes).unwrap() {
        ResultadoPropuesta::RechazadaInvariantes(instancias) => {
            assert_eq!(instancias.len(), 1);
            assert_eq!(instancias[0].source_index, 0);
            assert_eq!(instancias[0].linea, Some(2));
            assert_eq!(instancias[0].columna, Some(1));
            assert_eq!(instancias[0].instance_index, None);
        }
        otro => panic!("la dependencia domain→adapter debía rechazarse: {otro:?}"),
    }
    assert_eq!(base.atomos(), atomos_base);
    assert_eq!(anexos(&base), anexos_base);

    // `proponer` devuelve el rechazo, no un candidato. La prueba reconstruye
    // solo en memoria el estado descrito por la fixture para situar la
    // corrección explícita; nunca persiste ni vuelve a leerlo.
    let candidato_rechazado = construir_candidato(&base, &delta_prohibido)
        .expect("el delta prohibido es estructuralmente construible")
        .into_estructura();
    assert_eq!(base.atomos(), atomos_base);
    assert_eq!(anexos(&base), anexos_base);
    let atomos_candidato_rechazado = candidato_rechazado.atomos();
    for atomo in delta_prohibido.agregar() {
        assert!(atomos_candidato_rechazado.contains(atomo));
    }
    let estado_contradictorio =
        cargar_bundle(&fixture_path("states/contradictory"), manifiesto.clone())
            .unwrap_or_else(|errores| panic!("snapshot contradictorio inválido: {errores:?}"));
    afirmar_misma_semantica(&candidato_rechazado, &estado_contradictorio);

    let (id_arista_domain_adapter, atomo_arista_domain_adapter) = delta_prohibido
        .agregar()
        .iter()
        .find_map(|atomo| match atomo {
            Atomo::Edge {
                id,
                origen,
                destino,
                sorte,
            } if origen.como_str() == "n/domain"
                && destino.como_str() == "n/adapter"
                && sorte.como_str() == "depends" =>
            {
                Some((id.clone(), atomo.clone()))
            }
            _ => None,
        })
        .expect("la propuesta contradictoria añade domain→adapter");
    let atomo_etiqueta_domain_adapter = delta_prohibido
        .agregar()
        .iter()
        .find_map(|atomo| match atomo {
            Atomo::HasLab {
                sujeto: Sujeto::Arista(id),
                etiqueta,
            } if id == &id_arista_domain_adapter && etiqueta.como_str() == "dependency" => {
                Some(atomo.clone())
            }
            _ => None,
        })
        .expect("la propuesta etiqueta la arista contradictoria");
    let atomos_contradictorios = estado_contradictorio.atomos();
    let aristas_contradictorias: Vec<_> = atomos_contradictorios
        .iter()
        .filter(|atomo| matches!(atomo, Atomo::Edge { .. }))
        .collect();
    assert_eq!(aristas_contradictorias, vec![&atomo_arista_domain_adapter]);
    assert!(atomos_contradictorios.contains(&atomo_etiqueta_domain_adapter));
    for (ruta, sorte) in [("n/domain", "domain"), ("n/adapter", "adapter")] {
        assert!(atomos_contradictorios.contains(&Atomo::Type {
            sujeto: BundlePath::nuevo(ruta).unwrap(),
            sorte: Sorte::nuevo(sorte),
        }));
    }
    let archivo_domain_contradictorio =
        fs::read_to_string(fixture_path("states/contradictory/n/domain.md")).unwrap();
    assert!(archivo_domain_contradictorio.contains("to: \"[[adapter]]\""));
    assert!(archivo_domain_contradictorio.contains("kind: dependency"));

    // La salida conserva el testigo abstracto. Su binding falsificante se
    // correlaciona internamente con la única arista domain→adapter del
    // snapshot inventado; el test no añade esos IDs a la salida pública.
    let resultado_check_contradictorio = check_explicado(
        &fixture_path("states/contradictory"),
        &manifiesto_path,
        &invariantes_path,
        OsStr::new("decide"),
        OsStr::new("json"),
    );
    assert_eq!(resultado_check_contradictorio.status.code(), Some(1));
    assert!(resultado_check_contradictorio.stderr.is_empty());
    let json_contradictorio: Value =
        serde_json::from_slice(&resultado_check_contradictorio.stdout).unwrap();
    assert_eq!(json_contradictorio["status"], "reject");
    let objetos_contradictorios = json_contradictorio["explanations"]["objects"]
        .as_array()
        .expect("objetos de explicación de contradicción");
    assert_eq!(objetos_contradictorios.len(), 1);
    let objeto_contradictorio = &objetos_contradictorios[0];
    assert_eq!(objeto_contradictorio["source_index"], 0);
    assert_eq!(objeto_contradictorio["line"], 2);
    assert!(objeto_contradictorio["presence_core"].is_null());
    assert!(objeto_contradictorio["gap"].is_null());
    let contraejemplo_presencia = &objeto_contradictorio["counterexample"];
    assert_eq!(contraejemplo_presencia["rule"], "forall");
    assert_eq!(contraejemplo_presencia["value"], false);
    let bindings_presencia = contraejemplo_presencia["instances"]
        .as_array()
        .expect("bindings del contraejemplo de presencia");
    let bindings_falsificantes: Vec<_> = bindings_presencia
        .iter()
        .filter(|binding| binding["value"] == false)
        .collect();
    assert_eq!(bindings_falsificantes.len(), 1);
    let binding_presencia = bindings_falsificantes[0];
    assert_eq!(binding_presencia["guard"]["predicate"], "edge");
    assert_eq!(binding_presencia["guard"]["value"], true);
    assert_eq!(binding_presencia["body"]["rule"], "or");
    assert_eq!(binding_presencia["body"]["value"], false);
    let disyuntos = binding_presencia["body"]["children"]
        .as_array()
        .expect("disyuntos del testigo de presencia");
    assert_eq!(disyuntos.len(), 2);
    for disyunto in disyuntos {
        assert_eq!(disyunto["rule"], "not");
        assert_eq!(disyunto["value"], false);
        assert_eq!(disyunto["inner"]["predicate"], "type");
        assert_eq!(disyunto["inner"]["value"], true);
    }

    let delta_correctivo = parsear_delta_json_v1(
        &fs::read_to_string(fixture_path("proposals/valid/delta.json")).unwrap(),
    )
    .expect("delta correctivo felix-delta-v1 válido");
    // La fixture aporta la corrección; Felix evalúa la propuesta recibida.
    let candidato = match proponer(&candidato_rechazado, &delta_correctivo, &invariantes).unwrap() {
        ResultadoPropuesta::Admitida(candidato) => candidato,
        otro => panic!("la corrección explícita debía admitirse: {otro:?}"),
    };
    let atomos_candidato = candidato.estructura().atomos();
    for atomo in delta_correctivo.agregar() {
        assert!(atomos_candidato.contains(atomo));
    }
    for atomo in delta_correctivo.retirar() {
        assert!(!atomos_candidato.contains(atomo));
    }
    let arista_retirada = delta_correctivo
        .retirar()
        .iter()
        .find_map(|atomo| match atomo {
            Atomo::Edge { id, .. } => Some(id),
            _ => None,
        })
        .expect("la corrección retira la arista directa");
    let etiqueta_arista_retirada = delta_prohibido
        .agregar()
        .iter()
        .find(|atomo| {
            matches!(
                *atomo,
                Atomo::HasLab {
                    sujeto: Sujeto::Arista(id),
                    ..
                } if id == arista_retirada
            )
        })
        .expect("el delta contradictorio etiqueta la arista directa");
    let mut atomos_esperados = atomos_base.clone();
    for atomo in delta_prohibido.retirar() {
        atomos_esperados.retain(|existente| existente != atomo);
    }
    atomos_esperados.extend(delta_prohibido.agregar().iter().cloned());
    for atomo in delta_correctivo.retirar() {
        atomos_esperados.retain(|existente| existente != atomo);
    }
    atomos_esperados.retain(|atomo| atomo != etiqueta_arista_retirada);
    atomos_esperados.extend(delta_correctivo.agregar().iter().cloned());
    atomos_esperados.sort_unstable();
    assert_eq!(atomos_candidato, atomos_esperados);
    assert!(
        atomos_candidato
            .iter()
            .all(|atomo| { !matches!(atomo, Atomo::Edge { id, .. } if id == arista_retirada) })
    );
    assert!(!atomos_candidato.contains(etiqueta_arista_retirada));
    assert_eq!(anexos_base, anexos(candidato.estructura()));
    let estado_corregido = cargar_bundle(&fixture_path("states/corrected"), manifiesto.clone())
        .unwrap_or_else(|errores| panic!("snapshot corregido inválido: {errores:?}"));
    afirmar_misma_semantica(candidato.estructura(), &estado_corregido);

    let salida = tempfile::tempdir().unwrap();
    serializar_bundle(candidato.estructura(), salida.path()).unwrap();
    let recargado = cargar_bundle(salida.path(), manifiesto.clone())
        .unwrap_or_else(|errores| panic!("candidato serializado inválido: {errores:?}"));
    afirmar_misma_semantica(candidato.estructura(), &recargado);

    let segunda_salida = tempfile::tempdir().unwrap();
    serializar_bundle(&recargado, segunda_salida.path()).unwrap();
    for archivo in ["n/domain.md", "n/port.md", "n/adapter.md"] {
        assert_eq!(
            std::fs::read(salida.path().join(archivo)).unwrap(),
            std::fs::read(segunda_salida.path().join(archivo)).unwrap(),
            "serialización no idempotente de {archivo}"
        );
    }

    let resultado_validate = validar(salida.path(), &manifiesto_path);
    assert_eq!(resultado_validate["summary"]["nodes"], 3);
    assert_eq!(resultado_validate["summary"]["edges"], 2);
    let resultado_check = check(salida.path(), &manifiesto_path, &invariantes_path);
    assert_eq!(resultado_check["status"], "admit");

    let invariantes_gap_path = fixture_path("invariants/architecture-gap.sx");
    let invariantes_gap_texto = fs::read_to_string(&invariantes_gap_path).unwrap();
    let invariantes_gap =
        parsear_programa(&invariantes_gap_texto).expect("invariante forall-exists público válido");
    let delta_gap = parsear_delta_json_v1(
        &fs::read_to_string(fixture_path("proposals/gap/delta.json")).unwrap(),
    )
    .expect("delta gap felix-delta-v1 válido");
    let arista_domain_port = delta_gap
        .retirar()
        .iter()
        .find_map(|atomo| match atomo {
            Atomo::Edge {
                id,
                origen,
                destino,
                ..
            } if origen.como_str() == "n/domain" && destino.como_str() == "n/port" => Some(id),
            _ => None,
        })
        .expect("el delta gap retira domain→port");
    let etiqueta_domain_port = delta_gap
        .retirar()
        .iter()
        .find(|atomo| {
            matches!(
                *atomo,
                Atomo::HasLab {
                    sujeto: Sujeto::Arista(id),
                    ..
                } if id == arista_domain_port
            )
        })
        .expect("el delta gap retira también la etiqueta de la arista");
    let candidato_antes_gap_atomos = candidato.estructura().atomos();
    let candidato_antes_gap_anexos = anexos(candidato.estructura());
    match proponer(candidato.estructura(), &delta_gap, &invariantes_gap).unwrap() {
        ResultadoPropuesta::RechazadaInvariantes(instancias) => {
            assert_eq!(instancias.len(), 1);
            assert_eq!(instancias[0].source_index, 0);
            assert_eq!(instancias[0].linea, Some(2));
            assert_eq!(instancias[0].columna, Some(1));
            assert_eq!(instancias[0].instance_index, None);
        }
        otro => panic!("quitar la conexión requerida debía rechazarse: {otro:?}"),
    }
    assert_eq!(candidato.estructura().atomos(), candidato_antes_gap_atomos);
    assert_eq!(anexos(candidato.estructura()), candidato_antes_gap_anexos);

    // El escenario reconstruye solo en memoria el estado tras el delta gap
    // para compararlo con el snapshot ilustrativo aportado por la fixture.
    let candidato_gap = construir_candidato(candidato.estructura(), &delta_gap)
        .expect("el delta gap es estructuralmente construible")
        .into_estructura();
    assert_eq!(candidato.estructura().atomos(), candidato_antes_gap_atomos);
    assert_eq!(anexos(candidato.estructura()), candidato_antes_gap_anexos);
    assert!(
        candidato_gap
            .atomos()
            .iter()
            .all(|atomo| { !matches!(atomo, Atomo::Edge { id, .. } if id == arista_domain_port) })
    );
    assert!(!candidato_gap.atomos().contains(etiqueta_domain_port));
    let estado_gap = cargar_bundle(&fixture_path("states/gap"), manifiesto.clone())
        .unwrap_or_else(|errores| panic!("snapshot gap inválido: {errores:?}"));
    afirmar_misma_semantica(&candidato_gap, &estado_gap);

    let resultado_validate_gap = validar(&fixture_path("states/gap"), &manifiesto_path);
    assert_eq!(resultado_validate_gap["summary"]["nodes"], 3);
    assert_eq!(resultado_validate_gap["summary"]["edges"], 1);

    let resultado_check_gap = check_explicado(
        &fixture_path("states/gap"),
        &manifiesto_path,
        &invariantes_gap_path,
        OsStr::new("enumerate"),
        OsStr::new("json"),
    );
    assert_eq!(resultado_check_gap.status.code(), Some(1));
    assert!(resultado_check_gap.stderr.is_empty());
    let json_gap: Value = serde_json::from_slice(&resultado_check_gap.stdout).unwrap();
    assert_eq!(json_gap["command"], "check");
    assert_eq!(json_gap["status"], "reject");
    assert_eq!(json_gap["summary"]["failed"], 1);
    assert_eq!(json_gap["explanations"]["mode"], "enumerate");
    let objetos_gap = json_gap["explanations"]["objects"]
        .as_array()
        .expect("objetos de explicación enumerados");
    assert_eq!(objetos_gap.len(), 1);
    let objeto_gap = &objetos_gap[0];
    assert_eq!(objeto_gap["source_index"], 0);
    assert_eq!(objeto_gap["line"], 2);
    assert_eq!(objeto_gap["column"], 1);
    assert!(objeto_gap["instance_index"].is_null());
    assert_eq!(
        objeto_gap["gap"],
        json!({
            "class": "forall_exists_without_witness",
            "counts": {
                "available_nodes": 3,
                "available_edges": 1,
                "existential_candidates_evaluated": 3,
                "universal_bindings_examined": 2
            },
            "form": {
                "body": {"kind": "atom", "predicate": "type"},
                "edge_binders": 1,
                "guard": {"kind": "atom", "predicate": "edge"},
                "node_binders": 1
            }
        })
    );
    assert_eq!(objeto_gap["counterexample"]["rule"], "forall");
    assert_eq!(objeto_gap["counterexample"]["value"], false);
    let bindings_gap = objeto_gap["counterexample"]["instances"]
        .as_array()
        .expect("bindings del contraejemplo gap");
    let bindings_gap_falsificantes: Vec<_> = bindings_gap
        .iter()
        .filter(|binding| binding["value"] == false)
        .collect();
    assert_eq!(bindings_gap_falsificantes.len(), 1);
    let binding_gap = bindings_gap_falsificantes[0];
    assert_eq!(binding_gap["guard"]["predicate"], "type");
    assert_eq!(binding_gap["guard"]["value"], true);
    assert_eq!(binding_gap["body"]["rule"], "exists");
    assert_eq!(binding_gap["body"]["value"], false);
    let candidatos_existenciales = binding_gap["body"]["instances"]
        .as_array()
        .expect("candidatos examinados por el testigo gap");
    assert_eq!(candidatos_existenciales.len(), 3);
    assert!(candidatos_existenciales.iter().all(|candidato| {
        candidato["value"] == false
            && candidato["guard"]["predicate"] == "edge"
            && candidato["guard"]["value"] == false
    }));

    let atomo_arista_domain_port = delta_gap
        .retirar()
        .iter()
        .find(|atomo| matches!(atomo, Atomo::Edge { id, .. } if id == arista_domain_port))
        .expect("el delta retira el átomo concreto domain→port");
    let atomo_etiqueta_domain_port = delta_gap
        .retirar()
        .iter()
        .find(|atomo| matches!(atomo, Atomo::HasLab { sujeto: Sujeto::Arista(id), .. } if id == arista_domain_port))
        .expect("el delta retira el HasLab concreto de domain→port");
    assert!(
        candidato
            .estructura()
            .atomos()
            .contains(atomo_arista_domain_port)
    );
    assert!(
        candidato
            .estructura()
            .atomos()
            .contains(atomo_etiqueta_domain_port)
    );
    assert!(!estado_gap.atomos().contains(atomo_arista_domain_port));
    assert!(!estado_gap.atomos().contains(atomo_etiqueta_domain_port));
    let atomos_gap = estado_gap.atomos();
    let nodos_domain_gap: Vec<_> = atomos_gap
        .iter()
        .filter(|atomo| matches!(atomo, Atomo::Type { sorte, .. } if sorte.como_str() == "domain"))
        .collect();
    assert_eq!(nodos_domain_gap.len(), 1);
    assert!(matches!(
        nodos_domain_gap[0],
        Atomo::Type { sujeto, .. } if sujeto.como_str() == "n/domain"
    ));
    let aristas_gap: Vec<_> = atomos_gap
        .iter()
        .filter(|atomo| matches!(atomo, Atomo::Edge { .. }))
        .collect();
    assert_eq!(aristas_gap.len(), 1);
    assert!(matches!(
        aristas_gap[0],
        Atomo::Edge { origen, destino, sorte, .. }
            if origen.como_str() == "n/port"
                && destino.como_str() == "n/adapter"
                && sorte.como_str() == "depends"
    ));
    let archivo_domain_corregido =
        fs::read_to_string(fixture_path("states/corrected/n/domain.md")).unwrap();
    let archivo_domain_gap = fs::read_to_string(fixture_path("states/gap/n/domain.md")).unwrap();
    assert!(archivo_domain_corregido.contains("to: \"[[port]]\""));
    assert!(archivo_domain_corregido.contains("kind: dependency"));
    assert!(!archivo_domain_gap.contains("depends:"));
    assert!(invariantes_gap_texto.lines().nth(1).is_some_and(|linea| {
        linea.contains("(Type x domain)")
            && linea.contains("(Edge e x y depends)")
            && linea.contains("(Type y port)")
    }));

    let resultado_check_gap_texto = check_explicado(
        &fixture_path("states/gap"),
        &manifiesto_path,
        &invariantes_gap_path,
        OsStr::new("enumerate"),
        OsStr::new("text"),
    );
    assert_eq!(resultado_check_gap_texto.status.code(), Some(1));
    assert!(resultado_check_gap_texto.stderr.is_empty());
    let resultado_check_contradictorio_texto = check_explicado(
        &fixture_path("states/contradictory"),
        &manifiesto_path,
        &invariantes_path,
        OsStr::new("decide"),
        OsStr::new("text"),
    );
    assert_eq!(resultado_check_contradictorio_texto.status.code(), Some(1));
    assert!(resultado_check_contradictorio_texto.stderr.is_empty());
    let marcadores_concretos = [
        "n/domain",
        "n/port",
        "n/adapter",
        "n/domain→n/adapter#depends#0",
        "n/domain→n/port#depends#0",
        "n/port→n/adapter#depends#0",
        "dependency",
        "states/contradictory/n/domain.md",
        "states/gap/n/domain.md",
        "examples/software-architecture/states/contradictory/n/domain.md",
        "examples/software-architecture/states/gap/n/domain.md",
        "architecture.sx",
        "architecture-gap.sx",
        "(forall ((e E) (x N) (y N))\n  :guarda (Edge e x y depends)\n  (or (not (Type x domain)) (not (Type y adapter))))",
        "(forall ((x N)) :guarda (Type x domain) (exists ((e E) (y N)) :guarda (Edge e x y depends) (Type y port)))",
    ];
    for (nombre, bytes) in [
        (
            "contradictory JSON stdout",
            resultado_check_contradictorio.stdout.as_slice(),
        ),
        (
            "contradictory JSON stderr",
            resultado_check_contradictorio.stderr.as_slice(),
        ),
        (
            "contradictory text stdout",
            resultado_check_contradictorio_texto.stdout.as_slice(),
        ),
        (
            "contradictory text stderr",
            resultado_check_contradictorio_texto.stderr.as_slice(),
        ),
        ("gap JSON stdout", resultado_check_gap.stdout.as_slice()),
        ("gap JSON stderr", resultado_check_gap.stderr.as_slice()),
        (
            "gap text stdout",
            resultado_check_gap_texto.stdout.as_slice(),
        ),
        (
            "gap text stderr",
            resultado_check_gap_texto.stderr.as_slice(),
        ),
    ] {
        let salida_publica = String::from_utf8_lossy(bytes);
        for marcador in marcadores_concretos {
            assert!(
                !salida_publica.contains(marcador),
                "{nombre} expuso marcador concreto {marcador:?}"
            );
        }
    }
}

use std::path::Path;
use std::process::{Command, Output};

use serde_json::Value;
use universo_core::{
    Atomo, BundlePath, Delta, Estructura, Etiqueta, IdArista, ResultadoPropuesta, Sorte, Sujeto,
    construir_candidato, parsear_programa, proponer,
};
use universo_okf::{cargar_bundle, cargar_manifiesto, serializar_bundle};
use universo_testkit::bundle_sintetico::Constructor;

const MANIFIESTO: &str = r#"
version = "demo-arquitectura/1"
[nodos]
sortes = ["domain", "port", "adapter", "Retirado", "Patrón"]
[[aristas]]
clave = "depends"
simetrica = false
destino = "to"
etiqueta = "kind"
[layout]
directorios = ["n"]
"#;

const INVARIANTE: &str = r#"
(forall ((e E) (x N) (y N))
  :guarda (Edge e x y depends)
  (or (not (Type x domain)) (not (Type y adapter))))
"#;

fn path(texto: &str) -> BundlePath {
    BundlePath::nuevo(texto).expect("path sintético válido")
}

fn arista_depende(origen: &str, destino: &str) -> [Atomo; 2] {
    let origen = path(origen);
    let destino = path(destino);
    let sorte = Sorte::nuevo("depends");
    let id = IdArista {
        origen: origen.clone(),
        sorte: sorte.clone(),
        destino: destino.clone(),
        ordinal: 0,
    };
    [
        Atomo::Edge {
            id: id.clone(),
            origen,
            destino,
            sorte,
        },
        Atomo::HasLab {
            sujeto: Sujeto::Arista(id),
            etiqueta: Etiqueta::nuevo("dependency"),
        },
    ]
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

fn ejecutar_universo(args: &[String]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_universo"))
        .args(args)
        .output()
        .expect("CLI de universo ejecutable")
}

fn validar(root: &Path, manifiesto: &Path) -> Value {
    let args = [
        "validate".to_owned(),
        root.to_string_lossy().into_owned(),
        "--manifiesto".to_owned(),
        manifiesto.to_string_lossy().into_owned(),
        "--format".to_owned(),
        "json".to_owned(),
    ];
    let salida = ejecutar_universo(&args);
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
    let args = [
        "check".to_owned(),
        root.to_string_lossy().into_owned(),
        "--manifiesto".to_owned(),
        manifiesto.to_string_lossy().into_owned(),
        "--invariantes".to_owned(),
        invariantes.to_string_lossy().into_owned(),
        "--format".to_owned(),
        "json".to_owned(),
    ];
    let salida = ejecutar_universo(&args);
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

#[test]
fn la_fixture_aporta_la_correccion_y_felix_evalua_y_admite_el_candidato() {
    let base_dir = Constructor::nuevo()
        .archivo(
            "n/domain.md",
            "---\ntype: domain\ntitle: Domain\nfixture-note: preserve domain\n---\nCore domain body.\n",
        )
        .archivo(
            "n/port.md",
            "---\ntype: port\ntitle: Port\nfixture-note: preserve port\n---\nApplication boundary.\n",
        )
        .archivo(
            "n/adapter.md",
            "---\ntype: adapter\ntitle: Adapter\nfixture-note: preserve adapter\n---\nExternal adapter.\n",
        )
        .escribir();
    let manifiesto_path = base_dir.path().join("manifest.toml");
    std::fs::write(&manifiesto_path, MANIFIESTO).unwrap();
    let invariantes_path = base_dir.path().join("invariants.sx");
    std::fs::write(&invariantes_path, INVARIANTE).unwrap();

    let manifiesto = cargar_manifiesto(&manifiesto_path).expect("manifiesto sintético válido");
    let base = cargar_bundle(base_dir.path(), manifiesto.clone())
        .unwrap_or_else(|errores| panic!("bundle base inválido: {errores:?}"));
    for (ruta, sorte) in [
        ("n/domain", "domain"),
        ("n/port", "port"),
        ("n/adapter", "adapter"),
    ] {
        let id = base.por_path(&path(ruta)).expect("nodo base existente");
        assert_eq!(base.nodo(id).unwrap().tipo, Sorte::nuevo(sorte));
    }
    let invariantes = parsear_programa(INVARIANTE).expect("invariante público válido");
    let delta_vacio = Delta::nuevo([], []).unwrap();
    let atomos_base = base.atomos();
    let anexos_base = anexos(&base);
    assert!(matches!(
        proponer(&base, &delta_vacio, &invariantes).unwrap(),
        ResultadoPropuesta::Admitida(_)
    ));
    assert_eq!(base.atomos(), atomos_base);
    assert_eq!(anexos(&base), anexos_base);

    let [arista_directa, etiqueta_directa] = arista_depende("n/domain", "n/adapter");
    let delta_prohibido =
        Delta::nuevo([arista_directa.clone(), etiqueta_directa.clone()], []).unwrap();
    match proponer(&base, &delta_prohibido, &invariantes).unwrap() {
        ResultadoPropuesta::RechazadaInvariantes(instancias) => {
            assert_eq!(instancias.len(), 1);
            assert_eq!(instancias[0].source_index, 0);
            assert_eq!(instancias[0].linea, Some(2));
            assert_eq!(instancias[0].instance_index, None);
        }
        otro => panic!("la dependencia domain→adapter debía rechazarse: {otro:?}"),
    }
    assert_eq!(base.atomos(), atomos_base);
    assert_eq!(anexos(&base), anexos_base);

    // `proponer` devuelve el rechazo, no un candidato. La prueba reconstruye
    // esta fixture solo en memoria para aplicar la corrección conocida; nunca
    // persiste ni vuelve a leer el candidato contradictorio.
    let candidato_rechazado = construir_candidato(&base, &delta_prohibido)
        .expect("el delta prohibido es estructuralmente construible")
        .into_estructura();
    assert!(candidato_rechazado.atomos().contains(&arista_directa));

    let [arista_domain_port, etiqueta_domain_port] = arista_depende("n/domain", "n/port");
    let [arista_port_adapter, etiqueta_port_adapter] = arista_depende("n/port", "n/adapter");
    let delta_correctivo = Delta::nuevo(
        [
            arista_domain_port.clone(),
            etiqueta_domain_port.clone(),
            arista_port_adapter.clone(),
            etiqueta_port_adapter.clone(),
        ],
        [arista_directa.clone()],
    )
    .unwrap();
    // La fixture aporta la corrección; Felix evalúa la propuesta recibida.
    let candidato = match proponer(&candidato_rechazado, &delta_correctivo, &invariantes).unwrap() {
        ResultadoPropuesta::Admitida(candidato) => candidato,
        otro => panic!("la corrección explícita debía admitirse: {otro:?}"),
    };
    let atomos_candidato = candidato.estructura().atomos();
    assert!(!atomos_candidato.contains(&arista_directa));
    assert!(!atomos_candidato.contains(&etiqueta_directa));
    for atomo in [
        &arista_domain_port,
        &etiqueta_domain_port,
        &arista_port_adapter,
        &etiqueta_port_adapter,
    ] {
        assert!(atomos_candidato.contains(atomo));
    }

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
}

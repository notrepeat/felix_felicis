use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;
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

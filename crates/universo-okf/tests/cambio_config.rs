//! Fase 10, U5: cambiar reglas o manifiesto sin romper el historial, y sellar
//! un cambio externo dejando rastro.
use std::{fs, path::PathBuf};
use universo_okf::transaccion::{
    cambio_config::{CambioConfig, ResultadoConfig, aplicar_config},
    composicion::componer,
    historial::{Procedencia, leer},
    sello::{EstadoSello, evaluar_guardia, sellar, sellar_con},
};

struct Fixture {
    _dir: tempfile::TempDir,
    root: PathBuf,
    manifest: PathBuf,
    invariants: PathBuf,
}

const MANIFIESTO: &str = "version = \"t/1\"\n[nodos]\nsortes = [\"a\", \"b\", \"Retirado\", \"Patrón\"]\n[layout]\ndirectorios = [\"n\"]\n";

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("bundle");
    fs::create_dir_all(root.join("n")).unwrap();
    fs::write(
        root.join("n/node.md"),
        "---\ntype: a\ntitle: Node\n---\nbody\n",
    )
    .unwrap();
    let manifest = dir.path().join("manifest.toml");
    fs::write(&manifest, MANIFIESTO).unwrap();
    let invariants = dir.path().join("invariants.sx");
    fs::write(&invariants, "(Type @n/node a)\n").unwrap();
    sellar(&root, &manifest, &invariants).unwrap();
    Fixture {
        _dir: dir,
        root,
        manifest,
        invariants,
    }
}

fn procedencia() -> Procedencia {
    Procedencia {
        autor: Some("ana".into()),
        motivo: Some("ajuste".into()),
    }
}

fn aplicar(f: &Fixture, cambio: &CambioConfig<'_>, dry_run: bool) -> ResultadoConfig {
    aplicar_config(
        &f.root,
        &f.manifest,
        &f.invariants,
        cambio,
        &procedencia(),
        dry_run,
    )
    .unwrap()
}

#[test]
fn una_regla_que_se_cumple_se_instala_sella_y_registra() {
    let f = fixture();
    let nueva = "(regla :id \"nodo-a\" :mensaje \"El nodo es a.\" (Type @n/node a))\n";
    let r = aplicar(
        &f,
        &CambioConfig {
            manifiesto: None,
            invariantes: Some(nueva),
        },
        false,
    );
    assert!(
        matches!(
            r,
            ResultadoConfig::Admitido {
                aplicado: true,
                sellado: true,
                registrado: true
            }
        ),
        "{r:?}"
    );
    assert_eq!(fs::read_to_string(&f.invariants).unwrap(), nueva);
    let g = evaluar_guardia(&f.root, &f.manifest, &f.invariants).unwrap();
    assert_eq!(g.sello, EstadoSello::Coincide);
    let h = leer(&f.root).unwrap();
    assert_eq!(h.entradas.len(), 1);
    assert_eq!(h.entradas[0].clase.as_deref(), Some("config"));
    assert_eq!(h.entradas[0].autor.as_deref(), Some("ana"));
    let d = componer(&h, 0, 1).unwrap();
    assert_eq!(d.config, vec![1]);
    assert!(d.agregar.is_empty() && d.retirar.is_empty());
}

#[test]
fn una_regla_que_el_bundle_no_cumple_se_rechaza_sin_escribir_nada() {
    let f = fixture();
    let previa = fs::read_to_string(&f.invariants).unwrap();
    let estricta = "(regla :id \"nodo-b\" :mensaje \"El nodo es b.\" (Type @n/node b))\n";
    let r = aplicar(
        &f,
        &CambioConfig {
            manifiesto: None,
            invariantes: Some(estricta),
        },
        false,
    );
    match r {
        ResultadoConfig::Rechazado { reglas, .. } => {
            assert_eq!(
                reglas,
                vec![(Some("nodo-b".into()), Some("El nodo es b.".into()))]
            );
        }
        otro => panic!("{otro:?}"),
    }
    assert_eq!(fs::read_to_string(&f.invariants).unwrap(), previa);
    assert!(leer(&f.root).unwrap().entradas.is_empty());
    let g = evaluar_guardia(&f.root, &f.manifest, &f.invariants).unwrap();
    assert!(g.aprobada());
}

#[test]
fn dry_run_no_escribe_y_un_cambio_de_manifiesto_tambien_se_registra() {
    let f = fixture();
    let manifiesto = MANIFIESTO.replace("\"b\", ", "\"b\", \"c\", ");
    let cambio = CambioConfig {
        manifiesto: Some(&manifiesto),
        invariantes: None,
    };
    let r = aplicar(&f, &cambio, true);
    assert!(
        matches!(
            r,
            ResultadoConfig::Admitido {
                aplicado: false,
                ..
            }
        ),
        "{r:?}"
    );
    assert_eq!(fs::read_to_string(&f.manifest).unwrap(), MANIFIESTO);
    assert!(leer(&f.root).unwrap().entradas.is_empty());

    let r = aplicar(&f, &cambio, false);
    assert!(
        matches!(r, ResultadoConfig::Admitido { aplicado: true, .. }),
        "{r:?}"
    );
    assert_eq!(fs::read_to_string(&f.manifest).unwrap(), manifiesto);
    let h = leer(&f.root).unwrap();
    assert_eq!(h.entradas[0].clase.as_deref(), Some("config"));
    // Ningún temporal queda junto a los archivos de configuración.
    let sobrantes: Vec<_> = fs::read_dir(f.manifest.parent().unwrap())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .contains(".felix-config.tmp")
        })
        .collect();
    assert!(sobrantes.is_empty());
}

#[test]
fn sellar_una_edicion_externa_deja_una_entrada_seal_que_corta_el_diff() {
    let f = fixture();
    fs::write(
        f.root.join("n/node.md"),
        "---\ntype: a\ntitle: Node\n---\nbody editado a mano\n",
    )
    .unwrap();
    let g = evaluar_guardia(&f.root, &f.manifest, &f.invariants).unwrap();
    assert_eq!(g.sello, EstadoSello::Difiere);
    assert!(sellar_con(&f.root, &f.manifest, &f.invariants, &procedencia()).unwrap());
    let h = leer(&f.root).unwrap();
    assert_eq!(h.entradas.len(), 1);
    assert_eq!(h.entradas[0].clase.as_deref(), Some("seal"));
    assert_eq!(componer(&h, 0, 1).unwrap_err().codigo, "external_change");
    // Volver a sellar el mismo estado no añade nada.
    assert!(!sellar_con(&f.root, &f.manifest, &f.invariants, &procedencia()).unwrap());
    assert_eq!(leer(&f.root).unwrap().entradas.len(), 1);
}

#[test]
fn una_procedencia_invalida_no_cambia_nada() {
    let f = fixture();
    let mala = Procedencia {
        autor: Some("\u{202e}x".into()),
        motivo: None,
    };
    let e = aplicar_config(
        &f.root,
        &f.manifest,
        &f.invariants,
        &CambioConfig {
            manifiesto: None,
            invariantes: Some("(Type @n/node a)\n"),
        },
        &mala,
        false,
    )
    .unwrap_err();
    assert_eq!(e.0, "provenance_invalid");
    assert_eq!(
        fs::read_to_string(&f.invariants).unwrap(),
        "(Type @n/node a)\n"
    );
    assert!(leer(&f.root).unwrap().entradas.is_empty());
}

fn sello(f: &Fixture) -> Vec<u8> {
    fs::read(f.root.parent().unwrap().join(".bundle.felix.seal")).unwrap()
}

fn sin_temporales(f: &Fixture) {
    let sobrantes: Vec<_> = fs::read_dir(f.manifest.parent().unwrap())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains("felix-config"))
        .collect();
    assert!(sobrantes.is_empty(), "{sobrantes:?}");
}

#[test]
fn con_una_edicion_externa_sin_sellar_no_cambia_la_configuracion() {
    let f = fixture();
    fs::write(
        f.root.join("n/node.md"),
        "---\ntype: a\ntitle: Node\n---\nbody editado\n",
    )
    .unwrap();
    let e = aplicar_config(
        &f.root,
        &f.manifest,
        &f.invariants,
        &CambioConfig {
            manifiesto: None,
            invariantes: Some("(Type @n/node a)\n; otra\n"),
        },
        &procedencia(),
        false,
    )
    .unwrap_err();
    assert_eq!(e.0, "config_seal_mismatch");
    assert_eq!(
        fs::read_to_string(&f.invariants).unwrap(),
        "(Type @n/node a)\n"
    );
    assert!(leer(&f.root).unwrap().entradas.is_empty());
    let g = evaluar_guardia(&f.root, &f.manifest, &f.invariants).unwrap();
    assert_eq!(g.sello, EstadoSello::Difiere);
}

#[test]
fn un_rechazo_o_unas_reglas_mal_formadas_no_tocan_sello_ni_historial() {
    let f = fixture();
    let antes = sello(&f);
    let r = aplicar(
        &f,
        &CambioConfig {
            manifiesto: None,
            invariantes: Some("(Type @n/node b)\n"),
        },
        false,
    );
    assert!(matches!(r, ResultadoConfig::Rechazado { .. }), "{r:?}");
    let e = aplicar_config(
        &f.root,
        &f.manifest,
        &f.invariants,
        &CambioConfig {
            manifiesto: None,
            invariantes: Some("(Type @n/node\n"),
        },
        &procedencia(),
        false,
    )
    .unwrap_err();
    assert_ne!(e.0, "config_io");
    assert_eq!(sello(&f), antes);
    assert!(leer(&f.root).unwrap().entradas.is_empty());
    assert_eq!(
        fs::read_to_string(&f.invariants).unwrap(),
        "(Type @n/node a)\n"
    );
    sin_temporales(&f);
}

#[test]
fn un_sello_atrasado_no_se_registra_como_cambio_externo() {
    let f = fixture();
    let viejo = sello(&f);
    let nueva = "(regla :id \"r\" :mensaje \"m\" (Type @n/node a))\n";
    aplicar(
        &f,
        &CambioConfig {
            manifiesto: None,
            invariantes: Some(nueva),
        },
        false,
    );
    // Como si el cambio se hubiera registrado sin llegar a sellar.
    fs::write(f.root.parent().unwrap().join(".bundle.felix.seal"), viejo).unwrap();
    assert!(!sellar_con(&f.root, &f.manifest, &f.invariants, &procedencia()).unwrap());
    let h = leer(&f.root).unwrap();
    assert_eq!(h.entradas.len(), 1);
    assert!(componer(&h, 0, 1).is_ok());
    let g = evaluar_guardia(&f.root, &f.manifest, &f.invariants).unwrap();
    assert!(g.aprobada());
}

#[test]
fn un_sello_ilegible_se_reemplaza_y_queda_como_cambio_externo() {
    let f = fixture();
    fs::write(
        f.root.parent().unwrap().join(".bundle.felix.seal"),
        "<<<<<<< HEAD\nfelix-seal-v1 00\n=======\n>>>>>>> otro\n",
    )
    .unwrap();
    assert!(sellar_con(&f.root, &f.manifest, &f.invariants, &procedencia()).unwrap());
    let h = leer(&f.root).unwrap();
    assert_eq!(h.entradas[0].clase.as_deref(), Some("seal"));
    let g = evaluar_guardia(&f.root, &f.manifest, &f.invariants).unwrap();
    assert!(g.aprobada());
}

#[cfg(unix)]
#[test]
fn una_configuracion_enlazada_no_se_reemplaza() {
    let f = fixture();
    let real = f.invariants.with_file_name("real.sx");
    fs::rename(&f.invariants, &real).unwrap();
    std::os::unix::fs::symlink(&real, &f.invariants).unwrap();
    let e = aplicar_config(
        &f.root,
        &f.manifest,
        &f.invariants,
        &CambioConfig {
            manifiesto: None,
            invariantes: Some("(Type @n/node a)\n"),
        },
        &procedencia(),
        false,
    )
    .unwrap_err();
    assert_eq!(e.0, "config_path_symlink");
    assert!(
        fs::symlink_metadata(&f.invariants)
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[test]
fn si_el_historial_no_se_puede_escribir_seal_no_sella() {
    let f = fixture();
    fs::write(
        f.root.join("n/node.md"),
        "---\ntype: a\ntitle: Node\n---\nbody editado\n",
    )
    .unwrap();
    // Un directorio donde va el historial: no se puede añadir la línea.
    fs::create_dir(f.root.parent().unwrap().join(".bundle.felix.log")).unwrap();
    let e = sellar_con(&f.root, &f.manifest, &f.invariants, &procedencia()).unwrap_err();
    assert_eq!(e.codigo(), "seal_history_io");
    let g = evaluar_guardia(&f.root, &f.manifest, &f.invariants).unwrap();
    assert_eq!(g.sello, EstadoSello::Difiere);
}

use super::super::{ErrorJournalAnclado, IdentidadArchivoJournal};
use super::super::{JournalTemporalAnclado, ResultadoPublicacionJournal};
use crate::transaccion::lock::LockEscritura;
use crate::transaccion::{anclado::journal_temporal::identidad_archivo, raiz::UbicacionRaiz};
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use tempfile::tempdir;

fn crear_root(parent: &Path) -> PathBuf {
    let root = parent.join("bundle");
    fs::create_dir(&root).unwrap();
    root
}

fn nombre_temporal(parent: &Path) -> PathBuf {
    parent.join(".bundle.felix.journal.tmp")
}

fn nombre_journal(root: &Path) -> PathBuf {
    UbicacionRaiz::nueva(root)
        .unwrap()
        .journal()
        .as_path()
        .to_path_buf()
}

fn identidad(path: &Path) -> IdentidadArchivoJournal {
    identidad_archivo(&File::open(path).unwrap()).unwrap()
}

fn confirmado(resultado: ResultadoPublicacionJournal) -> bool {
    matches!(resultado, ResultadoPublicacionJournal::Confirmado { .. })
}

#[test]
fn publicar_preserva_bytes_e_identidad_y_quita_temporal() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability: JournalTemporalAnclado<'_> = lock.journal_temporal_anclado().unwrap();
    let bytes = b"published journal bytes";
    let esperada = capability.crear_nuevo(bytes).unwrap();
    let journal = nombre_journal(&root);

    assert!(confirmado(capability.publicar(esperada)));
    assert_eq!(identidad(&journal), esperada);
    assert_eq!(fs::read(journal).unwrap(), bytes);
    assert!(!nombre_temporal(temp.path()).exists());
}

#[test]
fn identidad_incorrecta_conserva_temporal_y_no_crea_canonico() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = lock.journal_temporal_anclado().unwrap();
    let temporal = nombre_temporal(temp.path());
    let journal = nombre_journal(&root);
    let bytes = b"expected source";
    capability.crear_nuevo(bytes).unwrap();
    let otro = temp.path().join("other");
    fs::write(&otro, b"other identity").unwrap();

    let mut syscall_invocado = false;
    assert_eq!(
        capability.publicar_con(identidad(&otro), |_, _| {
            syscall_invocado = true;
            Err(ErrorJournalAnclado::Io)
        }),
        ResultadoPublicacionJournal::PrecondicionFallida(ErrorJournalAnclado::IdentityMismatch)
    );
    assert!(!syscall_invocado);
    assert_eq!(fs::read(temporal).unwrap(), bytes);
    assert!(!journal.exists());
}

#[test]
fn archivo_canonico_en_conflicto_conserva_ambos_canaries() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = lock.journal_temporal_anclado().unwrap();
    let temporal = nombre_temporal(temp.path());
    let journal = nombre_journal(&root);
    let bytes_temporal = b"temporary canary";
    let bytes_canonicos = b"canonical canary";
    let esperada = capability.crear_nuevo(bytes_temporal).unwrap();
    fs::write(&journal, bytes_canonicos).unwrap();

    assert_eq!(
        capability.publicar(esperada),
        ResultadoPublicacionJournal::NoAplicado(ErrorJournalAnclado::CanonicalConflict)
    );
    assert_eq!(fs::read(temporal).unwrap(), bytes_temporal);
    assert_eq!(fs::read(journal).unwrap(), bytes_canonicos);
}

#[test]
fn directorio_canonico_en_conflicto_conserva_canaries() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = lock.journal_temporal_anclado().unwrap();
    let temporal = nombre_temporal(temp.path());
    let journal = nombre_journal(&root);
    let bytes_temporal = b"temporary canary";
    let esperada = capability.crear_nuevo(bytes_temporal).unwrap();
    fs::create_dir(&journal).unwrap();
    fs::write(journal.join("canary"), b"directory canary").unwrap();

    assert_eq!(
        capability.publicar(esperada),
        ResultadoPublicacionJournal::NoAplicado(ErrorJournalAnclado::RegularFileRequired)
    );
    assert_eq!(fs::read(temporal).unwrap(), bytes_temporal);
    assert_eq!(
        fs::read(journal.join("canary")).unwrap(),
        b"directory canary"
    );
}

#[test]
fn fuente_ausente_falla_con_codigo_estable() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let otro = temp.path().join("other");
    fs::write(&otro, b"identity source").unwrap();
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = lock.journal_temporal_anclado().unwrap();

    assert_eq!(
        capability.publicar(identidad(&otro)),
        ResultadoPublicacionJournal::PrecondicionFallida(ErrorJournalAnclado::SourceMissing)
    );
    assert!(!nombre_journal(&root).exists());
}

#[cfg(windows)]
#[test]
fn raiz_unc_rechaza_publicacion_sin_tocar_temporal_ni_canonico() {
    let unc = Path::new(r"\\server\share\bundle");
    assert!(super::raiz_es_remota(unc));
    assert!(super::raiz_es_remota(Path::new(
        r"\\?\UNC\server\share\bundle"
    )));
    assert!(!super::raiz_es_remota(Path::new(r"C:\bundle")));
    assert!(!super::raiz_es_remota(Path::new(r"\\?\C:\bundle")));
    assert!(!super::raiz_es_remota(Path::new(
        r"\\?\Volume{GUID}\bundle"
    )));

    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let mut capability = lock.journal_temporal_anclado().unwrap();
    let temporal = nombre_temporal(temp.path());
    let journal = nombre_journal(&root);
    let bytes_temporales = b"temporary canary";
    let bytes_canonicos = b"canonical canary";
    let esperada = capability.crear_nuevo(bytes_temporales).unwrap();
    fs::write(&journal, bytes_canonicos).unwrap();
    let identidad_canonica = identidad(&journal);
    capability.raiz_remota = super::raiz_es_remota(unc);

    assert_eq!(
        capability.publicar(esperada),
        ResultadoPublicacionJournal::PrecondicionFallida(ErrorJournalAnclado::RemoteUnsupported)
    );
    assert_eq!(identidad(&temporal), esperada);
    assert_eq!(fs::read(&temporal).unwrap(), bytes_temporales);
    assert_eq!(identidad(&journal), identidad_canonica);
    assert_eq!(fs::read(&journal).unwrap(), bytes_canonicos);
}

#[cfg(unix)]
#[test]
fn reintento_despues_de_linkat_ve_el_mismo_inode_y_limpia_temporal() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = lock.journal_temporal_anclado().unwrap();
    let temporal = nombre_temporal(temp.path());
    let journal = nombre_journal(&root);
    let bytes = b"committed before crash";
    let esperada = capability.crear_nuevo(bytes).unwrap();
    fs::hard_link(&temporal, &journal).unwrap();

    assert!(confirmado(capability.publicar(esperada)));
    assert_eq!(identidad(&journal), esperada);
    assert_eq!(fs::read(journal).unwrap(), bytes);
    assert!(!temporal.exists());
}

#[cfg(unix)]
#[test]
fn fallo_de_cleanup_despues_de_linkat_devuelve_publicado_pendiente() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = lock.journal_temporal_anclado().unwrap();
    let temporal = nombre_temporal(temp.path());
    let journal = nombre_journal(&root);
    let bytes = b"committed with uncertain cleanup";
    let esperada = capability.crear_nuevo(bytes).unwrap();
    let fuente = super::abrir_temporal(&capability.parent, &capability.nombre).unwrap();

    super::publicar_plataforma(
        &capability.parent,
        &capability.nombre,
        &capability.nombre_journal,
        &fuente,
    )
    .unwrap();
    let mut cleanup_llamado = false;
    let resultado = capability.resultado_despues_de_commit(esperada, |_, _| {
        cleanup_llamado = true;
        super::EstadoLimpiezaPublicacion::Pendiente
    });

    assert!(cleanup_llamado);
    assert!(confirmado(resultado));
    assert_eq!(identidad(&temporal), esperada);
    assert_eq!(identidad(&journal), esperada);
    assert_eq!(fs::read(temporal).unwrap(), bytes);
    assert_eq!(fs::read(journal).unwrap(), bytes);
}

#[cfg(unix)]
#[test]
fn linkat_exitoso_con_verificacion_ausente_o_incierta_es_indeterminado() {
    for observacion in [Ok(false), Err(ErrorJournalAnclado::Io)] {
        let temp = tempdir().unwrap();
        let root = crear_root(temp.path());
        let mut lock = LockEscritura::adquirir(&root).unwrap();
        let capability = lock.journal_temporal_anclado().unwrap();
        let temporal = nombre_temporal(temp.path());
        let journal = nombre_journal(&root);
        let bytes = b"keep evidence when verification is uncertain";
        let esperada = capability.crear_nuevo(bytes).unwrap();
        let fuente = super::abrir_temporal(&capability.parent, &capability.nombre).unwrap();

        super::publicar_plataforma(
            &capability.parent,
            &capability.nombre,
            &capability.nombre_journal,
            &fuente,
        )
        .unwrap();
        let mut cleanup_llamado = false;
        let resultado = capability.resultado_despues_de_commit_observando(
            esperada,
            |_, _| observacion,
            |_, _| {
                cleanup_llamado = true;
                super::EstadoLimpiezaPublicacion::Pendiente
            },
        );

        assert!(matches!(
            resultado,
            ResultadoPublicacionJournal::Indeterminado { .. }
        ));
        assert!(!cleanup_llamado);
        assert_eq!(identidad(&temporal), esperada);
        assert_eq!(identidad(&journal), esperada);
        assert_eq!(fs::read(&temporal).unwrap(), bytes);
        assert_eq!(fs::read(&journal).unwrap(), bytes);
    }
}

#[cfg(any(unix, windows))]
#[test]
fn efecto_de_publicacion_seguido_de_error_se_confirma_como_ya_publicado() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = lock.journal_temporal_anclado().unwrap();
    let temporal = nombre_temporal(temp.path());
    let journal = nombre_journal(&root);
    let bytes = b"effect completed before platform error";
    let esperada = capability.crear_nuevo(bytes).unwrap();
    let fuente = super::abrir_temporal(&capability.parent, &capability.nombre).unwrap();

    super::publicar_plataforma(
        &capability.parent,
        &capability.nombre,
        &capability.nombre_journal,
        &fuente,
    )
    .unwrap();
    let resultado = capability.resolver_error_publicacion(esperada, ErrorJournalAnclado::Io);

    assert!(confirmado(resultado));
    assert_eq!(identidad(&journal), esperada);
    assert_eq!(fs::read(journal).unwrap(), bytes);
    assert!(!temporal.exists());
}

#[test]
fn error_de_publicacion_con_observaciones_inciertas_preserva_evidencia() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = lock.journal_temporal_anclado().unwrap();
    let temporal = nombre_temporal(temp.path());
    let journal = nombre_journal(&root);
    let bytes = b"preserve evidence after uncertain observations";
    let esperada = capability.crear_nuevo(bytes).unwrap();
    let mut canonico_observado = false;
    let mut temporal_observado = false;
    let mut limpieza_llamada = false;

    let resultado = capability.resolver_error_publicacion_observando(
        esperada,
        ErrorJournalAnclado::Io,
        |_, _| {
            canonico_observado = true;
            Err(ErrorJournalAnclado::Io)
        },
        |_, _| {
            temporal_observado = true;
            Err(ErrorJournalAnclado::Io)
        },
        |_, _| {
            limpieza_llamada = true;
            super::EstadoLimpiezaPublicacion::Completa
        },
    );

    assert!(matches!(
        resultado,
        ResultadoPublicacionJournal::Indeterminado { .. }
    ));
    assert!(canonico_observado);
    assert!(temporal_observado);
    assert!(!limpieza_llamada);
    assert_eq!(identidad(&temporal), esperada);
    assert_eq!(fs::read(&temporal).unwrap(), bytes);
    assert!(!journal.exists());
}

#[cfg(any(unix, windows))]
#[test]
fn cleanup_clasifica_conflictos_de_apertura_y_preserva_canaries() {
    #[cfg(unix)]
    let casos = [
        (false, ErrorJournalAnclado::RegularFileRequired),
        (true, ErrorJournalAnclado::ReparseNotAllowed),
    ];
    #[cfg(windows)]
    let casos = [
        (false, ErrorJournalAnclado::RegularFileRequired),
        (true, ErrorJournalAnclado::ReparseNotAllowed),
    ];
    for (es_enlace, error) in casos {
        let temp = tempdir().unwrap();
        let root = crear_root(temp.path());
        let mut lock = LockEscritura::adquirir(&root).unwrap();
        let capability = lock.journal_temporal_anclado().unwrap();
        let temporal = nombre_temporal(temp.path());
        let journal = nombre_journal(&root);
        let outside = temp.path().join("outside-canary");
        let bytes = b"canonical remains committed";
        let esperada = capability.crear_nuevo(bytes).unwrap();
        let fuente = super::abrir_temporal(&capability.parent, &capability.nombre).unwrap();
        super::publicar_plataforma(
            &capability.parent,
            &capability.nombre,
            &capability.nombre_journal,
            &fuente,
        )
        .unwrap();
        if temporal.exists() {
            fs::remove_file(&temporal).unwrap();
        }
        if es_enlace {
            #[cfg(unix)]
            {
                fs::write(&outside, b"outside canary").unwrap();
                std::os::unix::fs::symlink(&outside, &temporal).unwrap();
            }
            #[cfg(windows)]
            {
                fs::create_dir(&outside).unwrap();
                fs::write(outside.join("canary"), b"outside canary").unwrap();
                let output = std::process::Command::new("cmd")
                    .args(["/c", "mklink", "/J"])
                    .arg(&temporal)
                    .arg(&outside)
                    .output()
                    .expect("cmd crea junction temporal");
                if !output.status.success() {
                    let detalle = format!(
                        "{}{}",
                        String::from_utf8_lossy(&output.stdout),
                        String::from_utf8_lossy(&output.stderr)
                    );
                    if detalle.to_ascii_lowercase().contains("privilege") {
                        eprintln!(
                            "SKIP explícito: Windows no concedió privilegio para la junction"
                        );
                        continue;
                    }
                    panic!("no se pudo crear junction: {detalle}");
                }
            }
        } else {
            fs::create_dir(&temporal).unwrap();
            fs::write(temporal.join("canary"), b"directory canary").unwrap();
        }

        let resultado = capability
            .resultado_despues_de_commit(esperada, |capability, identidad| {
                capability.limpiar_temporal_si_coincide(identidad)
            });
        assert_eq!(
            resultado,
            ResultadoPublicacionJournal::Confirmado {
                limpieza: super::super::LimpiezaConfirmadaJournal::Conflicto(error),
            }
        );
        let debug = format!("{resultado:?}");
        assert!(debug.contains(error.codigo()));
        assert!(!debug.contains(temp.path().to_string_lossy().as_ref()));
        assert_eq!(fs::read(&journal).unwrap(), bytes);
        if es_enlace {
            #[cfg(unix)]
            {
                assert!(
                    fs::symlink_metadata(&temporal)
                        .unwrap()
                        .file_type()
                        .is_symlink()
                );
                assert_eq!(fs::read(outside).unwrap(), b"outside canary");
            }
            #[cfg(windows)]
            {
                assert!(temporal.exists());
                assert_eq!(fs::read(outside.join("canary")).unwrap(), b"outside canary");
            }
        } else {
            assert_eq!(
                fs::read(temporal.join("canary")).unwrap(),
                b"directory canary"
            );
        }
    }
}

#[test]
fn conflictos_de_cleanup_confirmado_y_limpieza_indeterminada_son_outcomes() {
    for limpieza in [
        super::EstadoLimpiezaPublicacion::Completa,
        super::EstadoLimpiezaPublicacion::Pendiente,
        super::EstadoLimpiezaPublicacion::Conflicto(ErrorJournalAnclado::IdentityMismatch),
        super::EstadoLimpiezaPublicacion::Indeterminada,
    ] {
        assert_eq!(
            super::resultado_con_limpieza_confirmada(limpieza),
            ResultadoPublicacionJournal::Confirmado { limpieza }
        );
    }
}

#[test]
fn clasificador_post_intento_es_total_y_conservador() {
    use super::{ClasificacionErrorPostIntento as C, clasificar_error_post_intento as clasificar};

    assert_eq!(
        clasificar(ErrorJournalAnclado::Io, Ok(true), None),
        C::Confirmado
    );
    assert_eq!(
        clasificar(ErrorJournalAnclado::Io, Ok(false), Some(Ok(true))),
        C::NoAplicado(ErrorJournalAnclado::Io)
    );
    assert_eq!(
        clasificar(
            ErrorJournalAnclado::CanonicalConflict,
            Err(ErrorJournalAnclado::RegularFileRequired),
            Some(Err(ErrorJournalAnclado::Io)),
        ),
        C::NoAplicado(ErrorJournalAnclado::RegularFileRequired)
    );
    for (canonico, temporal) in [
        (Ok(false), None),
        (Ok(false), Some(Ok(false))),
        (Ok(false), Some(Err(ErrorJournalAnclado::Io))),
        (Err(ErrorJournalAnclado::Io), Some(Ok(true))),
        (
            Err(ErrorJournalAnclado::IdentityMismatch),
            Some(Err(ErrorJournalAnclado::Io)),
        ),
    ] {
        assert!(matches!(
            clasificar(ErrorJournalAnclado::Io, canonico, temporal),
            C::Indeterminado(_)
        ));
    }
}

#[cfg(any(unix, windows))]
#[test]
fn resolver_error_distingue_no_aplicado_de_commit_confirmado() {
    let error = ErrorJournalAnclado::IdentityMismatch;

    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = lock.journal_temporal_anclado().unwrap();
    let temporal = nombre_temporal(temp.path());
    let journal = nombre_journal(&root);
    let bytes = b"expected source before commit";
    let esperada = capability.crear_nuevo(bytes).unwrap();
    assert_eq!(
        capability.resolver_error_publicacion(esperada, error),
        ResultadoPublicacionJournal::NoAplicado(error)
    );
    assert_eq!(fs::read(&temporal).unwrap(), bytes);
    assert!(!journal.exists());
    drop(lock);

    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = lock.journal_temporal_anclado().unwrap();
    let temporal = nombre_temporal(temp.path());
    let journal = nombre_journal(&root);
    let bytes = b"canonical commit is already confirmed";
    let esperada = capability.crear_nuevo(bytes).unwrap();
    let fuente = super::abrir_temporal(&capability.parent, &capability.nombre).unwrap();
    super::publicar_plataforma(
        &capability.parent,
        &capability.nombre,
        &capability.nombre_journal,
        &fuente,
    )
    .unwrap();
    if temporal.exists() {
        fs::remove_file(&temporal).unwrap();
    }
    fs::write(&temporal, b"replacement temp").unwrap();

    let outcome = capability.resolver_error_publicacion(esperada, error);
    assert_eq!(
        outcome,
        ResultadoPublicacionJournal::Confirmado {
            limpieza: super::super::LimpiezaConfirmadaJournal::Conflicto(error),
        }
    );
    let debug = format!("{outcome:?}");
    assert!(debug.contains("journal_identity_mismatch"));
    assert!(!debug.contains(temp.path().to_string_lossy().as_ref()));
    assert_eq!(identidad(&journal), esperada);
    assert_eq!(fs::read(&journal).unwrap(), bytes);
    assert_eq!(fs::read(&temporal).unwrap(), b"replacement temp");
}

#[cfg(windows)]
#[test]
fn reintento_despues_de_rename_ve_canonico_esperado() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = lock.journal_temporal_anclado().unwrap();
    let temporal = nombre_temporal(temp.path());
    let journal = nombre_journal(&root);
    let bytes = b"committed before crash";
    let esperada = capability.crear_nuevo(bytes).unwrap();
    fs::rename(&temporal, &journal).unwrap();

    assert!(confirmado(capability.publicar(esperada)));
    assert_eq!(identidad(&journal), esperada);
    assert_eq!(fs::read(journal).unwrap(), bytes);
    assert!(!temporal.exists());
}

#[cfg(windows)]
#[test]
fn junction_canonica_en_conflicto_conserva_temporal_y_canary() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let temporal = nombre_temporal(temp.path());
    let journal = nombre_journal(&root);
    let outside = temp.path().join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("canary"), b"outside canary").unwrap();
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = lock.journal_temporal_anclado().unwrap();
    let bytes_temporal = b"temporary canary";
    let esperada = capability.crear_nuevo(bytes_temporal).unwrap();
    let output = std::process::Command::new("cmd")
        .args(["/c", "mklink", "/J"])
        .arg(&journal)
        .arg(&outside)
        .output()
        .expect("cmd crea junction");
    if !output.status.success() {
        let detalle = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        if detalle.to_ascii_lowercase().contains("privilege") {
            eprintln!("SKIP explícito: Windows no concedió privilegio para la junction");
            return;
        }
        panic!("no se pudo crear junction: {detalle}");
    }

    assert_eq!(
        capability.publicar(esperada),
        ResultadoPublicacionJournal::NoAplicado(ErrorJournalAnclado::ReparseNotAllowed)
    );
    assert_eq!(fs::read(temporal).unwrap(), bytes_temporal);
    assert_eq!(fs::read(outside.join("canary")).unwrap(), b"outside canary");
}

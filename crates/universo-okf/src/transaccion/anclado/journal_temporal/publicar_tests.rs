use super::super::{ErrorJournalAnclado, IdentidadArchivoJournal};
use super::super::{JournalTemporalAnclado, ResultadoPublicacionJournal};
use crate::transaccion::journal::selector::JournalPersistidoError;
use crate::transaccion::journal::v2::JournalV2;
use crate::transaccion::journal::{
    ArtifactFingerprint, DirectoryIdentity, Journal, JournalState, RawTreeDigest,
};
use crate::transaccion::lock::LockEscritura;
use crate::transaccion::{anclado::journal_temporal::identidad_archivo, raiz::UbicacionRaiz};
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use tempfile::tempdir;

#[path = "successor_observation_support.rs"]
pub(super) mod successor_observation_support;

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

fn capability_con_scope_parent_real(lock: &mut LockEscritura) -> JournalTemporalAnclado<'_> {
    let mut capability = lock.journal_temporal_anclado().unwrap();
    capability.scope_parent = Some(super::super::scope_filesystem(&capability.parent).unwrap());
    capability
}

fn confirmado(resultado: ResultadoPublicacionJournal) -> bool {
    matches!(resultado, ResultadoPublicacionJournal::Confirmado { .. })
}

fn journal_prueba(capability: &JournalTemporalAnclado<'_>, transaction_id: &str) -> Journal {
    let parent = capability.identidad_parent.journal_identity().unwrap();
    let directory = |id: u64| {
        #[cfg(unix)]
        {
            DirectoryIdentity::unix(7, id)
        }
        #[cfg(windows)]
        {
            DirectoryIdentity::windows(7, format!("{id:032x}"))
        }
    };
    let fingerprint = |id: u64, byte: char| {
        ArtifactFingerprint::new(
            directory(id),
            RawTreeDigest::from_hex(byte.to_string().repeat(64)).unwrap(),
        )
    };
    Journal::new(
        transaction_id,
        parent,
        fingerprint(2, 'a'),
        fingerprint(3, 'b'),
    )
    .unwrap()
}

fn preparar_temporal(
    capability: &JournalTemporalAnclado<'_>,
) -> (Journal, Vec<u8>, IdentidadArchivoJournal) {
    let journal = journal_prueba(capability, "0123456789abcdef0123456789abcdef");
    let bytes = journal.encode().unwrap();
    let identidad = capability.crear_nuevo(&bytes).unwrap();
    (journal, bytes, identidad)
}

fn journal_v2_prueba(capability: &JournalTemporalAnclado<'_>, transaction_id: &str) -> JournalV2 {
    let parent = capability.identidad_parent.journal_identity().unwrap();
    let directory = |id: u64| {
        #[cfg(unix)]
        {
            DirectoryIdentity::unix(7, id)
        }
        #[cfg(windows)]
        {
            DirectoryIdentity::windows(7, format!("{id:032x}"))
        }
    };
    let fingerprint = |id: u64, byte: char| {
        ArtifactFingerprint::new(
            directory(id),
            RawTreeDigest::from_hex(byte.to_string().repeat(64)).unwrap(),
        )
    };
    JournalV2::new(
        transaction_id,
        parent,
        fingerprint(4, 'c'),
        fingerprint(5, 'd'),
    )
    .unwrap()
}

#[cfg(unix)]
#[test]
fn mapper_unix_acepta_eisdir_y_enotdir_como_alternativas() {
    for codigo in [libc::EISDIR, libc::ENOTDIR] {
        assert_eq!(
            super::mapear_error_publicacion_unix(std::io::Error::from_raw_os_error(codigo)),
            ErrorJournalAnclado::RegularFileRequired
        );
    }
}

#[test]
fn publicar_preserva_bytes_e_identidad_y_quita_temporal() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability: JournalTemporalAnclado<'_> = lock.journal_temporal_anclado().unwrap();
    let (journal_esperado, bytes, esperada) = preparar_temporal(&capability);
    let journal = nombre_journal(&root);

    assert!(confirmado(capability.publicar(esperada, &journal_esperado)));
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
    let (journal_esperado, bytes, _) = preparar_temporal(&capability);
    let otro = temp.path().join("other");
    fs::write(&otro, b"other identity").unwrap();

    let mut syscall_invocado = false;
    assert_eq!(
        capability.publicar_con(identidad(&otro), &journal_esperado, |_, _| {
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
    let (journal_esperado, bytes_temporal, esperada) = preparar_temporal(&capability);
    let bytes_canonicos = b"canonical canary";
    fs::write(&journal, bytes_canonicos).unwrap();

    assert_eq!(
        capability.publicar(esperada, &journal_esperado),
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
    let (journal_esperado, bytes_temporal, esperada) = preparar_temporal(&capability);
    fs::create_dir(&journal).unwrap();
    fs::write(journal.join("canary"), b"directory canary").unwrap();

    assert_eq!(
        capability.publicar(esperada, &journal_esperado),
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
    let journal_esperado = journal_prueba(&capability, "0123456789abcdef0123456789abcdef");

    assert_eq!(
        capability.publicar(identidad(&otro), &journal_esperado),
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
    let (journal_esperado, bytes_temporales, esperada) = preparar_temporal(&capability);
    let bytes_canonicos = b"canonical canary";
    fs::write(&journal, bytes_canonicos).unwrap();
    let identidad_canonica = identidad(&journal);
    capability.raiz_remota = super::raiz_es_remota(unc);

    assert_eq!(
        capability.publicar(esperada, &journal_esperado),
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
    let (journal_esperado, bytes, esperada) = preparar_temporal(&capability);
    fs::hard_link(&temporal, &journal).unwrap();

    assert!(confirmado(capability.publicar(esperada, &journal_esperado)));
    assert_eq!(identidad(&journal), esperada);
    assert_eq!(fs::read(journal).unwrap(), bytes);
    assert!(!temporal.exists());
}

#[cfg(any(unix, windows))]
#[test]
fn reintento_con_misma_identidad_y_journal_distinto_preserva_el_canary() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = lock.journal_temporal_anclado().unwrap();
    let temporal = nombre_temporal(temp.path());
    let journal_path = nombre_journal(&root);
    let (journal_esperado, _, esperada) = preparar_temporal(&capability);
    fs::rename(&temporal, &journal_path).unwrap();

    let journal_conflictivo = journal_prueba(&capability, "1123456789abcdef0123456789abcdef");
    let bytes_conflictivos = journal_conflictivo.encode().unwrap();
    fs::write(&journal_path, &bytes_conflictivos).unwrap();
    assert_eq!(identidad(&journal_path), esperada);

    assert_eq!(
        capability.publicar(esperada, &journal_esperado),
        ResultadoPublicacionJournal::PrecondicionFallida(ErrorJournalAnclado::JournalMismatch)
    );
    assert_eq!(identidad(&journal_path), esperada);
    assert_eq!(fs::read(&journal_path).unwrap(), bytes_conflictivos);
    assert!(!temporal.exists());
}

#[cfg(any(unix, windows))]
#[test]
fn reconciliacion_con_misma_identidad_y_journal_distinto_no_confirma_ni_limpia() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = lock.journal_temporal_anclado().unwrap();
    let temporal = nombre_temporal(temp.path());
    let journal_path = nombre_journal(&root);
    let (journal_esperado, _, esperada) = preparar_temporal(&capability);
    fs::hard_link(&temporal, &journal_path).unwrap();

    let journal_conflictivo = journal_prueba(&capability, "1123456789abcdef0123456789abcdef");
    let bytes_conflictivos = journal_conflictivo.encode().unwrap();
    fs::write(&journal_path, &bytes_conflictivos).unwrap();
    assert_eq!(identidad(&journal_path), esperada);
    assert_eq!(identidad(&temporal), esperada);

    let resultado =
        capability.resolver_error_publicacion(esperada, &journal_esperado, ErrorJournalAnclado::Io);

    assert!(matches!(
        resultado,
        ResultadoPublicacionJournal::Indeterminado { .. }
    ));
    assert_eq!(identidad(&journal_path), esperada);
    assert_eq!(identidad(&temporal), esperada);
    assert_eq!(fs::read(&journal_path).unwrap(), bytes_conflictivos);
    assert_eq!(fs::read(&temporal).unwrap(), bytes_conflictivos);
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
    let (journal_esperado, bytes, esperada) = preparar_temporal(&capability);
    let fuente = super::abrir_temporal(&capability.parent, &capability.nombre).unwrap();

    super::publicar_plataforma(
        &capability.parent,
        &capability.nombre,
        &capability.nombre_journal,
        &fuente,
    )
    .unwrap();
    let mut cleanup_llamado = false;
    let resultado = capability.resultado_despues_de_commit(esperada, &journal_esperado, |_, _| {
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
        let (journal_esperado, bytes, esperada) = preparar_temporal(&capability);
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
            &journal_esperado,
            |_, _, observado| {
                assert_eq!(observado, &journal_esperado);
                observacion
            },
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
    let (journal_esperado, bytes, esperada) = preparar_temporal(&capability);
    let fuente = super::abrir_temporal(&capability.parent, &capability.nombre).unwrap();

    super::publicar_plataforma(
        &capability.parent,
        &capability.nombre,
        &capability.nombre_journal,
        &fuente,
    )
    .unwrap();
    let resultado =
        capability.resolver_error_publicacion(esperada, &journal_esperado, ErrorJournalAnclado::Io);

    assert!(confirmado(resultado));
    assert_eq!(identidad(&journal), esperada);
    assert_eq!(fs::read(journal).unwrap(), bytes);
    assert!(!temporal.exists());
}

#[cfg(any(unix, windows))]
#[test]
fn publicacion_exitosa_con_journal_canonico_distinto_no_limpia_evidencia() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = lock.journal_temporal_anclado().unwrap();
    let temporal = nombre_temporal(temp.path());
    let journal_path = nombre_journal(&root);
    let (journal_esperado, _, esperada) = preparar_temporal(&capability);
    let journal_conflictivo = journal_prueba(&capability, "1123456789abcdef0123456789abcdef");
    let bytes_conflictivos = journal_conflictivo.encode().unwrap();
    let mut syscall_reporto_exito = false;

    let resultado = capability.publicar_con(esperada, &journal_esperado, |_, _| {
        fs::hard_link(&temporal, &journal_path).map_err(|_| ErrorJournalAnclado::Io)?;
        fs::write(&journal_path, &bytes_conflictivos).map_err(|_| ErrorJournalAnclado::Io)?;
        syscall_reporto_exito = true;
        Ok(())
    });

    assert!(syscall_reporto_exito);
    assert_eq!(
        resultado,
        ResultadoPublicacionJournal::Indeterminado {
            causa: super::super::CausaIndeterminacionPublicacionJournal::CanonicoNoVerificable(
                ErrorJournalAnclado::JournalMismatch
            ),
        }
    );
    assert_eq!(identidad(&temporal), esperada);
    assert_eq!(identidad(&journal_path), esperada);
    assert_eq!(fs::read(&temporal).unwrap(), bytes_conflictivos);
    assert_eq!(fs::read(&journal_path).unwrap(), bytes_conflictivos);
    assert_eq!(
        Journal::parse(&bytes_conflictivos).unwrap(),
        journal_conflictivo
    );
}

#[test]
fn error_de_publicacion_con_observaciones_inciertas_preserva_evidencia() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = lock.journal_temporal_anclado().unwrap();
    let temporal = nombre_temporal(temp.path());
    let journal = nombre_journal(&root);
    let (journal_esperado, bytes, esperada) = preparar_temporal(&capability);
    let mut canonico_observado = false;
    let mut temporal_observado = false;
    let mut limpieza_llamada = false;

    let resultado = capability.resolver_error_publicacion_observando(
        esperada,
        &journal_esperado,
        ErrorJournalAnclado::Io,
        |_, _, observado| {
            assert_eq!(observado, &journal_esperado);
            canonico_observado = true;
            Err(ErrorJournalAnclado::Io)
        },
        |_, _, observado| {
            assert_eq!(observado, &journal_esperado);
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
        let (journal_esperado, bytes, esperada) = preparar_temporal(&capability);
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

        let resultado = capability.resultado_despues_de_commit(
            esperada,
            &journal_esperado,
            |capability, identidad| capability.limpiar_temporal_si_coincide(identidad),
        );
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
    let (journal_esperado, bytes, esperada) = preparar_temporal(&capability);
    assert_eq!(
        capability.resolver_error_publicacion(esperada, &journal_esperado, error),
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
    let (journal_esperado, bytes, esperada) = preparar_temporal(&capability);
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

    let outcome = capability.resolver_error_publicacion(esperada, &journal_esperado, error);
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
    let (journal_esperado, bytes, esperada) = preparar_temporal(&capability);
    fs::rename(&temporal, &journal).unwrap();

    assert!(confirmado(capability.publicar(esperada, &journal_esperado)));
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
    let (journal_esperado, bytes_temporal, esperada) = preparar_temporal(&capability);
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
        capability.publicar(esperada, &journal_esperado),
        ResultadoPublicacionJournal::NoAplicado(ErrorJournalAnclado::ReparseNotAllowed)
    );
    assert_eq!(fs::read(temporal).unwrap(), bytes_temporal);
    assert_eq!(fs::read(outside.join("canary")).unwrap(), b"outside canary");
}

#[test]
fn lector_temporal_v2_retiene_handle_y_verifica_parent_y_bytes() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = capability_con_scope_parent_real(&mut lock);
    let temporal = nombre_temporal(temp.path());
    let journal = journal_v2_prueba(&capability, "0123456789abcdef0123456789abcdef");
    let bytes = journal.encode().unwrap();
    let identidad_esperada = capability.crear_nuevo(&bytes).unwrap();

    let observacion = capability.observar_temporal_versionado(&journal).unwrap();

    assert_eq!(observacion.journal(), &journal);
    assert_eq!(observacion.identidad(), identidad_esperada);
    assert_eq!(
        format!("{observacion:?}"),
        "ObservacionTemporalVersionada([redacted])"
    );
    drop(observacion);
    assert_eq!(identidad(&temporal), identidad_esperada);
    assert_eq!(fs::read(&temporal).unwrap(), bytes);
}

#[test]
fn lector_temporal_v2_compara_el_registro_completo() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = capability_con_scope_parent_real(&mut lock);
    let temporal = nombre_temporal(temp.path());
    let journal_guardado = journal_v2_prueba(&capability, "0123456789abcdef0123456789abcdef");
    let journal_esperado = journal_v2_prueba(&capability, "1123456789abcdef0123456789abcdef");
    let bytes = journal_guardado.encode().unwrap();
    let identidad_esperada = capability.crear_nuevo(&bytes).unwrap();

    assert_eq!(
        capability
            .observar_temporal_versionado(&journal_esperado)
            .unwrap_err(),
        super::super::versionado::ErrorLecturaJournalVersionada::Lease(
            ErrorJournalAnclado::JournalMismatch
        )
    );
    assert_eq!(identidad(&temporal), identidad_esperada);
    assert_eq!(fs::read(&temporal).unwrap(), bytes);
}

#[test]
fn lector_temporal_v2_rechaza_journal_v1_y_bytes_malformados() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = capability_con_scope_parent_real(&mut lock);
    let temporal = nombre_temporal(temp.path());
    let journal_esperado = journal_v2_prueba(&capability, "0123456789abcdef0123456789abcdef");
    let (_, bytes_v1, _) = preparar_temporal(&capability);
    assert_eq!(
        capability
            .observar_temporal_versionado(&journal_esperado)
            .unwrap_err(),
        super::super::versionado::ErrorLecturaJournalVersionada::Lease(
            ErrorJournalAnclado::JournalMismatch
        )
    );
    assert_eq!(fs::read(&temporal).unwrap(), bytes_v1);
    drop(capability);
    drop(lock);

    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = capability_con_scope_parent_real(&mut lock);
    let bytes_malformados = b"{ invalid selector";
    let journal_esperado = journal_v2_prueba(&capability, "0123456789abcdef0123456789abcdef");
    capability.crear_nuevo(bytes_malformados).unwrap();

    assert_eq!(
        capability
            .observar_temporal_versionado(&journal_esperado)
            .unwrap_err(),
        super::super::versionado::ErrorLecturaJournalVersionada::Selector(
            JournalPersistidoError::Malformed
        )
    );
    assert_eq!(
        fs::read(nombre_temporal(temp.path())).unwrap(),
        bytes_malformados
    );
}

#[test]
fn lector_temporal_v2_falla_cerrado_sin_scope_parent() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let mut capability = lock.journal_temporal_anclado().unwrap();
    let temporal = nombre_temporal(temp.path());
    let journal = journal_v2_prueba(&capability, "0123456789abcdef0123456789abcdef");
    let bytes = journal.encode().unwrap();
    capability.crear_nuevo(&bytes).unwrap();
    capability.scope_parent = None;

    assert_eq!(
        capability
            .observar_temporal_versionado(&journal)
            .unwrap_err(),
        super::super::versionado::ErrorLecturaJournalVersionada::Lease(
            ErrorJournalAnclado::ParentScopeUnverified
        )
    );
    assert_eq!(fs::read(temporal).unwrap(), bytes);
}

#[test]
fn lector_temporal_v2_rechaza_reemplazo_de_mismos_bytes_con_otro_id() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = capability_con_scope_parent_real(&mut lock);
    let temporal = nombre_temporal(temp.path());
    let desplazado = temp.path().join("temporal-original");
    let journal = journal_v2_prueba(&capability, "0123456789abcdef0123456789abcdef");
    let bytes = journal.encode().unwrap();
    let identidad_original = capability.crear_nuevo(&bytes).unwrap();

    let resultado = capability.observar_temporal_versionado_con_prueba(&journal, |etapa| {
        if etapa == super::super::versionado::EtapaLecturaJournalVersionada::AntesDeSondaVisible {
            fs::rename(&temporal, &desplazado).unwrap();
            fs::write(&temporal, &bytes).unwrap();
        }
    });

    assert_eq!(
        resultado.unwrap_err(),
        super::super::versionado::ErrorLecturaJournalVersionada::Lease(
            ErrorJournalAnclado::IdentityMismatch
        )
    );
    let identidad_reemplazo = identidad(&temporal);
    assert_ne!(identidad_reemplazo, identidad_original);
    assert_eq!(fs::read(&temporal).unwrap(), bytes);
    assert_eq!(fs::read(&desplazado).unwrap(), bytes);
}

#[test]
fn publicacion_v2_preparada_conserva_el_registro_y_limpia_su_identidad() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = capability_con_scope_parent_real(&mut lock);
    let temporal = nombre_temporal(temp.path());
    let canonico = nombre_journal(&root);
    let journal = journal_v2_prueba(&capability, "0123456789abcdef0123456789abcdef");
    let bytes = journal.encode().unwrap();
    let esperada = capability.crear_nuevo(&bytes).unwrap();

    let resultado = capability.publicar_v2(esperada, &journal);

    assert_eq!(
        resultado,
        ResultadoPublicacionJournal::Confirmado {
            limpieza: super::super::LimpiezaConfirmadaJournal::Completa,
        }
    );
    assert_eq!(
        JournalV2::parse(&fs::read(canonico).unwrap()).unwrap(),
        journal
    );
    assert_eq!(identidad(&nombre_journal(&root)), esperada);
    assert!(!temporal.exists());
    assert_eq!(
        capability.publicar_v2(esperada, &journal),
        ResultadoPublicacionJournal::Confirmado {
            limpieza: super::super::LimpiezaConfirmadaJournal::Completa,
        }
    );
    assert!(!temporal.exists());
}

#[test]
fn publicacion_v2_rechaza_scope_ausente_fase_y_registros_distintos() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let mut capability = capability_con_scope_parent_real(&mut lock);
    let temporal = nombre_temporal(temp.path());
    let canonico = nombre_journal(&root);
    let journal = journal_v2_prueba(&capability, "0123456789abcdef0123456789abcdef");
    let bytes = journal.encode().unwrap();
    let esperada = capability.crear_nuevo(&bytes).unwrap();

    capability.scope_parent = None;
    assert_eq!(
        capability.publicar_v2(esperada, &journal),
        ResultadoPublicacionJournal::PrecondicionFallida(
            ErrorJournalAnclado::ParentScopeUnverified
        )
    );
    assert_eq!(fs::read(&temporal).unwrap(), bytes);
    capability.scope_parent = Some(super::super::scope_filesystem(&capability.parent).unwrap());

    #[cfg(unix)]
    let parent_foraneo = DirectoryIdentity::unix(7, 99);
    #[cfg(windows)]
    let parent_foraneo = DirectoryIdentity::windows(7, format!("{:032x}", 99));
    let journal_parent_foraneo = JournalV2::new(
        "2123456789abcdef0123456789abcdef",
        parent_foraneo,
        journal.old().clone(),
        journal.new_fingerprint().clone(),
    )
    .unwrap();
    assert_eq!(
        capability.publicar_v2(esperada, &journal_parent_foraneo),
        ResultadoPublicacionJournal::PrecondicionFallida(
            ErrorJournalAnclado::ParentIdentityMismatch
        )
    );

    let mut fase_invalida = journal.clone();
    fase_invalida.advance(JournalState::BackupReady).unwrap();
    assert_eq!(
        capability.publicar_v2(esperada, &fase_invalida),
        ResultadoPublicacionJournal::PrecondicionFallida(ErrorJournalAnclado::JournalMismatch)
    );
    let registro_distinto = journal_v2_prueba(&capability, "1123456789abcdef0123456789abcdef");
    let decoy = temp.path().join("temporal-decoy");
    fs::write(&decoy, &bytes).unwrap();
    let otra_identidad = identidad(&decoy);
    assert_ne!(otra_identidad, esperada);
    assert_eq!(
        capability.publicar_v2(otra_identidad, &registro_distinto),
        ResultadoPublicacionJournal::PrecondicionFallida(ErrorJournalAnclado::IdentityMismatch)
    );
    assert_eq!(
        capability.publicar_v2(esperada, &registro_distinto),
        ResultadoPublicacionJournal::PrecondicionFallida(ErrorJournalAnclado::JournalMismatch)
    );
    assert!(!canonico.exists());
    assert_eq!(identidad(&temporal), esperada);
    assert_eq!(fs::read(&temporal).unwrap(), bytes);
}

#[test]
fn publicacion_v2_no_interpreta_canonico_v1_como_confirmacion() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = capability_con_scope_parent_real(&mut lock);
    let temporal = nombre_temporal(temp.path());
    let canonico = nombre_journal(&root);
    let temporal_viejo = temp.path().join("temporal-v1-original");
    let (_, bytes_v1, _) = preparar_temporal(&capability);
    fs::rename(&temporal, &temporal_viejo).unwrap();

    let journal_v2 = journal_v2_prueba(&capability, "0123456789abcdef0123456789abcdef");
    let bytes_v2 = journal_v2.encode().unwrap();
    let esperada = capability.crear_nuevo(&bytes_v2).unwrap();
    fs::write(&canonico, &bytes_v1).unwrap();
    let identidad_canonica = identidad(&canonico);

    assert_eq!(
        capability.publicar_v2(esperada, &journal_v2),
        ResultadoPublicacionJournal::NoAplicado(ErrorJournalAnclado::CanonicalConflict)
    );
    assert_eq!(identidad(&canonico), identidad_canonica);
    assert_eq!(fs::read(&canonico).unwrap(), bytes_v1);
    assert_eq!(identidad(&temporal), esperada);
    assert_eq!(fs::read(&temporal).unwrap(), bytes_v2);
    assert_eq!(fs::read(temporal_viejo).unwrap(), bytes_v1);
}

#[test]
fn error_de_syscall_v2_antes_del_efecto_preserva_el_temporal() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = capability_con_scope_parent_real(&mut lock);
    let temporal = nombre_temporal(temp.path());
    let canonico = nombre_journal(&root);
    let journal = journal_v2_prueba(&capability, "0123456789abcdef0123456789abcdef");
    let bytes = journal.encode().unwrap();
    let esperada = capability.crear_nuevo(&bytes).unwrap();

    let resultado = capability.publicar_v2_con(
        esperada,
        &journal,
        |_: &super::super::JournalTemporalAnclado<'_>, _: &File| Err(ErrorJournalAnclado::Io),
    );

    assert_eq!(
        resultado,
        ResultadoPublicacionJournal::NoAplicado(ErrorJournalAnclado::Io)
    );
    assert!(!canonico.exists());
    assert_eq!(identidad(&temporal), esperada);
    assert_eq!(fs::read(&temporal).unwrap(), bytes);
}

#[test]
fn error_de_syscall_v2_despues_del_efecto_confirma_y_limpia_solo_el_temporal() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = capability_con_scope_parent_real(&mut lock);
    let temporal = nombre_temporal(temp.path());
    let canonico = nombre_journal(&root);
    let journal = journal_v2_prueba(&capability, "0123456789abcdef0123456789abcdef");
    let bytes = journal.encode().unwrap();
    let esperada = capability.crear_nuevo(&bytes).unwrap();

    let resultado = capability.publicar_v2_con(
        esperada,
        &journal,
        |capability: &super::super::JournalTemporalAnclado<'_>, fuente: &File| {
            super::publicar_plataforma(
                &capability.parent,
                &capability.nombre,
                &capability.nombre_journal,
                fuente,
            )?;
            Err(ErrorJournalAnclado::Io)
        },
    );

    assert_eq!(
        resultado,
        ResultadoPublicacionJournal::Confirmado {
            limpieza: super::super::LimpiezaConfirmadaJournal::Completa,
        }
    );
    assert_eq!(identidad(&canonico), esperada);
    assert_eq!(fs::read(canonico).unwrap(), bytes);
    assert!(!temporal.exists());
}

#[test]
fn error_de_syscall_v2_con_canonico_inverificable_queda_indeterminado() {
    let temp = tempdir().unwrap();
    let root = crear_root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = capability_con_scope_parent_real(&mut lock);
    let temporal = nombre_temporal(temp.path());
    let canonico = nombre_journal(&root);
    let bytes_canonicos = b"invalid synthetic canonical record";
    let journal = journal_v2_prueba(&capability, "0123456789abcdef0123456789abcdef");
    let bytes = journal.encode().unwrap();
    let esperada = capability.crear_nuevo(&bytes).unwrap();

    let resultado = capability.publicar_v2_con(
        esperada,
        &journal,
        |_capability: &super::super::JournalTemporalAnclado<'_>, _fuente: &File| {
            fs::write(&canonico, bytes_canonicos).unwrap();
            Err(ErrorJournalAnclado::Io)
        },
    );

    assert!(matches!(
        resultado,
        ResultadoPublicacionJournal::Indeterminado { .. }
    ));
    assert_eq!(fs::read(canonico).unwrap(), bytes_canonicos.to_vec());
    assert_eq!(identidad(&temporal), esperada);
    assert_eq!(fs::read(&temporal).unwrap(), bytes);
}

use super::*;

#[test]
fn error_sin_efecto_con_anterior_y_temporal_exactos_es_no_aplicado() {
    let temp = tempdir().unwrap();
    let root = root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = lock.journal_temporal_anclado().unwrap();
    let fixture = setup(&capability, &root, temp.path());

    assert_eq!(
        capability.reemplazar_siguiente_con(
            fixture.old_id,
            fixture.temp_id,
            &fixture.candidate,
            |_, _| Err(ErrorJournalAnclado::Io)
        ),
        ResultadoPublicacionJournal::NoAplicado(ErrorJournalAnclado::Io)
    );
    assert_eq!(file_id(&fixture.canonical), fixture.old_id);
    assert_eq!(file_id(&fixture.temporary), fixture.temp_id);
    assert_eq!(
        fs::read(&fixture.canonical).unwrap(),
        fixture.old.encode().unwrap()
    );
    assert_eq!(
        fs::read(&fixture.temporary).unwrap(),
        fixture.candidate.encode().unwrap()
    );
}

#[test]
fn exito_sin_efecto_o_error_con_observacion_incierta_es_indeterminado() {
    let temp = tempdir().unwrap();
    let root = root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = lock.journal_temporal_anclado().unwrap();
    let fixture = setup(&capability, &root, temp.path());
    assert!(matches!(
        capability.reemplazar_siguiente_con(
            fixture.old_id,
            fixture.temp_id,
            &fixture.candidate,
            |_, _| Ok(())
        ),
        ResultadoPublicacionJournal::Indeterminado { .. }
    ));

    let moved_old = temp.path().join("old-canary");
    let result = capability.reemplazar_siguiente_con(
        fixture.old_id,
        fixture.temp_id,
        &fixture.candidate,
        |_, _| {
            fs::rename(&fixture.canonical, &moved_old).unwrap();
            fs::create_dir(&fixture.canonical).unwrap();
            fs::write(fixture.canonical.join("canary"), b"conflict canary").unwrap();
            Err(ErrorJournalAnclado::Io)
        },
    );
    assert!(matches!(
        result,
        ResultadoPublicacionJournal::Indeterminado {
            causa: CausaIndeterminacionPublicacionJournal::ReemplazoNoDemostrable {
                canonico: EstadoObservacionReemplazoJournal::NoVerificable(_),
                ..
            }
        }
    ));
    assert_eq!(fs::read(moved_old).unwrap(), fixture.old.encode().unwrap());
    assert_eq!(
        fs::read(fixture.canonical.join("canary")).unwrap(),
        b"conflict canary"
    );
    assert_eq!(
        fs::read(&fixture.temporary).unwrap(),
        fixture.candidate.encode().unwrap()
    );
}

#[test]
fn reintento_sin_temporal_confirma_solo_el_mismo_registro_candidato() {
    let temp = tempdir().unwrap();
    let root = root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = lock.journal_temporal_anclado().unwrap();
    let fixture = setup(&capability, &root, temp.path());
    let source =
        super::super::super::publicar::abrir_temporal(&capability.parent, &capability.nombre)
            .unwrap();
    super::super::super::publicar::reemplazar_plataforma(
        &capability.parent,
        &capability.nombre,
        &capability.nombre_journal,
        &source,
    )
    .unwrap();
    drop(source);

    assert_eq!(
        capability.reemplazar_siguiente(fixture.old_id, fixture.temp_id, &fixture.candidate),
        ResultadoPublicacionJournal::Confirmado {
            limpieza: LimpiezaConfirmadaJournal::Completa
        }
    );
}

#[test]
fn gate_remoto_no_invoca_syscall_y_preserva_canaries() {
    let temp = tempdir().unwrap();
    let root = root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let mut capability = lock.journal_temporal_anclado().unwrap();
    let fixture = setup(&capability, &root, temp.path());
    capability.raiz_remota = true;
    let called = std::cell::Cell::new(false);

    assert_eq!(
        capability.reemplazar_siguiente_con(
            fixture.old_id,
            fixture.temp_id,
            &fixture.candidate,
            |_, _| {
                called.set(true);
                Ok(())
            }
        ),
        ResultadoPublicacionJournal::PrecondicionFallida(ErrorJournalAnclado::RemoteUnsupported)
    );
    assert!(!called.get());
    assert_eq!(
        fs::read(&fixture.canonical).unwrap(),
        fixture.old.encode().unwrap()
    );
    assert_eq!(
        fs::read(&fixture.temporary).unwrap(),
        fixture.candidate.encode().unwrap()
    );
}

#[test]
fn resultado_redacta_identidades_rutas_y_registro() {
    let temp = tempdir().unwrap();
    let root = root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = lock.journal_temporal_anclado().unwrap();
    let fixture = setup(&capability, &root, temp.path());
    let result =
        capability.reemplazar_siguiente(fixture.temp_id, fixture.temp_id, &fixture.candidate);
    let rendered = format!("{result:?}");

    assert!(!rendered.contains(fixture.candidate.transaction_id()));
    assert!(!rendered.contains(temp.path().to_string_lossy().as_ref()));
    assert!(!rendered.contains("0123456789abcdef"));
}

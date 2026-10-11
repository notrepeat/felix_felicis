use super::*;

#[cfg(windows)]
#[test]
fn temporal_replacement_handle_requests_only_attributes_and_delete_access() {
    use windows_sys::Win32::Storage::FileSystem::{DELETE, FILE_READ_ATTRIBUTES, FILE_READ_DATA};

    let access = super::super::super::publicar::acceso_temporal_publicacion();
    assert_eq!(access, FILE_READ_ATTRIBUTES | DELETE);
    assert_eq!(access & FILE_READ_DATA, 0);
}

#[cfg(unix)]
#[test]
fn symlink_canonico_en_conflicto_preserva_destino_y_temporal() {
    use std::os::unix::fs::symlink;
    let temp = tempdir().unwrap();
    let root = root(temp.path());
    let outside = temp.path().join("outside");
    fs::write(&outside, b"outside canary").unwrap();
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = lock.journal_temporal_anclado().unwrap();
    let fixture = setup(&capability, &root, temp.path());
    fs::remove_file(&fixture.canonical).unwrap();
    symlink(&outside, &fixture.canonical).unwrap();

    assert_eq!(
        capability.reemplazar_siguiente(fixture.old_id, fixture.temp_id, &fixture.candidate),
        ResultadoPublicacionJournal::PrecondicionFallida(ErrorJournalAnclado::ReparseNotAllowed)
    );
    assert_eq!(fs::read(&outside).unwrap(), b"outside canary");
    assert_eq!(
        fs::read(&fixture.temporary).unwrap(),
        fixture.candidate.encode().unwrap()
    );
}

#[cfg(windows)]
#[test]
fn junction_canonica_en_conflicto_preserva_destino_y_temporal() {
    let temp = tempdir().unwrap();
    let root = root(temp.path());
    let outside = temp.path().join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("canary"), b"outside canary").unwrap();
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = lock.journal_temporal_anclado().unwrap();
    let fixture = setup(&capability, &root, temp.path());
    fs::remove_file(&fixture.canonical).unwrap();
    let output = std::process::Command::new("cmd")
        .args(["/c", "mklink", "/J"])
        .arg(&fixture.canonical)
        .arg(&outside)
        .output()
        .unwrap();
    if !output.status.success() {
        let detail = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        if detail.to_ascii_lowercase().contains("privilege") {
            eprintln!("SKIP explícito: Windows no concedió privilegio para la junction");
            return;
        }
        panic!("no se pudo crear junction: {detail}");
    }

    assert_eq!(
        capability.reemplazar_siguiente(fixture.old_id, fixture.temp_id, &fixture.candidate),
        ResultadoPublicacionJournal::PrecondicionFallida(ErrorJournalAnclado::ReparseNotAllowed)
    );
    assert_eq!(fs::read(outside.join("canary")).unwrap(), b"outside canary");
    assert_eq!(
        fs::read(&fixture.temporary).unwrap(),
        fixture.candidate.encode().unwrap()
    );
}

#[test]
fn error_despues_del_efecto_se_confirma_por_registro_e_identidad() {
    let temp = tempdir().unwrap();
    let root = root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = lock.journal_temporal_anclado().unwrap();
    let fixture = setup(&capability, &root, temp.path());
    let result = capability.reemplazar_siguiente_con(
        fixture.old_id,
        fixture.temp_id,
        &fixture.candidate,
        |cap, source| {
            super::super::super::publicar::reemplazar_plataforma(
                &cap.parent,
                &cap.nombre,
                &cap.nombre_journal,
                source,
            )?;
            Err(ErrorJournalAnclado::Io)
        },
    );

    assert_eq!(
        result,
        ResultadoPublicacionJournal::Confirmado {
            limpieza: LimpiezaConfirmadaJournal::Completa
        }
    );
    assert_eq!(file_id(&fixture.canonical), fixture.temp_id);
    assert_eq!(
        fs::read(&fixture.canonical).unwrap(),
        fixture.candidate.encode().unwrap()
    );
    assert!(!fixture.temporary.exists());
}

#[cfg(unix)]
#[test]
fn temporal_residual_hardlink_al_canonico_se_limpia_sin_perder_candidato() {
    let temp = tempdir().unwrap();
    let root = root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = lock.journal_temporal_anclado().unwrap();
    let fixture = setup(&capability, &root, temp.path());
    let result = capability.reemplazar_siguiente_con(
        fixture.old_id,
        fixture.temp_id,
        &fixture.candidate,
        |cap, source| {
            super::super::super::publicar::reemplazar_plataforma(
                &cap.parent,
                &cap.nombre,
                &cap.nombre_journal,
                source,
            )?;
            fs::hard_link(&fixture.canonical, &fixture.temporary).unwrap();
            Err(ErrorJournalAnclado::Io)
        },
    );

    assert_eq!(
        result,
        ResultadoPublicacionJournal::Confirmado {
            limpieza: LimpiezaConfirmadaJournal::Completa
        }
    );
    assert_eq!(file_id(&fixture.canonical), fixture.temp_id);
    assert_eq!(
        fs::read(&fixture.canonical).unwrap(),
        fixture.candidate.encode().unwrap()
    );
    assert!(!fixture.temporary.exists());
}

#[cfg(windows)]
#[test]
fn target_con_handle_sin_share_delete_devuelve_no_aplicado_y_preserva_canaries() {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::{FILE_SHARE_READ, FILE_SHARE_WRITE};
    let temp = tempdir().unwrap();
    let root = root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = lock.journal_temporal_anclado().unwrap();
    let fixture = setup(&capability, &root, temp.path());
    let blocker = fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .open(&fixture.canonical)
        .unwrap();

    assert_eq!(
        capability.reemplazar_siguiente(fixture.old_id, fixture.temp_id, &fixture.candidate),
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
    drop(blocker);
}

#[cfg(windows)]
#[test]
#[allow(clippy::permissions_set_readonly_false)]
fn destino_readonly_no_se_modifica_implicitamente() {
    let temp = tempdir().unwrap();
    let root = root(temp.path());
    let mut lock = LockEscritura::adquirir(&root).unwrap();
    let capability = lock.journal_temporal_anclado().unwrap();
    let fixture = setup(&capability, &root, temp.path());
    let mut permissions = fs::metadata(&fixture.canonical).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&fixture.canonical, permissions).unwrap();

    let result =
        capability.reemplazar_siguiente(fixture.old_id, fixture.temp_id, &fixture.candidate);
    match result {
        ResultadoPublicacionJournal::Confirmado { .. } => {
            assert_eq!(file_id(&fixture.canonical), fixture.temp_id);
            assert_eq!(
                fs::read(&fixture.canonical).unwrap(),
                fixture.candidate.encode().unwrap()
            );
        }
        ResultadoPublicacionJournal::NoAplicado(ErrorJournalAnclado::Io) => {
            assert!(
                fs::metadata(&fixture.canonical)
                    .unwrap()
                    .permissions()
                    .readonly()
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
            let mut permissions = fs::metadata(&fixture.canonical).unwrap().permissions();
            permissions.set_readonly(false);
            fs::set_permissions(&fixture.canonical, permissions).unwrap();
        }
        other => panic!("resultado inesperado para destino readonly: {other:?}"),
    }
}

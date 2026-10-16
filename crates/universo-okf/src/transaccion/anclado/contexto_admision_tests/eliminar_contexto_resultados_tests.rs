use super::*;
use std::cell::Cell;

#[test]
fn ausencia_previa_al_callback_devuelve_ausente_sin_llamarlo() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let (ubicacion, _) = ubicacion(&parent, true);
    let bytes = frame("0123456789abcdef0123456789abcdef", parent_identity(&parent));
    let canonical = contexto_path(&ubicacion);
    let retained = parent.join("context-retained");
    fs::write(&canonical, &bytes).unwrap();
    let id_original = identidad_archivo(&File::open(&canonical).unwrap()).unwrap();
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner =
        super::super::super::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(&mut lock)
            .unwrap();
    let lease = owner.contexto_admision().unwrap();
    let (_, evidence) = lease.leer_canonico_con_evidencia().unwrap().unwrap();
    fs::rename(&canonical, &retained).unwrap();
    let callbacks = Cell::new(0);

    let result = lease.eliminar_canonico_con(&evidence, |_parent, _name, _file| {
        callbacks.set(callbacks.get() + 1);
        Ok(())
    });

    assert_eq!(result, ResultadoEliminacionContextoAnclado::Ausente);
    assert_eq!(callbacks.get(), 0);
    assert!(!canonical.exists());
    assert_eq!(fs::read(&retained).unwrap(), bytes);
    assert_eq!(
        identidad_archivo(&File::open(&retained).unwrap()).unwrap(),
        id_original
    );
}

#[test]
fn callback_error_con_canonico_igual_es_indeterminado_una_vez() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let (ubicacion, _) = ubicacion(&parent, true);
    let bytes = frame("0123456789abcdef0123456789abcdef", parent_identity(&parent));
    let canonical = contexto_path(&ubicacion);
    fs::write(&canonical, &bytes).unwrap();
    let id_original = identidad_archivo(&File::open(&canonical).unwrap()).unwrap();
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner =
        super::super::super::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(&mut lock)
            .unwrap();
    let lease = owner.contexto_admision().unwrap();
    let (_, evidence) = lease.leer_canonico_con_evidencia().unwrap().unwrap();
    let callbacks = Cell::new(0);

    let result = lease.eliminar_canonico_con(&evidence, |_parent, _name, _file| {
        callbacks.set(callbacks.get() + 1);
        Err(ErrorContextoAdmisionAnclado::Io)
    });

    let detail = format!("{result:?}");
    assert!(detail.starts_with("Indeterminado("));
    assert!(detail.contains("admission_context_removal_unverified"));
    assert_eq!(callbacks.get(), 1);
    assert_eq!(fs::read(&canonical).unwrap(), bytes);
    assert_eq!(
        identidad_archivo(&File::open(&canonical).unwrap()).unwrap(),
        id_original
    );
}

#[test]
fn callback_exitoso_con_canonico_aun_presente_no_confirma_retiro() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let (ubicacion, _) = ubicacion(&parent, true);
    let bytes = frame("0123456789abcdef0123456789abcdef", parent_identity(&parent));
    let canonical = contexto_path(&ubicacion);
    fs::write(&canonical, &bytes).unwrap();
    let id_original = identidad_archivo(&File::open(&canonical).unwrap()).unwrap();
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner =
        super::super::super::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(&mut lock)
            .unwrap();
    let lease = owner.contexto_admision().unwrap();
    let (_, evidence) = lease.leer_canonico_con_evidencia().unwrap().unwrap();
    let callbacks = Cell::new(0);

    let result = lease.eliminar_canonico_con(&evidence, |_parent, _name, _file| {
        callbacks.set(callbacks.get() + 1);
        Ok(())
    });

    let detail = format!("{result:?}");
    assert!(detail.starts_with("Indeterminado("));
    assert!(detail.contains("admission_context_removal_unverified"));
    assert_eq!(callbacks.get(), 1);
    assert_eq!(fs::read(&canonical).unwrap(), bytes);
    assert_eq!(
        identidad_archivo(&File::open(&canonical).unwrap()).unwrap(),
        id_original
    );
}

#[test]
fn callback_error_con_nombre_ausente_devuelve_solo_ausencia_observada() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let (ubicacion, _) = ubicacion(&parent, true);
    let bytes = frame("0123456789abcdef0123456789abcdef", parent_identity(&parent));
    let canonical = contexto_path(&ubicacion);
    fs::write(&canonical, &bytes).unwrap();
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner =
        super::super::super::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(&mut lock)
            .unwrap();
    let lease = owner.contexto_admision().unwrap();
    let (_, evidence) = lease.leer_canonico_con_evidencia().unwrap().unwrap();
    let callbacks = Cell::new(0);

    let result = lease.eliminar_canonico_con(&evidence, |_parent, _name, _file| {
        callbacks.set(callbacks.get() + 1);
        fs::remove_file(&canonical).unwrap();
        Err(ErrorContextoAdmisionAnclado::Io)
    });

    assert_eq!(format!("{result:?}"), "Ausente");
    assert_eq!(callbacks.get(), 1);
    assert!(!canonical.exists());
}

#[test]
fn callback_que_cambia_contenido_in_place_preserva_el_reemplazo_y_devuelve_conflicto() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let (ubicacion, _) = ubicacion(&parent, true);
    let parent_id = parent_identity(&parent);
    let original = frame("0123456789abcdef0123456789abcdef", parent_id.clone());
    let changed = frame("1123456789abcdef0123456789abcdef", parent_id);
    let canonical = contexto_path(&ubicacion);
    fs::write(&canonical, &original).unwrap();
    let id_original = identidad_archivo(&File::open(&canonical).unwrap()).unwrap();
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner =
        super::super::super::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(&mut lock)
            .unwrap();
    let lease = owner.contexto_admision().unwrap();
    let (_, evidence) = lease.leer_canonico_con_evidencia().unwrap().unwrap();
    let callbacks = Cell::new(0);

    let result = lease.eliminar_canonico_con(&evidence, |_parent, _name, _file| {
        callbacks.set(callbacks.get() + 1);
        fs::write(&canonical, &changed).unwrap();
        Err(ErrorContextoAdmisionAnclado::Io)
    });

    let detail = format!("{result:?}");
    assert!(detail.starts_with("Conflicto("));
    assert!(detail.contains("admission_context_canonical_unverified"));
    assert_eq!(callbacks.get(), 1);
    assert_eq!(fs::read(&canonical).unwrap(), changed);
    assert_eq!(
        identidad_archivo(&File::open(&canonical).unwrap()).unwrap(),
        id_original
    );
}

#[test]
fn callback_que_reemplaza_nombre_con_mismo_frame_conflicto_preserva_ambos_ids() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let (ubicacion, _) = ubicacion(&parent, true);
    let original = frame("0123456789abcdef0123456789abcdef", parent_identity(&parent));
    let canonical = contexto_path(&ubicacion);
    let retained = parent.join("context-retained-after-callback");
    fs::write(&canonical, &original).unwrap();
    let id_original = identidad_archivo(&File::open(&canonical).unwrap()).unwrap();
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner =
        super::super::super::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(&mut lock)
            .unwrap();
    let lease = owner.contexto_admision().unwrap();
    let (_, evidence) = lease.leer_canonico_con_evidencia().unwrap().unwrap();
    let callbacks = Cell::new(0);

    let result = lease.eliminar_canonico_con(&evidence, |_parent, _name, _file| {
        callbacks.set(callbacks.get() + 1);
        fs::rename(&canonical, &retained).unwrap();
        fs::write(&canonical, &original).unwrap();
        Err(ErrorContextoAdmisionAnclado::Io)
    });

    assert_eq!(
        result,
        ResultadoEliminacionContextoAnclado::Conflicto(
            ErrorContextoAdmisionAnclado::CanonicalUnverified
        )
    );
    assert_eq!(callbacks.get(), 1);
    assert_eq!(fs::read(&canonical).unwrap(), original);
    assert_eq!(fs::read(&retained).unwrap(), original);
    let id_replacement = identidad_archivo(&File::open(&canonical).unwrap()).unwrap();
    assert_ne!(id_replacement, id_original);
    assert_eq!(
        identidad_archivo(&File::open(&retained).unwrap()).unwrap(),
        id_original
    );
}

#[cfg(unix)]
#[test]
fn lock_inverificable_despues_del_callback_deja_estado_indeterminado() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let (ubicacion, _) = ubicacion(&parent, true);
    let bytes = frame("0123456789abcdef0123456789abcdef", parent_identity(&parent));
    let canonical = contexto_path(&ubicacion);
    fs::write(&canonical, &bytes).unwrap();
    let id_context = identidad_archivo(&File::open(&canonical).unwrap()).unwrap();
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner =
        super::super::super::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(&mut lock)
            .unwrap();
    let lease = owner.contexto_admision().unwrap();
    let (_, evidence) = lease.leer_canonico_con_evidencia().unwrap().unwrap();
    let lock_route = ubicacion.lock();
    let lock_path = lock_route.as_path().to_path_buf();
    let displaced_lock = parent.join("lock-displaced");
    let decoy = b"post-callback lock decoy";
    let callbacks = Cell::new(0);

    let result = lease.eliminar_canonico_con(&evidence, |_parent, _name, _file| {
        callbacks.set(callbacks.get() + 1);
        fs::rename(&lock_path, &displaced_lock).unwrap();
        fs::write(&lock_path, decoy).unwrap();
        Err(ErrorContextoAdmisionAnclado::Io)
    });

    let detail = format!("{result:?}");
    assert!(detail.starts_with("Indeterminado("));
    assert!(detail.contains("admission_context_owner_unverified"));
    assert_eq!(callbacks.get(), 1);
    assert_eq!(fs::read(&canonical).unwrap(), bytes);
    assert_eq!(
        identidad_archivo(&File::open(&canonical).unwrap()).unwrap(),
        id_context
    );
    assert_eq!(fs::read(&lock_path).unwrap().as_slice(), decoy);
    assert!(displaced_lock.is_file());
}

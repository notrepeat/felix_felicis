use super::*;
use std::cell::Cell;

#[test]
fn elimina_contexto_canonico_y_confirma_ausencia_real() {
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

    let result = lease.eliminar_canonico(&evidence);

    assert_eq!(result, ResultadoEliminacionContextoAnclado::Eliminado);
    assert!(!canonical.exists());
    assert_eq!(
        fs::read(&canonical).unwrap_err().kind(),
        std::io::ErrorKind::NotFound
    );
}

#[test]
fn bytes_distintos_con_mismo_id_bloquean_el_callback_y_se_preservan() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let (ubicacion, _) = ubicacion(&parent, true);
    let parent_id = parent_identity(&parent);
    let original = frame("0123456789abcdef0123456789abcdef", parent_id.clone());
    let changed = frame("1123456789abcdef0123456789abcdef", parent_id);
    assert_eq!(original.len(), changed.len());
    let canonical = contexto_path(&ubicacion);
    fs::write(&canonical, &original).unwrap();
    let id_original = identidad_archivo(&File::open(&canonical).unwrap()).unwrap();
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner =
        super::super::super::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(&mut lock)
            .unwrap();
    let lease = owner.contexto_admision().unwrap();
    let (_, evidence) = lease.leer_canonico_con_evidencia().unwrap().unwrap();
    fs::write(&canonical, &changed).unwrap();
    assert_eq!(
        identidad_archivo(&File::open(&canonical).unwrap()).unwrap(),
        id_original
    );
    let callbacks = Cell::new(0);

    let result = lease.eliminar_canonico_con(&evidence, |_parent, _name, _file| {
        callbacks.set(callbacks.get() + 1);
        Ok(())
    });

    assert_eq!(
        result,
        ResultadoEliminacionContextoAnclado::Conflicto(
            ErrorContextoAdmisionAnclado::CanonicalUnverified
        )
    );
    assert_eq!(callbacks.get(), 0);
    assert_eq!(fs::read(&canonical).unwrap(), changed);
    assert_eq!(
        identidad_archivo(&File::open(&canonical).unwrap()).unwrap(),
        id_original
    );
}

#[test]
fn mismo_frame_con_id_nuevo_bloquea_el_callback_y_preserva_ambos_archivos() {
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
    fs::write(&canonical, &bytes).unwrap();
    let id_replacement = identidad_archivo(&File::open(&canonical).unwrap()).unwrap();
    assert_ne!(id_original, id_replacement);
    let callbacks = Cell::new(0);

    let result = lease.eliminar_canonico_con(&evidence, |_parent, _name, _file| {
        callbacks.set(callbacks.get() + 1);
        Ok(())
    });

    assert_eq!(
        result,
        ResultadoEliminacionContextoAnclado::Conflicto(
            ErrorContextoAdmisionAnclado::CanonicalUnverified
        )
    );
    assert_eq!(callbacks.get(), 0);
    assert_eq!(fs::read(&canonical).unwrap(), bytes);
    assert_eq!(fs::read(&retained).unwrap(), bytes);
    assert_eq!(
        identidad_archivo(&File::open(&canonical).unwrap()).unwrap(),
        id_replacement
    );
    assert_eq!(
        identidad_archivo(&File::open(&retained).unwrap()).unwrap(),
        id_original
    );
}

#[cfg(unix)]
#[test]
fn lock_visible_sustituido_es_conflicto_antes_de_invocar_el_borrado() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let (ubicacion, _) = ubicacion(&parent, true);
    let bytes = frame("0123456789abcdef0123456789abcdef", parent_identity(&parent));
    let canonical = contexto_path(&ubicacion);
    fs::write(&canonical, &bytes).unwrap();
    let id_canonical = identidad_archivo(&File::open(&canonical).unwrap()).unwrap();
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner =
        super::super::super::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(&mut lock)
            .unwrap();
    let lease = owner.contexto_admision().unwrap();
    let (_, evidence) = lease.leer_canonico_con_evidencia().unwrap().unwrap();
    let ruta_lock = ubicacion.lock();
    let lock_path = ruta_lock.as_path().to_path_buf();
    let retained_lock = parent.join("lock-retained");
    let decoy_bytes = b"replacement lock";
    let id_original_lock = identidad_archivo(&File::open(&lock_path).unwrap()).unwrap();
    fs::rename(&lock_path, &retained_lock).unwrap();
    fs::write(&lock_path, decoy_bytes).unwrap();
    let id_decoy = identidad_archivo(&File::open(&lock_path).unwrap()).unwrap();
    let callbacks = Cell::new(0);

    let result = lease.eliminar_canonico_con(&evidence, |_parent, _name, _file| {
        callbacks.set(callbacks.get() + 1);
        Ok(())
    });

    assert_eq!(
        result,
        ResultadoEliminacionContextoAnclado::Conflicto(
            ErrorContextoAdmisionAnclado::OwnerUnverified
        )
    );
    assert_eq!(callbacks.get(), 0);
    assert_eq!(fs::read(&canonical).unwrap(), bytes);
    assert_eq!(
        identidad_archivo(&File::open(&canonical).unwrap()).unwrap(),
        id_canonical
    );
    assert_eq!(fs::read(&lock_path).unwrap().as_slice(), decoy_bytes);
    assert_eq!(
        identidad_archivo(&File::open(&lock_path).unwrap()).unwrap(),
        id_decoy
    );
    assert_eq!(
        identidad_archivo(&File::open(&retained_lock).unwrap()).unwrap(),
        id_original_lock
    );
}

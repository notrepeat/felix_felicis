use super::*;

#[test]
fn reabre_contexto_canonico_y_contrasta_el_mismo_handle() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let (ubicacion, _) = ubicacion(&parent, true);
    let esperado = frame("0123456789abcdef0123456789abcdef", parent_identity(&parent));
    let canonico = contexto_path(&ubicacion);
    fs::write(&canonico, &esperado).unwrap();
    let id_esperado = identidad_archivo(&File::open(&canonico).unwrap()).unwrap();
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner =
        super::super::super::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(&mut lock)
            .unwrap();
    let lease = owner.contexto_admision().unwrap();
    let (_, evidencia) = lease.leer_canonico_con_evidencia().unwrap().unwrap();

    let archivo = lease.verificar_canonico_con_evidencia(&evidencia).unwrap();

    assert_eq!(identidad_archivo(&archivo).unwrap(), id_esperado);
    assert_eq!(archivo.metadata().unwrap().len(), evidencia.longitud);
    assert_eq!(fs::read(&canonico).unwrap(), esperado);
}

#[test]
fn rechaza_bytes_distintos_con_el_mismo_id_y_preserva_el_canonico() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let (ubicacion, _) = ubicacion(&parent, true);
    let identidad_parent = parent_identity(&parent);
    let primero = frame("0123456789abcdef0123456789abcdef", identidad_parent.clone());
    let segundo = frame("1123456789abcdef0123456789abcdef", identidad_parent);
    assert_eq!(primero.len(), segundo.len());
    let canonico = contexto_path(&ubicacion);
    fs::write(&canonico, &primero).unwrap();
    let id_original = identidad_archivo(&File::open(&canonico).unwrap()).unwrap();
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner =
        super::super::super::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(&mut lock)
            .unwrap();
    let lease = owner.contexto_admision().unwrap();
    let (_, evidencia) = lease.leer_canonico_con_evidencia().unwrap().unwrap();

    fs::write(&canonico, &segundo).unwrap();
    let id_mutado = identidad_archivo(&File::open(&canonico).unwrap()).unwrap();
    let error = lease
        .verificar_canonico_con_evidencia(&evidencia)
        .unwrap_err();

    assert_eq!(id_mutado, id_original);
    assert_eq!(error, ErrorContextoAdmisionAnclado::CanonicalUnverified);
    assert_eq!(error.codigo(), "admission_context_canonical_unverified");
    assert_eq!(fs::read(&canonico).unwrap(), segundo);
    assert_eq!(
        identidad_archivo(&File::open(&canonico).unwrap()).unwrap(),
        id_original
    );
}

#[test]
fn rechaza_reemplazo_de_mismo_frame_con_id_distinto_y_preserva_ambos_archivos() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("parent");
    let (ubicacion, _) = ubicacion(&parent, true);
    let esperado = frame("0123456789abcdef0123456789abcdef", parent_identity(&parent));
    let canonico = contexto_path(&ubicacion);
    let retenido = parent.join("context-retained");
    fs::write(&canonico, &esperado).unwrap();
    let id_canonico = identidad_archivo(&File::open(&canonico).unwrap()).unwrap();
    let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
    let mut owner =
        super::super::super::duenio_transaccion::DuenioTransaccionAnclado::desde_lock(&mut lock)
            .unwrap();
    let lease = owner.contexto_admision().unwrap();
    let (_, evidencia) = lease.leer_canonico_con_evidencia().unwrap().unwrap();
    fs::rename(&canonico, &retenido).unwrap();
    fs::write(&canonico, &esperado).unwrap();
    let id_reemplazo = identidad_archivo(&File::open(&canonico).unwrap()).unwrap();
    assert_ne!(id_canonico, id_reemplazo);

    let error = lease
        .verificar_canonico_con_evidencia(&evidencia)
        .unwrap_err();

    assert_eq!(error, ErrorContextoAdmisionAnclado::CanonicalUnverified);
    assert_eq!(fs::read(&canonico).unwrap(), esperado);
    assert_eq!(fs::read(&retenido).unwrap(), esperado);
    assert_eq!(
        identidad_archivo(&File::open(&canonico).unwrap()).unwrap(),
        id_reemplazo
    );
    assert_eq!(
        identidad_archivo(&File::open(&retenido).unwrap()).unwrap(),
        id_canonico
    );
}

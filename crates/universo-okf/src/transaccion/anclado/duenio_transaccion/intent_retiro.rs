//! Lease privado de lectura para volver a comprobar un intent canónico.
//!
//! Conserva el owner y el handle observado, pero no concede autoridad de
//! replay, limpieza o aplicación.

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::fs::File;
use std::io::Read;
use std::sync::Arc;

use crate::transaccion::anclado::intent_persistido::{IntentRetiroPersistido, MAX_INTENT_BYTES};
use crate::transaccion::anclado::{
    ErrorDirectorioAnclado, IdentidadArchivo, IdentidadDirectorio, ScopeFilesystem,
    TipoEntradaAnclada, validar_componente,
};

use super::observar_siblings_nuevo_flujo::nombre_intent_retiro;
use super::{DuenioTransaccionAnclado, ErrorDuenioTransaccionAnclado};

/// Datos observados y archivo final bajo el owner retenido; nunca es prueba.
#[must_use = "el lease conserva el owner mientras se usa"]
pub(super) struct LeaseIntentRetiro<'owner, 'lock> {
    _owner: &'owner mut DuenioTransaccionAnclado<'lock>,
    _autoridad: Arc<()>,
    _identidad_parent: IdentidadDirectorio,
    _scope_parent: ScopeFilesystem,
    _identidad_intent: IdentidadArchivo,
    _registro: IntentRetiroPersistido,
    _archivo: File,
}

impl fmt::Debug for LeaseIntentRetiro<'_, '_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("LeaseIntentRetiro { .. }")
    }
}

/// Reabre el nombre interno y compara los datos observados con el mismo owner.
/// La lectura es puntual y no congela bytes ni namespace frente a terceros.
pub(super) fn abrir_lease_retiro<'owner, 'lock>(
    owner: &'owner mut DuenioTransaccionAnclado<'lock>,
    autoridad_observada: Arc<()>,
    identidad_parent_observada: IdentidadDirectorio,
    scope_parent_observado: ScopeFilesystem,
    identidad_intent_observada: IdentidadArchivo,
    registro_observado: IntentRetiroPersistido,
    archivo_observado: File,
) -> Result<Option<LeaseIntentRetiro<'owner, 'lock>>, ErrorDuenioTransaccionAnclado> {
    validar_vinculo_owner(
        owner,
        &autoridad_observada,
        identidad_parent_observada,
        scope_parent_observado,
    )?;

    let nombres = NombresIntentRetiro::desde_owner(owner)?;
    let parent = owner
        .parent_revalidado()
        .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
    validar_temporales(owner.scope_parent, &parent, &nombres)?;

    let Some(archivo_sonda) = abrir_sonda(&parent, &nombres.canonico)? else {
        validar_vinculo_owner(
            owner,
            &autoridad_observada,
            identidad_parent_observada,
            scope_parent_observado,
        )?;
        let parent_final = owner
            .parent_revalidado()
            .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
        validar_temporales(owner.scope_parent, &parent_final, &nombres)?;
        drop(archivo_observado);
        return Ok(None);
    };

    #[cfg(windows)]
    let archivo_final = {
        let archivo = abrir_final_con_delete(&parent, &nombres.canonico)?;
        drop(archivo_sonda);
        archivo
    };

    #[cfg(not(windows))]
    let archivo_final = archivo_sonda;

    let (identidad_final, scope_final) =
        super::lector_intent_canonico::validar_archivo_intent(&archivo_final, owner.scope_parent)?;
    if identidad_final != identidad_intent_observada {
        return Err(ErrorDuenioTransaccionAnclado::NoVerificable);
    }

    let metadata = archivo_final
        .metadata()
        .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
    if metadata.len()
        > u64::try_from(MAX_INTENT_BYTES)
            .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?
    {
        return Err(ErrorDuenioTransaccionAnclado::NoVerificable);
    }
    let limite = u64::try_from(MAX_INTENT_BYTES)
        .ok()
        .and_then(|maximo| maximo.checked_add(1))
        .ok_or(ErrorDuenioTransaccionAnclado::NoVerificable)?;
    let mut bytes = Vec::new();
    (&archivo_final)
        .take(limite)
        .read_to_end(&mut bytes)
        .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
    if bytes.len() > MAX_INTENT_BYTES {
        return Err(ErrorDuenioTransaccionAnclado::NoVerificable);
    }

    let registro = IntentRetiroPersistido::decode(&bytes)
        .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
    registro
        .validar_vinculo_archivo_intent(identidad_final, scope_final)
        .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
    let identidad_parent_journal = identidad_parent_observada
        .journal_identity()
        .ok_or(ErrorDuenioTransaccionAnclado::NoVerificable)?;
    if registro.journal().parent() != &identidad_parent_journal || registro != registro_observado {
        return Err(ErrorDuenioTransaccionAnclado::NoVerificable);
    }

    validar_vinculo_owner(
        owner,
        &autoridad_observada,
        identidad_parent_observada,
        scope_parent_observado,
    )?;
    let parent_final = owner
        .parent_revalidado()
        .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
    validar_temporales(owner.scope_parent, &parent_final, &nombres)?;

    let autoridad_final = Arc::clone(&owner._autoridad);
    drop(archivo_observado);
    Ok(Some(LeaseIntentRetiro {
        _owner: owner,
        _autoridad: autoridad_final,
        _identidad_parent: identidad_parent_observada,
        _scope_parent: scope_parent_observado,
        _identidad_intent: identidad_final,
        _registro: registro,
        _archivo: archivo_final,
    }))
}

fn validar_vinculo_owner(
    owner: &DuenioTransaccionAnclado<'_>,
    autoridad_observada: &Arc<()>,
    identidad_parent_observada: IdentidadDirectorio,
    scope_parent_observado: ScopeFilesystem,
) -> Result<(), ErrorDuenioTransaccionAnclado> {
    if !Arc::ptr_eq(autoridad_observada, &owner._autoridad)
        || identidad_parent_observada != owner.identidad_parent
        || scope_parent_observado != owner.scope_parent
    {
        return Err(ErrorDuenioTransaccionAnclado::NoVerificable);
    }
    Ok(())
}

fn validar_temporales(
    scope_parent: ScopeFilesystem,
    parent: &File,
    nombres: &NombresIntentRetiro,
) -> Result<(), ErrorDuenioTransaccionAnclado> {
    super::lector_intent_canonico::validar_temporales_ausentes(
        parent,
        scope_parent,
        nombres.temporales(),
    )
}

fn abrir_sonda(
    parent: &File,
    nombre: &OsStr,
) -> Result<Option<File>, ErrorDuenioTransaccionAnclado> {
    let archivo = match super::super::abrir::abrir_entrada_relativa(
        parent,
        nombre,
        TipoEntradaAnclada::Archivo,
    ) {
        Ok(archivo) => archivo,
        Err(ErrorDirectorioAnclado::EntradaAusente) => return Ok(None),
        Err(_) => return Err(ErrorDuenioTransaccionAnclado::NoVerificable),
    };
    Ok(Some(archivo))
}

#[cfg(windows)]
fn abrir_final_con_delete(
    parent: &File,
    nombre: &OsStr,
) -> Result<File, ErrorDuenioTransaccionAnclado> {
    use fs_at::OpenOptions as OpenOptionsAt;
    use fs_at::os::windows::OpenOptionsExt;
    use std::path::Path;
    use windows_sys::Win32::Storage::FileSystem::{DELETE, FILE_READ_ATTRIBUTES, FILE_READ_DATA};

    let mut opciones = OpenOptionsAt::default();
    opciones
        .desired_access(FILE_READ_DATA | FILE_READ_ATTRIBUTES | DELETE)
        .follow(false);
    opciones
        .open_at(parent, Path::new(nombre))
        .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)
}

struct NombresIntentRetiro {
    canonico: OsString,
    temporales: [OsString; 3],
}

impl NombresIntentRetiro {
    fn desde_owner(
        owner: &DuenioTransaccionAnclado<'_>,
    ) -> Result<Self, ErrorDuenioTransaccionAnclado> {
        let ubicacion = owner._lock.ubicacion();
        let basename = ubicacion
            .ruta()
            .file_name()
            .ok_or(ErrorDuenioTransaccionAnclado::NoVerificable)?;
        validar_componente(basename).map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;

        let canonico = nombre_intent_retiro(basename)?;
        let temporal_intent = super::super::contexto_admision::nombre_temporal_sibling(&canonico)
            .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
        let temporal_journal = ubicacion.nombre_journal_temporal();
        validar_componente(&temporal_journal)
            .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
        let nombre_contexto = super::super::contexto_admision::nombre_canonico(basename)
            .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
        let temporal_contexto =
            super::super::contexto_admision::nombre_temporal_sibling(&nombre_contexto)
                .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;

        Ok(Self {
            canonico,
            temporales: [temporal_intent, temporal_journal, temporal_contexto],
        })
    }

    fn temporales(&self) -> [&OsStr; 3] {
        [
            self.temporales[0].as_os_str(),
            self.temporales[1].as_os_str(),
            self.temporales[2].as_os_str(),
        ]
    }
}

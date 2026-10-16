//! Lease privado de lectura para volver a comprobar un intent canónico.
//!
//! Conserva el owner y el handle observado, pero no concede autoridad de
//! replay, limpieza o aplicación.

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::fs::File;
use std::sync::Arc;

use crate::transaccion::anclado::intent_persistido::IntentRetiroPersistido;
use crate::transaccion::anclado::{
    IdentidadArchivo, IdentidadDirectorio, ScopeFilesystem, validar_componente,
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

    let Some(archivo_sonda) =
        super::lector_intent_canonico::abrir_intent_canonico(&parent, &nombres.canonico)
            .map_err(super::lector_intent_canonico::mapear_error_lectura_intent)?
    else {
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

    let leido = super::lector_intent_canonico::leer_intent_canonico(
        archivo_final,
        owner.scope_parent,
        identidad_parent_observada,
        Some((identidad_intent_observada, &registro_observado)),
        super::super::scope_filesystem,
    )
    .map_err(super::lector_intent_canonico::mapear_error_lectura_intent)?;
    let identidad_final = leido.identidad;
    let _scope_final = leido.scope;
    let registro = leido.registro;
    let archivo_final = leido.archivo;

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

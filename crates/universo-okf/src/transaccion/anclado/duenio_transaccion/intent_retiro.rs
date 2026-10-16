//! Lease privado para validar y, en una única tentativa, retirar un intent canónico.
//!
//! Conserva owner y handle observado, pero no concede autoridad de replay,
//! recovery o aplicación.

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

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum EstadoRetiroIntent {
    Eliminado,
    Ausente,
    Conflicto,
    Indeterminado,
}

/// Resultado owner-bound, sin handle de archivo ni detalles sensibles.
#[must_use = "el resultado conserva el owner para una continuación privada"]
pub(super) struct ResultadoRetiroIntent<'owner, 'lock> {
    _owner: &'owner mut DuenioTransaccionAnclado<'lock>,
    _autoridad: Arc<()>,
    _identidad_parent: IdentidadDirectorio,
    _scope_parent: ScopeFilesystem,
    _registro: IntentRetiroPersistido,
    estado: EstadoRetiroIntent,
}

impl fmt::Debug for ResultadoRetiroIntent<'_, '_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ResultadoRetiroIntent { .. }")
    }
}

impl ResultadoRetiroIntent<'_, '_> {
    pub(super) fn estado(&self) -> EstadoRetiroIntent {
        self.estado
    }

    #[cfg(test)]
    pub(super) fn owner_binding_coincide_para_test(&self) -> bool {
        Arc::ptr_eq(&self._autoridad, &self._owner._autoridad)
            && self._identidad_parent == self._owner.identidad_parent
            && self._scope_parent == self._owner.scope_parent
            && self
                ._identidad_parent
                .journal_identity()
                .is_some_and(|parent| self._registro.journal().parent() == &parent)
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

impl<'owner, 'lock> LeaseIntentRetiro<'owner, 'lock> {
    /// Intenta una sola retirada y conserva owner, autoridad y registro como datos.
    #[allow(dead_code)]
    pub(super) fn retirar(self) -> ResultadoRetiroIntent<'owner, 'lock> {
        let LeaseIntentRetiro {
            _owner,
            _autoridad,
            _identidad_parent,
            _scope_parent,
            _identidad_intent,
            _registro,
            _archivo,
        } = self;
        let estado = intentar_retirar_intent(
            _owner,
            &_autoridad,
            _identidad_parent,
            _scope_parent,
            _identidad_intent,
            &_registro,
            _archivo,
        );
        resultado_retiro(
            _owner,
            _autoridad,
            _identidad_parent,
            _scope_parent,
            _registro,
            estado,
        )
    }
}

fn intentar_retirar_intent(
    owner: &mut DuenioTransaccionAnclado<'_>,
    autoridad: &Arc<()>,
    identidad_parent: IdentidadDirectorio,
    scope_parent: ScopeFilesystem,
    identidad_intent: IdentidadArchivo,
    registro: &IntentRetiroPersistido,
    archivo_anterior: File,
) -> EstadoRetiroIntent {
    // Igual que Context/Journal, solo rechaza raíces remotas reconocidas lexicalmente.
    if super::super::journal_temporal::raiz_es_remota(owner._lock.raiz_canonica()) {
        return EstadoRetiroIntent::Conflicto;
    }
    let nombres = match NombresIntentRetiro::desde_owner(owner) {
        Ok(nombres) => nombres,
        Err(_) => return EstadoRetiroIntent::Conflicto,
    };
    let parent = match validar_owner_y_temporales(
        owner,
        autoridad,
        identidad_parent,
        scope_parent,
        &nombres,
    ) {
        Ok(parent) => parent,
        Err(_) => return EstadoRetiroIntent::Conflicto,
    };
    let fuente = match abrir_y_leer_fuente(
        &parent,
        &nombres.canonico,
        scope_parent,
        identidad_parent,
        identidad_intent,
        registro,
    ) {
        Ok(Some(fuente)) => fuente,
        Ok(None) => return EstadoRetiroIntent::Conflicto,
        Err(error) => {
            return match error {
                super::lector_intent_canonico::ErrorLecturaIntent::Io => {
                    EstadoRetiroIntent::Indeterminado
                }
                super::lector_intent_canonico::ErrorLecturaIntent::Discrepancia => {
                    EstadoRetiroIntent::Conflicto
                }
            };
        }
    };
    drop(parent);

    let parent_accion = match validar_owner_y_temporales(
        owner,
        autoridad,
        identidad_parent,
        scope_parent,
        &nombres,
    ) {
        Ok(parent) => parent,
        Err(_) => {
            drop(fuente.archivo);
            return EstadoRetiroIntent::Conflicto;
        }
    };

    drop(archivo_anterior);
    let intento = retirar_nombre_plataforma(&parent_accion, &nombres.canonico, fuente.archivo);
    drop(parent_accion);
    let contexto = ContextoConfirmacionRetiro {
        autoridad,
        identidad_parent,
        scope_parent,
        nombres: &nombres,
        identidad_intent,
        registro_esperado: registro,
    };
    confirmar_retiro(owner, contexto, intento)
}

fn resultado_retiro<'owner, 'lock>(
    owner: &'owner mut DuenioTransaccionAnclado<'lock>,
    autoridad: Arc<()>,
    identidad_parent: IdentidadDirectorio,
    scope_parent: ScopeFilesystem,
    registro: IntentRetiroPersistido,
    estado: EstadoRetiroIntent,
) -> ResultadoRetiroIntent<'owner, 'lock> {
    ResultadoRetiroIntent {
        _owner: owner,
        _autoridad: autoridad,
        _identidad_parent: identidad_parent,
        _scope_parent: scope_parent,
        _registro: registro,
        estado,
    }
}

fn validar_owner_y_temporales(
    owner: &mut DuenioTransaccionAnclado<'_>,
    autoridad: &Arc<()>,
    identidad_parent: IdentidadDirectorio,
    scope_parent: ScopeFilesystem,
    nombres: &NombresIntentRetiro,
) -> Result<File, ErrorDuenioTransaccionAnclado> {
    validar_vinculo_owner(owner, autoridad, identidad_parent, scope_parent)?;
    let parent = owner
        .parent_revalidado()
        .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
    validar_temporales(scope_parent, &parent, nombres)?;
    Ok(parent)
}

fn abrir_y_leer_fuente(
    parent: &File,
    nombre: &OsStr,
    scope_parent: ScopeFilesystem,
    identidad_parent: IdentidadDirectorio,
    identidad_intent: IdentidadArchivo,
    registro_esperado: &IntentRetiroPersistido,
) -> Result<
    Option<super::lector_intent_canonico::IntentoCanonicoLeido>,
    super::lector_intent_canonico::ErrorLecturaIntent,
> {
    let Some(sonda) = super::lector_intent_canonico::abrir_intent_canonico(parent, nombre)? else {
        return Ok(None);
    };
    #[cfg(windows)]
    let archivo = {
        drop(sonda);
        abrir_final_con_delete(parent, nombre)
            .map_err(|_| super::lector_intent_canonico::ErrorLecturaIntent::Io)?
    };
    #[cfg(not(windows))]
    let archivo = sonda;

    super::lector_intent_canonico::leer_intent_canonico(
        archivo,
        scope_parent,
        identidad_parent,
        Some((identidad_intent, registro_esperado)),
        super::super::scope_filesystem,
    )
    .map(Some)
}

struct ContextoConfirmacionRetiro<'a> {
    autoridad: &'a Arc<()>,
    identidad_parent: IdentidadDirectorio,
    scope_parent: ScopeFilesystem,
    nombres: &'a NombresIntentRetiro,
    identidad_intent: IdentidadArchivo,
    registro_esperado: &'a IntentRetiroPersistido,
}

fn confirmar_retiro(
    owner: &mut DuenioTransaccionAnclado<'_>,
    contexto: ContextoConfirmacionRetiro<'_>,
    intento: Result<(), ()>,
) -> EstadoRetiroIntent {
    let ContextoConfirmacionRetiro {
        autoridad,
        identidad_parent,
        scope_parent,
        nombres,
        identidad_intent,
        registro_esperado,
    } = contexto;
    let owner_preconfirmacion_valido =
        validar_owner_y_temporales(owner, autoridad, identidad_parent, scope_parent, nombres)
            .is_ok();
    // Siempre se observa el nombre bajo el parent retenido, aunque el owner
    // falle: sin ambos chequeos el resultado sigue siendo indeterminado.
    let confirmacion = match owner.parent.try_clone() {
        Ok(parent) => {
            let confirmacion = abrir_y_leer_fuente(
                &parent,
                &nombres.canonico,
                scope_parent,
                identidad_parent,
                identidad_intent,
                registro_esperado,
            )
            .map(|resultado| match resultado {
                Some(leido) => {
                    drop(leido.archivo);
                    true
                }
                None => false,
            });
            drop(parent);
            confirmacion
        }
        Err(_) => Err(super::lector_intent_canonico::ErrorLecturaIntent::Io),
    };
    let owner_postconfirmacion_valido =
        validar_owner_y_temporales(owner, autoridad, identidad_parent, scope_parent, nombres)
            .is_ok();
    if !owner_preconfirmacion_valido || !owner_postconfirmacion_valido {
        return EstadoRetiroIntent::Indeterminado;
    }

    match confirmacion {
        Ok(false) if intento.is_ok() => EstadoRetiroIntent::Eliminado,
        Ok(false) => EstadoRetiroIntent::Ausente,
        Ok(true) | Err(super::lector_intent_canonico::ErrorLecturaIntent::Io) => {
            EstadoRetiroIntent::Indeterminado
        }
        Err(super::lector_intent_canonico::ErrorLecturaIntent::Discrepancia) => {
            EstadoRetiroIntent::Conflicto
        }
    }
}

fn retirar_nombre_plataforma(parent: &File, nombre: &OsStr, archivo: File) -> Result<(), ()> {
    #[cfg(unix)]
    {
        use std::ffi::CString;
        use std::os::fd::AsRawFd;
        use std::os::unix::ffi::OsStrExt;

        let nombre = CString::new(nombre.as_bytes()).map_err(|_| ())?;
        // SAFETY: name is an internal validated component under the retained
        // parent. External name races remain possible between check and call;
        // the validated source handle stays open across unlinkat.
        let resultado = unsafe { libc::unlinkat(parent.as_raw_fd(), nombre.as_ptr(), 0) };
        drop(archivo);
        if resultado == 0 { Ok(()) } else { Err(()) }
    }
    #[cfg(windows)]
    {
        use std::mem::size_of;
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Wdk::Storage::FileSystem::{
            FILE_DISPOSITION_INFORMATION, FileDispositionInformation, NtSetInformationFile,
        };
        use windows_sys::Win32::Foundation::HANDLE;
        use windows_sys::Win32::System::IO::IO_STATUS_BLOCK;

        let _ = (parent, nombre);
        let mut disposicion = FILE_DISPOSITION_INFORMATION { DeleteFile: true };
        let mut estado_io = IO_STATUS_BLOCK::default();
        // SAFETY: this handle was opened with DELETE and the typed reader
        // validated its native identity, scope and full intent record.
        let estado = unsafe {
            NtSetInformationFile(
                archivo.as_raw_handle() as HANDLE,
                &mut estado_io,
                (&mut disposicion as *mut FILE_DISPOSITION_INFORMATION).cast(),
                size_of::<FILE_DISPOSITION_INFORMATION>() as u32,
                FileDispositionInformation,
            )
        };
        drop(archivo);
        if estado >= 0 { Ok(()) } else { Err(()) }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (parent, nombre);
        drop(archivo);
        Err(())
    }
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

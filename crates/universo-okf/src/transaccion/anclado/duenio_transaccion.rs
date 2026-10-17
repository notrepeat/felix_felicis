//! Dueño en memoria del lock y el parent de una transacción.
//!
//! Conserva el lock y el parent retenido, emite leases secuenciales del journal
//! y contexto, y observa el root raw. Sus resultados son datos, no autoridad de
//! recovery; el token de scope no demuestra una frontera general de mounts.

use std::ffi::OsStr;
use std::fmt;
use std::fs::File;
use std::sync::Arc;

use crate::transaccion::captura::{
    ConfigRawCapturada, Snapshot, capturar_snapshot_soportado_anclado,
    capturar_snapshot_soportado_con_cas_anclado,
};
use crate::transaccion::lock::LockEscritura;
use crate::transaccion::snapshot::{ErrorSnapshot, SnapshotToken};

use super::abrir::abrir_entrada_relativa;
use super::{
    DirectorioAnclado, ErrorDirectorioAnclado, IdentidadDirectorio, ScopeFilesystem,
    TipoEntradaAnclada, abrir_directorio_hijo_lectura, identidad_archivo, metadata_directorio,
    scope_filesystem, validar_componente,
};

/// Error único y redactado cuando no se puede demostrar el ownership anclado.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ErrorDuenioTransaccionAnclado {
    NoVerificable,
}

impl ErrorDuenioTransaccionAnclado {
    pub(crate) const fn codigo(self) -> &'static str {
        "transaction_owner_unverified"
    }
}

impl fmt::Debug for ErrorDuenioTransaccionAnclado {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("ErrorDuenioTransaccionAnclado")
            .field(&self.codigo())
            .finish()
    }
}

impl fmt::Display for ErrorDuenioTransaccionAnclado {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.codigo())
    }
}

impl std::error::Error for ErrorDuenioTransaccionAnclado {}

/// Retiene el guard, el parent abierto y los tokens para leases futuros.
///
/// Sus campos son privados y el parent se conserva aunque cambie su nombre en
/// el namespace. La capability solo coordina escritores cooperantes; no cubre
/// hardlinks ni carreras de procesos externos.
#[must_use = "el dueño mantiene el lock y el parent anclado durante la operación"]
pub(crate) struct DuenioTransaccionAnclado<'lock> {
    _lock: &'lock mut LockEscritura,
    parent: File,
    identidad_parent: IdentidadDirectorio,
    scope_parent: ScopeFilesystem,
    _autoridad: Arc<()>,
}

impl<'lock> DuenioTransaccionAnclado<'lock> {
    /// Fija el parent de la ubicación del lock sin abrir ni inspeccionar root.
    pub(crate) fn desde_lock(
        lock: &'lock mut LockEscritura,
    ) -> Result<Self, ErrorDuenioTransaccionAnclado> {
        let ubicacion = lock.ubicacion();
        let parent_path = ubicacion
            .ruta()
            .parent()
            .ok_or(ErrorDuenioTransaccionAnclado::NoVerificable)?;
        let nombre_lock = ubicacion
            .lock()
            .as_path()
            .file_name()
            .ok_or(ErrorDuenioTransaccionAnclado::NoVerificable)?
            .to_os_string();
        validar_componente(&nombre_lock).map_err(mapear_error)?;

        let parent = super::abrir_directorio_absoluto(parent_path, true).map_err(mapear_error)?;
        let identidad_parent = metadata_directorio(&parent).map_err(mapear_error)?;
        let scope_parent = scope_filesystem(&parent).map_err(mapear_error)?;
        let lock_visible =
            abrir_entrada_relativa(&parent, &nombre_lock, TipoEntradaAnclada::Archivo)
                .map_err(mapear_error)?;
        let identidad_visible = identidad_archivo(&lock_visible)
            .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
        let identidad_guard = lock
            .identidad_archivo_lock()
            .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
        if identidad_visible != identidad_guard {
            return Err(ErrorDuenioTransaccionAnclado::NoVerificable);
        }

        let autoridad = lock.autoridad();
        Ok(Self {
            _lock: lock,
            parent,
            identidad_parent,
            scope_parent,
            _autoridad: autoridad,
        })
    }

    /// Emite un lease del journal desde el mismo parent retenido por el dueño.
    ///
    /// Revalida parent, scope y lock sibling antes de clonar el handle. El
    /// préstamo mutable del dueño impide mantener dos leases a la vez.
    pub(crate) fn journal_temporal(
        &mut self,
    ) -> Result<
        super::journal_temporal::JournalTemporalAnclado<'_>,
        super::journal_temporal::ErrorJournalAnclado,
    > {
        let parent = self.parent_revalidado()?;
        super::journal_temporal::desde_parent_retenido(
            &mut *self._lock,
            parent,
            self.identidad_parent,
            Arc::clone(&self._autoridad),
        )
    }

    /// Emite un lease del Journal que conserva el scope capturado por el dueño.
    ///
    /// Emite el lease estricto del Journal usado por el consumidor privado
    /// A→Journal→B. Los leases legacy conservan `journal_temporal`.
    #[allow(dead_code)]
    pub(super) fn journal_temporal_estricto(
        &mut self,
    ) -> Result<
        super::journal_temporal::JournalTemporalAnclado<'_>,
        super::journal_temporal::ErrorJournalAnclado,
    > {
        let parent = self.parent_revalidado()?;
        super::journal_temporal::desde_parent_retenido_con_scope(
            &mut *self._lock,
            parent,
            self.identidad_parent,
            self.scope_parent,
            Arc::clone(&self._autoridad),
        )
    }

    /// Devuelve datos versionados solo tras volver a validar el owner.
    pub(super) fn observar_journal_versionado(
        &mut self,
    ) -> Result<
        Option<super::journal_temporal::versionado::ObservacionJournalPersistido>,
        super::journal_temporal::versionado::ErrorLecturaJournalVersionada,
    > {
        self.observar_journal_versionado_con(|| {})
    }

    #[cfg(test)]
    pub(super) fn observar_journal_versionado_con_prueba<F>(
        &mut self,
        despues_de_leer: F,
    ) -> Result<
        Option<super::journal_temporal::versionado::ObservacionJournalPersistido>,
        super::journal_temporal::versionado::ErrorLecturaJournalVersionada,
    >
    where
        F: FnOnce(),
    {
        self.observar_journal_versionado_con(despues_de_leer)
    }

    fn observar_journal_versionado_con<F>(
        &mut self,
        despues_de_leer: F,
    ) -> Result<
        Option<super::journal_temporal::versionado::ObservacionJournalPersistido>,
        super::journal_temporal::versionado::ErrorLecturaJournalVersionada,
    >
    where
        F: FnOnce(),
    {
        let lectura = self.leer_journal_versionado_con(despues_de_leer)?;
        let postcheck = self.parent_revalidado();
        finalizar_observacion_journal_versionado(lectura, postcheck)
    }

    #[cfg(test)]
    pub(super) fn observar_journal_versionado_con_posterror_prueba<F>(
        &mut self,
        despues_de_leer: F,
        error: super::journal_temporal::ErrorJournalAnclado,
    ) -> Result<
        Option<super::journal_temporal::versionado::ObservacionJournalPersistido>,
        super::journal_temporal::versionado::ErrorLecturaJournalVersionada,
    >
    where
        F: FnOnce(),
    {
        let lectura = self.leer_journal_versionado_con(despues_de_leer)?;
        finalizar_observacion_journal_versionado(lectura, Err(error))
    }

    fn leer_journal_versionado_con<F>(
        &mut self,
        despues_de_leer: F,
    ) -> Result<
        Result<
            Option<super::journal_temporal::versionado::ObservacionJournalPersistido>,
            super::journal_temporal::versionado::ErrorLecturaJournalVersionada,
        >,
        super::journal_temporal::versionado::ErrorLecturaJournalVersionada,
    >
    where
        F: FnOnce(),
    {
        let lectura = {
            let lease = self.journal_temporal_estricto().map_err(
                super::journal_temporal::versionado::ErrorLecturaJournalVersionada::Lease,
            )?;
            lease.observar_versionado()
        };
        despues_de_leer();
        Ok(lectura)
    }

    /// Emite un lease de lectura del contexto canónico desde el mismo parent.
    ///
    /// Revalida identidad, scope y lock visible antes de ceder el lease. Los
    /// bytes leídos siguen siendo datos, no autorización de recovery.
    pub(crate) fn contexto_admision(
        &mut self,
    ) -> Result<
        super::contexto_admision::ContextoAdmisionAnclado<'_>,
        super::contexto_admision::ErrorContextoAdmisionAnclado,
    > {
        let parent = self
            .parent_revalidado()
            .map_err(|_| super::contexto_admision::ErrorContextoAdmisionAnclado::OwnerUnverified)?;
        super::contexto_admision::desde_parent_retenido(
            &mut *self._lock,
            parent,
            self.identidad_parent,
            self.scope_parent,
            Arc::clone(&self._autoridad),
        )
    }

    fn abrir_root_lectura(
        &self,
    ) -> Result<Option<DirectorioAnclado>, ErrorDuenioTransaccionAnclado> {
        let parent = self
            .parent_revalidado()
            .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
        let ruta_root = self._lock.ubicacion().ruta();
        let basename = ruta_root
            .file_name()
            .ok_or(ErrorDuenioTransaccionAnclado::NoVerificable)?;
        self.abrir_sibling_lectura_desde_parent(parent, basename)
    }

    fn abrir_sibling_lectura(
        &self,
        nombre: &OsStr,
    ) -> Result<Option<DirectorioAnclado>, ErrorDuenioTransaccionAnclado> {
        let parent = self
            .parent_revalidado()
            .map_err(|_| ErrorDuenioTransaccionAnclado::NoVerificable)?;
        self.abrir_sibling_lectura_desde_parent(parent, nombre)
    }

    fn abrir_sibling_lectura_desde_parent(
        &self,
        parent: File,
        nombre: &OsStr,
    ) -> Result<Option<DirectorioAnclado>, ErrorDuenioTransaccionAnclado> {
        let basename = validar_componente(nombre).map_err(mapear_error)?;
        let identidad_parent = metadata_directorio(&parent).map_err(mapear_error)?;
        let scope_parent = scope_filesystem(&parent).map_err(mapear_error)?;
        if identidad_parent != self.identidad_parent || scope_parent != self.scope_parent {
            return Err(ErrorDuenioTransaccionAnclado::NoVerificable);
        }

        let raiz = match abrir_directorio_hijo_lectura(&parent, basename) {
            Ok(raiz) => raiz,
            Err(ErrorDirectorioAnclado::EntradaAusente) => return Ok(None),
            Err(error) => return Err(mapear_error(error)),
        };
        let identidad_raiz = metadata_directorio(&raiz).map_err(mapear_error)?;
        if scope_filesystem(&raiz).map_err(mapear_error)? != scope_parent {
            return Err(ErrorDuenioTransaccionAnclado::NoVerificable);
        }

        Ok(Some(DirectorioAnclado {
            parent,
            basename: basename.to_os_string(),
            raiz: Some(raiz),
            identidad_parent,
            identidad_raiz: Some(identidad_raiz),
            permite_escritura: false,
            puede_crear_stage_sibling: false,
            scope_validado: None,
        }))
    }

    /// Captura datos del root abierto desde el parent retenido por este owner.
    /// El snapshot no concede admisión, recovery ni autoridad de escritura.
    pub(crate) fn capturar_snapshot_soportado(
        &mut self,
        configuracion: ConfigRawCapturada,
    ) -> Result<Snapshot, ErrorSnapshot> {
        let raiz = self
            .abrir_root_lectura()
            .map_err(|_| ErrorSnapshot::Io)?
            .ok_or(ErrorSnapshot::RaizInvalida)?;
        capturar_snapshot_soportado_anclado(&raiz, configuracion)
    }

    /// Compara el token antes de validar procedencia o parsear configuración.
    pub(crate) fn capturar_snapshot_soportado_con_cas(
        &mut self,
        configuracion: ConfigRawCapturada,
        esperado: SnapshotToken,
    ) -> Result<Snapshot, ErrorSnapshot> {
        let raiz = self
            .abrir_root_lectura()
            .map_err(|_| ErrorSnapshot::Io)?
            .ok_or(ErrorSnapshot::RaizInvalida)?;
        capturar_snapshot_soportado_con_cas_anclado(&raiz, configuracion, esperado)
    }

    fn parent_revalidado(&self) -> Result<File, super::journal_temporal::ErrorJournalAnclado> {
        use super::journal_temporal::ErrorJournalAnclado;

        let identidad_parent =
            metadata_directorio(&self.parent).map_err(|_| ErrorJournalAnclado::ParentChanged)?;
        if identidad_parent != self.identidad_parent {
            return Err(ErrorJournalAnclado::ParentChanged);
        }
        let scope_parent = scope_filesystem(&self.parent)
            .map_err(|_| ErrorJournalAnclado::ParentScopeUnverified)?;
        if scope_parent != self.scope_parent {
            return Err(ErrorJournalAnclado::ParentScopeUnverified);
        }

        let ruta_lock = self._lock.ubicacion().lock();
        let nombre_lock = ruta_lock
            .as_path()
            .file_name()
            .ok_or(ErrorJournalAnclado::Io)?
            .to_os_string();
        validar_componente(&nombre_lock).map_err(|_| ErrorJournalAnclado::Io)?;
        let lock_visible =
            abrir_entrada_relativa(&self.parent, &nombre_lock, TipoEntradaAnclada::Archivo)
                .map_err(|error| match error {
                    ErrorDirectorioAnclado::EnlaceNoPermitido => {
                        ErrorJournalAnclado::ReparseNotAllowed
                    }
                    ErrorDirectorioAnclado::TipoNoAdmitido => {
                        ErrorJournalAnclado::RegularFileRequired
                    }
                    _ => ErrorJournalAnclado::Io,
                })?;
        let identidad_visible =
            identidad_archivo(&lock_visible).map_err(|_| ErrorJournalAnclado::Io)?;
        let identidad_guard = self
            ._lock
            .identidad_archivo_lock()
            .map_err(|_| ErrorJournalAnclado::Io)?;
        if identidad_visible != identidad_guard {
            return Err(ErrorJournalAnclado::IdentityMismatch);
        }

        self.parent.try_clone().map_err(|_| ErrorJournalAnclado::Io)
    }
}

impl fmt::Debug for DuenioTransaccionAnclado<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DuenioTransaccionAnclado { .. }")
    }
}

fn mapear_error(_error: ErrorDirectorioAnclado) -> ErrorDuenioTransaccionAnclado {
    ErrorDuenioTransaccionAnclado::NoVerificable
}

fn finalizar_observacion_journal_versionado(
    lectura: Result<
        Option<super::journal_temporal::versionado::ObservacionJournalPersistido>,
        super::journal_temporal::versionado::ErrorLecturaJournalVersionada,
    >,
    postcheck: Result<File, super::journal_temporal::ErrorJournalAnclado>,
) -> Result<
    Option<super::journal_temporal::versionado::ObservacionJournalPersistido>,
    super::journal_temporal::versionado::ErrorLecturaJournalVersionada,
> {
    postcheck.map_err(super::journal_temporal::versionado::ErrorLecturaJournalVersionada::Lease)?;
    lectura
}

#[cfg(all(test, any(target_os = "linux", windows)))]
#[path = "duenio_transaccion/fixture_root_replay.rs"]
pub(super) mod fixture_root_replay;

#[cfg(all(test, any(target_os = "linux", windows)))]
#[path = "duenio_transaccion/captura_soportada_tests.rs"]
mod captura_soportada_tests;

#[path = "duenio_transaccion/comparar_root_replay.rs"]
mod comparar_root_replay;

#[path = "duenio_transaccion/observar_root.rs"]
mod observar_root;

#[path = "duenio_transaccion/observar_siblings_nuevo_flujo.rs"]
mod observar_siblings_nuevo_flujo;

#[path = "duenio_transaccion/intent_retiro.rs"]
pub(in crate::transaccion::anclado::duenio_transaccion) mod intent_retiro;

// El reconciliador incremental será el primer consumidor productivo.
#[allow(dead_code)]
#[path = "duenio_transaccion/lector_intent_canonico.rs"]
mod lector_intent_canonico;

#[path = "duenio_transaccion/observer.rs"]
mod observer;

#[path = "duenio_transaccion/prueba_root_coincidente.rs"]
mod prueba_root_coincidente;

#[cfg(all(test, any(target_os = "linux", windows)))]
#[path = "duenio_transaccion_tests.rs"]
mod tests;

#[cfg(all(test, any(target_os = "linux", windows)))]
#[path = "duenio_transaccion/observer_namespace_tests.rs"]
mod observer_namespace_tests;

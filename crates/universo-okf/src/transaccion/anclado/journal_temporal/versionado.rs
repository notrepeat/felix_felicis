//! Lectura privada del Journal versionado desde el mismo handle anclado.

use std::fmt;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

use crate::transaccion::journal::DirectoryIdentity;
use crate::transaccion::journal::selector::{
    JournalPersistido, JournalPersistidoError, MAX_VERSIONED_JOURNAL_BYTES,
};

use super::{
    ErrorDirectorioAnclado, ErrorJournalAnclado, IdentidadArchivoJournal, JournalTemporalAnclado,
    ScopeFilesystem, TipoEntradaAnclada, abrir_entrada_relativa, identidad_archivo,
    mapear_error_directorio, scope_filesystem, validar_archivo_regular,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::transaccion::anclado) enum ErrorLecturaJournalVersionada {
    Lease(ErrorJournalAnclado),
    Selector(JournalPersistidoError),
}

impl ErrorLecturaJournalVersionada {
    const fn codigo(self) -> &'static str {
        match self {
            Self::Lease(error) => error.codigo(),
            Self::Selector(error) => error.code(),
        }
    }
}

impl fmt::Debug for ErrorLecturaJournalVersionada {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("ErrorLecturaJournalVersionada")
            .field(&self.codigo())
            .finish()
    }
}

impl fmt::Display for ErrorLecturaJournalVersionada {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.codigo())
    }
}

impl std::error::Error for ErrorLecturaJournalVersionada {}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::transaccion::anclado) enum EtapaLecturaJournalVersionada {
    DespuesDePrimeraAusencia,
    DespuesDeMetadata,
    AntesDeSondaVisible,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::transaccion::anclado) enum EtapaProbeScopeJournalVersionada {
    ParentInicial,
    ParentDespuesDePrimeraAusencia,
    SourceInicial,
    VisibleFinal,
}

pub(in crate::transaccion::anclado) struct ObservacionJournalPersistido {
    journal: JournalPersistido,
    identidad: IdentidadArchivoJournal,
    _archivo: File,
}

impl ObservacionJournalPersistido {
    pub(in crate::transaccion::anclado) fn journal(&self) -> &JournalPersistido {
        &self.journal
    }

    pub(in crate::transaccion::anclado) fn identidad(&self) -> IdentidadArchivoJournal {
        self.identidad
    }
}

impl fmt::Debug for ObservacionJournalPersistido {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ObservacionJournalPersistido([redacted])")
    }
}

impl JournalTemporalAnclado<'_> {
    pub(in crate::transaccion::anclado) fn observar_versionado(
        &self,
    ) -> Result<Option<ObservacionJournalPersistido>, ErrorLecturaJournalVersionada> {
        self.observar_versionado_con(|_| {}, |_, handle| scope_filesystem(handle))
    }

    #[cfg(test)]
    pub(in crate::transaccion::anclado) fn observar_versionado_con_prueba<F>(
        &self,
        despues_de_etapa: F,
    ) -> Result<Option<ObservacionJournalPersistido>, ErrorLecturaJournalVersionada>
    where
        F: FnMut(EtapaLecturaJournalVersionada),
    {
        self.observar_versionado_con(despues_de_etapa, |_, handle| scope_filesystem(handle))
    }

    #[cfg(test)]
    pub(in crate::transaccion::anclado) fn observar_versionado_con_scope_prueba<F>(
        &self,
        scope_probe: F,
    ) -> Result<Option<ObservacionJournalPersistido>, ErrorLecturaJournalVersionada>
    where
        F: FnMut(
            EtapaProbeScopeJournalVersionada,
            &File,
        ) -> Result<ScopeFilesystem, ErrorDirectorioAnclado>,
    {
        self.observar_versionado_con(|_| {}, scope_probe)
    }

    fn observar_versionado_con<F, P>(
        &self,
        mut despues_de_etapa: F,
        mut scope_probe: P,
    ) -> Result<Option<ObservacionJournalPersistido>, ErrorLecturaJournalVersionada>
    where
        F: FnMut(EtapaLecturaJournalVersionada),
        P: FnMut(
            EtapaProbeScopeJournalVersionada,
            &File,
        ) -> Result<ScopeFilesystem, ErrorDirectorioAnclado>,
    {
        if self.scope_parent.is_none() {
            return Err(ErrorLecturaJournalVersionada::Lease(
                ErrorJournalAnclado::ParentScopeUnverified,
            ));
        }
        {
            let mut probe = |handle: &File| {
                scope_probe(EtapaProbeScopeJournalVersionada::ParentInicial, handle)
            };
            self.validar_parent_con_scope(&mut probe)
                .map_err(ErrorLecturaJournalVersionada::Lease)?;
        }

        let mut archivo = match abrir_entrada_relativa(
            &self.parent,
            &self.nombre_journal,
            TipoEntradaAnclada::Archivo,
        ) {
            Ok(archivo) => archivo,
            Err(ErrorDirectorioAnclado::EntradaAusente) => {
                {
                    let mut probe = |handle: &File| {
                        scope_probe(
                            EtapaProbeScopeJournalVersionada::ParentDespuesDePrimeraAusencia,
                            handle,
                        )
                    };
                    self.validar_parent_con_scope(&mut probe)
                        .map_err(ErrorLecturaJournalVersionada::Lease)?;
                }
                despues_de_etapa(EtapaLecturaJournalVersionada::DespuesDePrimeraAusencia);
                match abrir_entrada_relativa(
                    &self.parent,
                    &self.nombre_journal,
                    TipoEntradaAnclada::Archivo,
                ) {
                    Err(ErrorDirectorioAnclado::EntradaAusente) => return Ok(None),
                    Ok(reaparecido) => {
                        drop(reaparecido);
                        return Err(ErrorLecturaJournalVersionada::Lease(
                            ErrorJournalAnclado::CanonicalConflict,
                        ));
                    }
                    Err(error) => {
                        return Err(ErrorLecturaJournalVersionada::Lease(
                            mapear_error_directorio(error),
                        ));
                    }
                }
            }
            Err(error) => {
                return Err(ErrorLecturaJournalVersionada::Lease(
                    mapear_error_directorio(error),
                ));
            }
        };

        validar_archivo_regular(&archivo).map_err(ErrorLecturaJournalVersionada::Lease)?;
        {
            let mut probe = |handle: &File| {
                scope_probe(EtapaProbeScopeJournalVersionada::SourceInicial, handle)
            };
            self.validar_scope_source_con(&archivo, &mut probe)
                .map_err(ErrorLecturaJournalVersionada::Lease)?;
        }
        let identidad =
            identidad_archivo(&archivo).map_err(ErrorLecturaJournalVersionada::Lease)?;

        let cuota = MAX_VERSIONED_JOURNAL_BYTES;
        let limite_usize = cuota
            .checked_add(1)
            .ok_or(ErrorLecturaJournalVersionada::Lease(
                ErrorJournalAnclado::TooLarge,
            ))?;
        let limite_u64 = u64::try_from(limite_usize)
            .map_err(|_| ErrorLecturaJournalVersionada::Lease(ErrorJournalAnclado::TooLarge))?;
        let cuota_u64 = u64::try_from(cuota)
            .map_err(|_| ErrorLecturaJournalVersionada::Lease(ErrorJournalAnclado::TooLarge))?;
        if archivo
            .metadata()
            .map_err(|_| ErrorLecturaJournalVersionada::Lease(ErrorJournalAnclado::Io))?
            .len()
            > cuota_u64
        {
            return Err(ErrorLecturaJournalVersionada::Lease(
                ErrorJournalAnclado::TooLarge,
            ));
        }
        despues_de_etapa(EtapaLecturaJournalVersionada::DespuesDeMetadata);

        archivo
            .seek(SeekFrom::Start(0))
            .map_err(|_| ErrorLecturaJournalVersionada::Lease(ErrorJournalAnclado::Io))?;
        let mut bytes = Vec::with_capacity(limite_usize);
        (&mut archivo)
            .take(limite_u64)
            .read_to_end(&mut bytes)
            .map_err(|_| ErrorLecturaJournalVersionada::Lease(ErrorJournalAnclado::Io))?;
        if bytes.len() > cuota {
            return Err(ErrorLecturaJournalVersionada::Lease(
                ErrorJournalAnclado::TooLarge,
            ));
        }

        let journal = crate::transaccion::journal::selector::parse(&bytes)
            .map_err(ErrorLecturaJournalVersionada::Selector)?;
        let parent_esperado = self.identidad_parent.journal_identity().ok_or(
            ErrorLecturaJournalVersionada::Lease(ErrorJournalAnclado::ParentIdentityUnavailable),
        )?;
        if identidad_parent_journal(&journal) != &parent_esperado {
            return Err(ErrorLecturaJournalVersionada::Lease(
                ErrorJournalAnclado::ParentIdentityMismatch,
            ));
        }

        despues_de_etapa(EtapaLecturaJournalVersionada::AntesDeSondaVisible);
        let visible = abrir_entrada_relativa(
            &self.parent,
            &self.nombre_journal,
            TipoEntradaAnclada::Archivo,
        )
        .map_err(|error| match error {
            ErrorDirectorioAnclado::EntradaAusente => {
                ErrorLecturaJournalVersionada::Lease(ErrorJournalAnclado::CanonicalConflict)
            }
            otro => ErrorLecturaJournalVersionada::Lease(mapear_error_directorio(otro)),
        })?;
        validar_archivo_regular(&visible).map_err(ErrorLecturaJournalVersionada::Lease)?;
        {
            let mut probe =
                |handle: &File| scope_probe(EtapaProbeScopeJournalVersionada::VisibleFinal, handle);
            self.validar_scope_source_con(&visible, &mut probe)
                .map_err(ErrorLecturaJournalVersionada::Lease)?;
        }
        let identidad_visible =
            identidad_archivo(&visible).map_err(ErrorLecturaJournalVersionada::Lease)?;
        if identidad_visible != identidad {
            return Err(ErrorLecturaJournalVersionada::Lease(
                ErrorJournalAnclado::IdentityMismatch,
            ));
        }

        Ok(Some(ObservacionJournalPersistido {
            journal,
            identidad,
            _archivo: archivo,
        }))
    }
}

fn identidad_parent_journal(journal: &JournalPersistido) -> &DirectoryIdentity {
    match journal {
        JournalPersistido::V1(journal) => journal.parent(),
        JournalPersistido::V2(journal) => journal.parent(),
    }
}

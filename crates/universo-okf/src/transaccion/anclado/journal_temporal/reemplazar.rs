use super::*;
use std::io::Read;

type RegistroLeido = (IdentidadArchivoJournal, Journal);
type ResultadoObservacionJournal = Result<Option<RegistroLeido>, ErrorJournalAnclado>;

impl JournalTemporalAnclado<'_> {
    /// Reemplaza el registro canónico solo por el sucesor tipado y esperado.
    /// El lock serializa a escritores cooperantes; procesos que muten estos
    /// nombres directamente quedan fuera del contrato de coordinación.
    pub(crate) fn reemplazar_siguiente(
        &self,
        identidad_canonica_esperada: IdentidadArchivoJournal,
        identidad_temporal_esperada: IdentidadArchivoJournal,
        candidato_esperado: &Journal,
    ) -> ResultadoPublicacionJournal {
        self.reemplazar_siguiente_con(
            identidad_canonica_esperada,
            identidad_temporal_esperada,
            candidato_esperado,
            |capability, temporal| {
                super::publicar::reemplazar_plataforma(
                    &capability.parent,
                    &capability.nombre,
                    &capability.nombre_journal,
                    temporal,
                )
            },
        )
    }

    fn reemplazar_siguiente_con(
        &self,
        identidad_canonica_esperada: IdentidadArchivoJournal,
        identidad_temporal_esperada: IdentidadArchivoJournal,
        candidato_esperado: &Journal,
        reemplazar: impl FnOnce(&Self, &File) -> Result<(), ErrorJournalAnclado>,
    ) -> ResultadoPublicacionJournal {
        if self.raiz_remota {
            return precondicion(ErrorJournalAnclado::RemoteUnsupported);
        }
        if let Err(error) = self.validar_parent() {
            return precondicion(error);
        }

        let temporal = match self.cargar_temporal() {
            Ok(registro) => registro,
            Err(ErrorJournalAnclado::SourceMissing) => {
                return self.resolver_reintento_sin_temporal(
                    identidad_temporal_esperada,
                    candidato_esperado,
                );
            }
            Err(error) => return precondicion(error),
        };
        let Some(canonico) = (match self.cargar_canonico() {
            Ok(registro) => registro,
            Err(error) => return precondicion(error),
        }) else {
            return precondicion(ErrorJournalAnclado::SourceMissing);
        };

        if canonico.0 != identidad_canonica_esperada || temporal.0 != identidad_temporal_esperada {
            return precondicion(ErrorJournalAnclado::IdentityMismatch);
        }
        if &temporal.1 != candidato_esperado {
            return precondicion(ErrorJournalAnclado::InvalidJournal(
                JournalError::InvalidTransition,
            ));
        }
        if let Err(error) = canonico.1.validate_successor(candidato_esperado) {
            return precondicion(ErrorJournalAnclado::InvalidJournal(error));
        }

        // Reopen both names without following links immediately before rename.
        // Windows needs the source handle itself for NtSetInformationFile; the
        // target handle is dropped before that call so it cannot block replace.
        let (target, target_id, target_record) = match self.abrir_cargar_canonico() {
            Ok(registro) => registro,
            Err(error) => return precondicion(error),
        };
        if target_id != identidad_canonica_esperada {
            return precondicion(ErrorJournalAnclado::IdentityMismatch);
        }
        if target_record != canonico.1 {
            return precondicion(ErrorJournalAnclado::InvalidJournal(
                JournalError::InvalidTransition,
            ));
        }
        drop(target);

        let (source, source_id, source_record) = match self.abrir_temporal_para_reemplazo() {
            Ok(registro) => registro,
            Err(error) => return precondicion(error),
        };
        if source_id != identidad_temporal_esperada {
            return precondicion(ErrorJournalAnclado::IdentityMismatch);
        }
        if source_record != *candidato_esperado {
            return precondicion(ErrorJournalAnclado::InvalidJournal(
                JournalError::InvalidTransition,
            ));
        }
        if let Err(error) = self.validar_parent() {
            return precondicion(error);
        }

        let intento = match reemplazar(self, &source) {
            Ok(()) => EstadoIntentoReemplazoJournal::Exito,
            Err(error) => EstadoIntentoReemplazoJournal::Error(error),
        };
        self.clasificar_despues_de_reemplazo(
            identidad_canonica_esperada,
            canonico.1,
            identidad_temporal_esperada,
            candidato_esperado,
            intento,
        )
    }

    fn resolver_reintento_sin_temporal(
        &self,
        identidad_temporal_esperada: IdentidadArchivoJournal,
        candidato_esperado: &Journal,
    ) -> ResultadoPublicacionJournal {
        match self.cargar_canonico() {
            Ok(Some((identidad, registro)))
                if identidad == identidad_temporal_esperada && registro == *candidato_esperado =>
            {
                ResultadoPublicacionJournal::Confirmado {
                    limpieza: LimpiezaConfirmadaJournal::Completa,
                }
            }
            Ok(Some((identidad, registro)))
                if identidad == identidad_temporal_esperada && registro != *candidato_esperado =>
            {
                ResultadoPublicacionJournal::Indeterminado {
                    causa: CausaIndeterminacionPublicacionJournal::ReemplazoNoDemostrable {
                        intento: EstadoIntentoReemplazoJournal::NoInvocado,
                        canonico: EstadoObservacionReemplazoJournal::RegistroDistinto,
                        temporal: EstadoObservacionReemplazoJournal::Ausente,
                    },
                }
            }
            Ok(_) => precondicion(ErrorJournalAnclado::SourceMissing),
            Err(error) => precondicion(error),
        }
    }

    fn abrir_cargar_canonico(
        &self,
    ) -> Result<(File, IdentidadArchivoJournal, Journal), ErrorJournalAnclado> {
        self.validar_parent()?;
        let archivo = abrir_entrada_relativa(
            &self.parent,
            &self.nombre_journal,
            TipoEntradaAnclada::Archivo,
        )
        .map_err(super::publicar::mapear_error_canonico)?;
        self.cargar_handle(archivo)
    }

    fn abrir_temporal_para_reemplazo(
        &self,
    ) -> Result<(File, IdentidadArchivoJournal, Journal), ErrorJournalAnclado> {
        self.validar_parent()?;
        let archivo = super::publicar::abrir_temporal(&self.parent, &self.nombre)?;
        self.cargar_handle(archivo)
    }

    fn cargar_handle(
        &self,
        mut archivo: File,
    ) -> Result<(File, IdentidadArchivoJournal, Journal), ErrorJournalAnclado> {
        validar_archivo_regular(&archivo)?;
        let identidad = identidad_archivo(&archivo)?;
        let mut bytes = Vec::with_capacity(MAX_JOURNAL_BYTES + 1);
        Read::by_ref(&mut archivo)
            .take((MAX_JOURNAL_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|_| ErrorJournalAnclado::Io)?;
        if bytes.len() > MAX_JOURNAL_BYTES {
            return Err(ErrorJournalAnclado::TooLarge);
        }
        let (_, registro) = self.cargar_journal_leido(identidad, &bytes)?;
        Ok((archivo, identidad, registro))
    }

    fn clasificar_despues_de_reemplazo(
        &self,
        identidad_anterior: IdentidadArchivoJournal,
        anterior: Journal,
        identidad_candidato: IdentidadArchivoJournal,
        candidato: &Journal,
        intento: EstadoIntentoReemplazoJournal,
    ) -> ResultadoPublicacionJournal {
        let canonico = self.observar_canonico_reemplazo();
        let temporal = self.observar_temporal_reemplazo();
        let estado_canonico = estado_observacion(
            &canonico,
            identidad_anterior,
            &anterior,
            identidad_candidato,
            candidato,
        );
        let estado_temporal = estado_observacion(
            &temporal,
            identidad_anterior,
            &anterior,
            identidad_candidato,
            candidato,
        );

        if estado_canonico == EstadoObservacionReemplazoJournal::CoincideCandidato {
            return ResultadoPublicacionJournal::Confirmado {
                limpieza: self.limpiar_temporal_candidato(identidad_candidato, candidato),
            };
        }
        if let EstadoIntentoReemplazoJournal::Error(error) = intento
            && estado_canonico == EstadoObservacionReemplazoJournal::CoincideAnterior
            && estado_temporal == EstadoObservacionReemplazoJournal::CoincideCandidato
        {
            return ResultadoPublicacionJournal::NoAplicado(error);
        }
        ResultadoPublicacionJournal::Indeterminado {
            causa: CausaIndeterminacionPublicacionJournal::ReemplazoNoDemostrable {
                intento,
                canonico: estado_canonico,
                temporal: estado_temporal,
            },
        }
    }

    fn observar_canonico_reemplazo(&self) -> ResultadoObservacionJournal {
        self.cargar_canonico()
    }

    fn observar_temporal_reemplazo(&self) -> ResultadoObservacionJournal {
        match self.cargar_temporal() {
            Ok(registro) => Ok(Some(registro)),
            Err(ErrorJournalAnclado::SourceMissing) => Ok(None),
            Err(error) => Err(error),
        }
    }

    fn limpiar_temporal_candidato(
        &self,
        identidad_esperada: IdentidadArchivoJournal,
        candidato: &Journal,
    ) -> LimpiezaConfirmadaJournal {
        match self.observar_temporal_reemplazo() {
            Ok(None) => LimpiezaConfirmadaJournal::Completa,
            Ok(Some((identidad, registro)))
                if identidad == identidad_esperada && registro == *candidato =>
            {
                self.limpiar_temporal_si_coincide(identidad_esperada)
            }
            Ok(Some((identidad, _))) if identidad != identidad_esperada => {
                LimpiezaConfirmadaJournal::Conflicto(ErrorJournalAnclado::IdentityMismatch)
            }
            Ok(Some(_)) => LimpiezaConfirmadaJournal::Conflicto(
                ErrorJournalAnclado::InvalidJournal(JournalError::InvalidTransition),
            ),
            Err(error) if es_conflicto_temporal(error) => {
                LimpiezaConfirmadaJournal::Conflicto(error)
            }
            Err(_) => LimpiezaConfirmadaJournal::Indeterminada,
        }
    }
}

fn precondicion(error: ErrorJournalAnclado) -> ResultadoPublicacionJournal {
    ResultadoPublicacionJournal::PrecondicionFallida(error)
}

fn estado_observacion(
    observado: &ResultadoObservacionJournal,
    identidad_anterior: IdentidadArchivoJournal,
    anterior: &Journal,
    identidad_candidato: IdentidadArchivoJournal,
    candidato: &Journal,
) -> EstadoObservacionReemplazoJournal {
    let registro = match observado {
        Ok(Some(registro)) => registro,
        Ok(None) => return EstadoObservacionReemplazoJournal::Ausente,
        Err(error) => return EstadoObservacionReemplazoJournal::NoVerificable(*error),
    };
    if registro.0 == identidad_candidato && &registro.1 == candidato {
        EstadoObservacionReemplazoJournal::CoincideCandidato
    } else if registro.0 == identidad_anterior && &registro.1 == anterior {
        EstadoObservacionReemplazoJournal::CoincideAnterior
    } else if registro.0 == identidad_anterior || registro.0 == identidad_candidato {
        EstadoObservacionReemplazoJournal::RegistroDistinto
    } else {
        EstadoObservacionReemplazoJournal::IdentidadDistinta
    }
}

fn es_conflicto_temporal(error: ErrorJournalAnclado) -> bool {
    matches!(
        error,
        ErrorJournalAnclado::IdentityMismatch
            | ErrorJournalAnclado::RegularFileRequired
            | ErrorJournalAnclado::ReparseNotAllowed
    )
}

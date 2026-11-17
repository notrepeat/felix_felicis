//! Publica un sucesor esperado. No acredita la fila root/backup/stage del caller.
use super::*;
use std::io::{Read, Seek, SeekFrom};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::transaccion::anclado) enum EstadoPublicacionSucesorV2 {
    Confirmada,
    Conservado,
    Indeterminada,
}

impl JournalTemporalAnclado<'_> {
    /// Retiene el recibo en el caller incluso ante resultado ambiguo.
    /// Cooperantes bajo lock; replace no evita carreras de escritores externos.
    pub(in crate::transaccion::anclado) fn publicar_sucesor_v2(
        &self,
        recibo: &ReciboTemporalSucesorJournalV2<'_>,
    ) -> Result<EstadoPublicacionSucesorV2, ErrorJournalAnclado> {
        self.publicar_sucesor_v2_con(
            recibo,
            || {},
            |lease, source| {
                publicar::reemplazar_sucesor_plataforma(
                    &lease.parent,
                    &lease.nombre,
                    &lease.nombre_journal,
                    source,
                )
            },
        )
    }

    #[cfg(test)]
    pub(in crate::transaccion::anclado) fn publicar_sucesor_v2_con_prueba<H, F>(
        &self,
        recibo: &ReciboTemporalSucesorJournalV2<'_>,
        hook: H,
        replace: F,
    ) -> Result<EstadoPublicacionSucesorV2, ErrorJournalAnclado>
    where
        H: FnOnce(),
        F: FnOnce(&Self, &File) -> Result<(), ErrorJournalAnclado>,
    {
        self.publicar_sucesor_v2_con(recibo, hook, replace)
    }

    fn publicar_sucesor_v2_con<H, F>(
        &self,
        recibo: &ReciboTemporalSucesorJournalV2<'_>,
        hook: H,
        replace: F,
    ) -> Result<EstadoPublicacionSucesorV2, ErrorJournalAnclado>
    where
        H: FnOnce(),
        F: FnOnce(&Self, &File) -> Result<(), ErrorJournalAnclado>,
    {
        if self.raiz_remota {
            return Err(ErrorJournalAnclado::RemoteUnsupported);
        }
        if self.scope_parent.is_none() {
            return Err(ErrorJournalAnclado::ParentScopeUnverified);
        }
        self.validar_parent()?;
        let JournalPersistido::V2(old) = recibo.original.journal() else {
            return Err(ErrorJournalAnclado::JournalMismatch);
        };
        old.validate_successor(&recibo.candidato)
            .map_err(ErrorJournalAnclado::InvalidJournalV2)?;
        self.validar_canonico_sucesor_v2(recibo.original.identidad(), old)?;
        let tmp = self
            .observar_temporal_versionado(&recibo.candidato)
            .map_err(mapear_error_preparacion_v2)?;
        if tmp.identidad() != recibo.identidad_temporal {
            return Err(ErrorJournalAnclado::IdentityMismatch);
        }
        drop(tmp);
        hook();
        self.validar_canonico_sucesor_v2(recibo.original.identidad(), old)?;
        let mut source = abrir_fuente(&self.parent, &self.nombre)?;
        self.verificar_source_sucesor(&mut source, recibo)?;
        self.validar_parent()?;
        let _resultado = replace(self, &source);
        drop(source);
        // Fallos de parent después de una syscall no afirman ausencia de efecto.
        if self.validar_parent().is_err() {
            return Ok(EstadoPublicacionSucesorV2::Indeterminada);
        }
        let canonico = self.observar_versionado();
        let temporal_ausente = matches!(
            abrir_entrada_relativa(&self.parent, &self.nombre, TipoEntradaAnclada::Archivo),
            Err(ErrorDirectorioAnclado::EntradaAusente)
        );
        let estado = match canonico {
            Ok(Some(actual))
                if actual.identidad() == recibo.identidad_temporal
                    && matches!(actual.journal(), JournalPersistido::V2(j) if j == &recibo.candidato)
                    && temporal_ausente =>
            {
                EstadoPublicacionSucesorV2::Confirmada
            }
            Ok(Some(actual))
                if actual.identidad() == recibo.original.identidad()
                    && matches!(actual.journal(), JournalPersistido::V2(j) if j == old) =>
            {
                match self.observar_temporal_versionado(&recibo.candidato) {
                    Ok(tmp) if tmp.identidad() == recibo.identidad_temporal => {
                        EstadoPublicacionSucesorV2::Conservado
                    }
                    _ => EstadoPublicacionSucesorV2::Indeterminada,
                }
            }
            _ => EstadoPublicacionSucesorV2::Indeterminada,
        };
        if self.validar_parent().is_err() {
            return Ok(EstadoPublicacionSucesorV2::Indeterminada);
        }
        Ok(estado)
    }

    fn verificar_source_sucesor(
        &self,
        source: &mut File,
        recibo: &ReciboTemporalSucesorJournalV2<'_>,
    ) -> Result<(), ErrorJournalAnclado> {
        validar_archivo_regular(source)?;
        self.validar_scope_source_con(source, &mut scope_filesystem)?;
        if identidad_archivo(source)? != recibo.identidad_temporal {
            return Err(ErrorJournalAnclado::IdentityMismatch);
        }
        let expected = recibo
            .candidato
            .encode()
            .map_err(ErrorJournalAnclado::InvalidJournalV2)?;
        if source
            .metadata()
            .map_err(|_| ErrorJournalAnclado::Io)?
            .len()
            != expected.len() as u64
        {
            return Err(ErrorJournalAnclado::JournalMismatch);
        }
        source
            .seek(SeekFrom::Start(0))
            .map_err(|_| ErrorJournalAnclado::Io)?;
        let mut bytes = Vec::new();
        Read::by_ref(source)
            .take(expected.len() as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| ErrorJournalAnclado::Io)?;
        if bytes != expected {
            return Err(ErrorJournalAnclado::JournalMismatch);
        }
        self.validar_scope_source_con(source, &mut scope_filesystem)?;
        if identidad_archivo(source)? != recibo.identidad_temporal {
            return Err(ErrorJournalAnclado::IdentityMismatch);
        }
        Ok(())
    }
}

#[cfg(not(windows))]
fn abrir_fuente(parent: &File, nombre: &std::ffi::OsStr) -> Result<File, ErrorJournalAnclado> {
    publicar::abrir_temporal(parent, nombre)
}

#[cfg(windows)]
fn abrir_fuente(parent: &File, nombre: &std::ffi::OsStr) -> Result<File, ErrorJournalAnclado> {
    use fs_at::os::windows::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::{DELETE, FILE_READ_ATTRIBUTES, FILE_READ_DATA};
    let mut options = OpenOptionsAt::default();
    options
        .desired_access(DELETE | FILE_READ_ATTRIBUTES | FILE_READ_DATA)
        .follow(false);
    options
        .open_at(parent, Path::new(nombre))
        .map_err(|_| ErrorJournalAnclado::Io)
}

//! Rollback terminal: Context antes de Journal, y recuperación Journal-only.
use super::super::observar_siblings_nuevo_flujo::PresenciaSiblingNuevoFlujo::{Ausente, Presente};
use super::*;
use crate::transaccion::admision::envelope::selector::EnvelopeAdmisionVersionada;
use crate::transaccion::anclado::{
    IdentidadArchivo, identidad_archivo, metadata_es_reparse, scope_filesystem,
};
use crate::transaccion::journal::selector::JournalPersistido;
use sha2::{Digest, Sha256};
use std::{
    ffi::{OsStr, OsString},
    fs::File,
    io::Read,
};

type Raw = (IdentidadArchivo, u64, [u8; 32]);
struct Terminal {
    journal: JournalV2,
    journal_raw: Raw,
    contexto: Option<(EnvelopeAdmisionVersionada, Raw)>,
}
pub(crate) struct ErrorTerminalRollbackV2 {
    evidencia: Option<Box<Terminal>>,
}
impl std::fmt::Debug for ErrorTerminalRollbackV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("rollback_terminal_unverified")
    }
}
impl ErrorTerminalRollbackV2 {
    fn nuevo() -> Self {
        Self { evidencia: None }
    }
}
impl DuenioTransaccionAnclado<'_> {
    pub(crate) fn retirar_terminal_rollback_v2(&mut self) -> Result<(), ErrorTerminalRollbackV2> {
        self.retirar_terminal_rollback_v2_con(|| {})
    }
    fn retirar_terminal_rollback_v2_con<H: FnOnce()>(
        &mut self,
        despues_contexto: H,
    ) -> Result<(), ErrorTerminalRollbackV2> {
        let initial = self.observar_terminal_rollback_v2()?;
        let result = (|| {
            if let Some((_, raw)) = &initial.contexto {
                let fresh = self.observar_terminal_rollback_v2()?;
                if !initial.coincide(&fresh) {
                    return Err(ErrorTerminalRollbackV2::nuevo());
                }
                drop(fresh);
                let nombre = self.nombre_contexto_rollback()?;
                self.eliminar_sidecar_rollback(&nombre, *raw)?;
                let after = self.observar_terminal_rollback_v2()?;
                if after.contexto.is_some()
                    || after.journal != initial.journal
                    || after.journal_raw != initial.journal_raw
                {
                    return Err(ErrorTerminalRollbackV2::nuevo());
                }
            }
            despues_contexto();
            let final_proof = self.observar_terminal_rollback_v2()?;
            if final_proof.contexto.is_some()
                || final_proof.journal != initial.journal
                || final_proof.journal_raw != initial.journal_raw
            {
                return Err(ErrorTerminalRollbackV2::nuevo());
            }
            let nombre = self
                ._lock
                .ubicacion()
                .journal()
                .as_path()
                .file_name()
                .ok_or_else(ErrorTerminalRollbackV2::nuevo)?
                .to_os_string();
            self.eliminar_sidecar_rollback(&nombre, initial.journal_raw)?;
            self.validar_terminal_vacio(&initial.journal)
        })();
        let result = if self.parent_revalidado().is_err() {
            Err(ErrorTerminalRollbackV2::nuevo())
        } else {
            result
        };
        result.map_err(|mut error| {
            error.evidencia = Some(Box::new(initial));
            error
        })
    }
    fn observar_terminal_rollback_v2(&mut self) -> Result<Terminal, ErrorTerminalRollbackV2> {
        self.parent_revalidado()
            .map_err(|_| ErrorTerminalRollbackV2::nuevo())?;
        let slots = self
            .observar_siblings_nuevo_flujo()
            .map_err(|_| ErrorTerminalRollbackV2::nuevo())?;
        if slots.journal() != Presente
            || slots.journal_temporal() != Ausente
            || slots.contexto_temporal() != Ausente
            || slots.intent_retiro() != Ausente
            || slots.intent_retiro_temporal() != Ausente
        {
            return Err(ErrorTerminalRollbackV2::nuevo());
        }
        let original = self
            .observar_journal_versionado()
            .map_err(|_| ErrorTerminalRollbackV2::nuevo())?
            .ok_or_else(ErrorTerminalRollbackV2::nuevo)?;
        let JournalPersistido::V2(journal) = original.journal() else {
            return Err(ErrorTerminalRollbackV2::nuevo());
        };
        if journal.state() == JournalState::Committed
            || journal.old().identity() == journal.new_fingerprint().identity()
        {
            return Err(ErrorTerminalRollbackV2::nuevo());
        }
        let bytes = journal
            .encode()
            .map_err(|_| ErrorTerminalRollbackV2::nuevo())?;
        let journal_raw = (
            original.identidad().identidad_archivo(),
            bytes.len() as u64,
            Sha256::digest(&bytes).into(),
        );
        let journal = journal.clone();
        let contexto = match self
            .observar_contexto_versionado()
            .map_err(|_| ErrorTerminalRollbackV2::nuevo())?
        {
            Some(contexto) => {
                let EnvelopeAdmisionVersionada::V2(envelope) = contexto.envelope_versionado()
                else {
                    return Err(ErrorTerminalRollbackV2::nuevo());
                };
                envelope
                    .calcular_new_esperado(&journal)
                    .map_err(|_| ErrorTerminalRollbackV2::nuevo())?;
                Some(contexto.into_evidencia_sin_handle())
            }
            None if slots.contexto() == Ausente => None,
            None => return Err(ErrorTerminalRollbackV2::nuevo()),
        };
        self.validar_root_old_terminal(&journal)?;
        let final_j = self
            .observar_journal_versionado()
            .map_err(|_| ErrorTerminalRollbackV2::nuevo())?
            .ok_or_else(ErrorTerminalRollbackV2::nuevo)?;
        if final_j.identidad() != original.identidad()
            || final_j.journal() != original.journal()
            || self
                .observar_siblings_nuevo_flujo()
                .map_err(|_| ErrorTerminalRollbackV2::nuevo())?
                != slots
        {
            return Err(ErrorTerminalRollbackV2::nuevo());
        }
        self.parent_revalidado()
            .map_err(|_| ErrorTerminalRollbackV2::nuevo())?;
        Ok(Terminal {
            journal,
            journal_raw,
            contexto,
        })
    }
    fn validar_root_old_terminal(
        &mut self,
        journal: &JournalV2,
    ) -> Result<(), ErrorTerminalRollbackV2> {
        let raw = self
            .observar_siblings_recovery_v2_raw(journal)
            .map_err(|_| ErrorTerminalRollbackV2::nuevo())?;
        if raw.root() != Some(journal.old()) || raw.backup().is_some() || raw.stage().is_some() {
            return Err(ErrorTerminalRollbackV2::nuevo());
        }
        Ok(())
    }
    fn validar_terminal_vacio(
        &mut self,
        journal: &JournalV2,
    ) -> Result<(), ErrorTerminalRollbackV2> {
        self.validar_root_old_terminal(journal)?;
        let slots = self
            .observar_siblings_nuevo_flujo()
            .map_err(|_| ErrorTerminalRollbackV2::nuevo())?;
        if slots.journal() != Ausente
            || slots.contexto() != Ausente
            || slots.journal_temporal() != Ausente
            || slots.contexto_temporal() != Ausente
            || slots.intent_retiro() != Ausente
            || slots.intent_retiro_temporal() != Ausente
        {
            return Err(ErrorTerminalRollbackV2::nuevo());
        }
        self.parent_revalidado()
            .map_err(|_| ErrorTerminalRollbackV2::nuevo())?;
        Ok(())
    }
    fn nombre_contexto_rollback(&self) -> Result<OsString, ErrorTerminalRollbackV2> {
        crate::transaccion::anclado::contexto_admision::nombre_canonico(
            self._lock
                .ubicacion()
                .ruta()
                .file_name()
                .ok_or_else(ErrorTerminalRollbackV2::nuevo)?,
        )
        .map_err(|_| ErrorTerminalRollbackV2::nuevo())
    }
    fn eliminar_sidecar_rollback(
        &mut self,
        nombre: &OsStr,
        raw: Raw,
    ) -> Result<(), ErrorTerminalRollbackV2> {
        let parent = self
            .parent_revalidado()
            .map_err(|_| ErrorTerminalRollbackV2::nuevo())?;
        #[cfg(windows)]
        let mut file = super::super::intent_retiro::abrir_final_con_delete(&parent, nombre)
            .map_err(|_| ErrorTerminalRollbackV2::nuevo())?;
        #[cfg(not(windows))]
        let mut file = super::super::lector_intent_canonico::abrir_intent_canonico(&parent, nombre)
            .map_err(|_| ErrorTerminalRollbackV2::nuevo())?
            .ok_or_else(ErrorTerminalRollbackV2::nuevo)?;
        verificar(&mut file, raw, self.scope_parent)?;
        self.parent_revalidado()
            .map_err(|_| ErrorTerminalRollbackV2::nuevo())?;
        let _outcome =
            super::super::intent_retiro::retirar_nombre_plataforma(&parent, nombre, file);
        self.parent_revalidado()
            .map_err(|_| ErrorTerminalRollbackV2::nuevo())?;
        Ok(()) // El caller confirma ausencia e inventario, no interpreta el syscall.
    }
}
impl Terminal {
    fn coincide(&self, other: &Self) -> bool {
        self.journal == other.journal
            && self.journal_raw == other.journal_raw
            && self.contexto.as_ref().map(|(_, raw)| raw)
                == other.contexto.as_ref().map(|(_, raw)| raw)
    }
}
fn verificar(
    file: &mut File,
    raw: Raw,
    scope: crate::transaccion::anclado::ScopeFilesystem,
) -> Result<(), ErrorTerminalRollbackV2> {
    let before = file
        .metadata()
        .map_err(|_| ErrorTerminalRollbackV2::nuevo())?;
    if !before.is_file()
        || metadata_es_reparse(&before)
        || before.len() != raw.1
        || identidad_archivo(file).map_err(|_| ErrorTerminalRollbackV2::nuevo())? != raw.0
        || scope_filesystem(file).map_err(|_| ErrorTerminalRollbackV2::nuevo())? != scope
    {
        return Err(ErrorTerminalRollbackV2::nuevo());
    }
    let mut remaining = raw.1;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 8192];
    while remaining > 0 {
        let count = remaining.min(buffer.len() as u64) as usize;
        file.read_exact(&mut buffer[..count])
            .map_err(|_| ErrorTerminalRollbackV2::nuevo())?;
        hash.update(&buffer[..count]);
        remaining -= count as u64;
    }
    let mut extra = [0u8; 1];
    loop {
        match file.read(&mut extra) {
            Ok(0) => break,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            _ => return Err(ErrorTerminalRollbackV2::nuevo()),
        }
    }
    let digest: [u8; 32] = hash.finalize().into();
    if digest != raw.2
        || identidad_archivo(file).map_err(|_| ErrorTerminalRollbackV2::nuevo())? != raw.0
        || scope_filesystem(file).map_err(|_| ErrorTerminalRollbackV2::nuevo())? != scope
    {
        return Err(ErrorTerminalRollbackV2::nuevo());
    }
    Ok(())
}

#[cfg(test)]
#[path = "terminal_tests.rs"]
mod tests;

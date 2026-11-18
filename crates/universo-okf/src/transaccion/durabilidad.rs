//! Puntos de durabilidad del apply: `fsync` de archivos y de directorios.
//!
//! Un `rename`/`link`/`unlink` solo es durable ante una caída del sistema
//! operativo cuando se sincroniza el directorio que contiene la entrada, y el
//! contenido de un archivo solo cuando se sincroniza el propio archivo. Este
//! módulo concentra esas llamadas para que el orden sea auditable:
//!
//! 1. archivos y directorios del stage antes de publicar el journal `Prepared`
//!    y de instalar el stage como root;
//! 2. el directorio padre después de cada cambio de nombres transaccional
//!    (journal, context, intent, renombrados root/backup/stage y retiros).
//!
//! Windows: los archivos se sincronizan con `FlushFileBuffers` (vía
//! `File::sync_all`). Los directorios no: Windows no admite `FlushFileBuffers`
//! sobre los handles de directorio que abre este crate (sin derecho de
//! escritura), y NTFS registra en su journal de metadatos los renombrados. La
//! durabilidad de entradas de directorio en Windows queda sin garantía
//! explícita y está documentada como límite.
use std::{fs::File, io};

/// Lugar del protocolo que pide la sincronización (solo para auditoría y tests).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Punto {
    StageArchivo,
    StageDirectorio,
    RenombrarDirectorio,
    JournalPublicado,
    JournalSucesor,
    JournalTemporalRetirado,
    JournalRetirado,
    ContextoPublicado,
    ContextoRetirado,
    IntentPublicado,
    NombreRetirado,
    BackupEliminado,
    StageEliminado,
}

pub(crate) fn sincronizar_archivo(archivo: &File, punto: Punto) -> io::Result<()> {
    registro::fallo_inyectado(punto)?;
    archivo.sync_all()?;
    registro::anotar(punto);
    Ok(())
}

pub(crate) fn sincronizar_directorio(directorio: &File, punto: Punto) -> io::Result<()> {
    registro::fallo_inyectado(punto)?;
    #[cfg(unix)]
    directorio.sync_all()?;
    #[cfg(not(unix))]
    let _ = directorio;
    registro::anotar(punto);
    Ok(())
}

#[cfg(test)]
pub(crate) mod registro {
    use super::Punto;
    use std::cell::RefCell;

    thread_local! {
        static EVENTOS: RefCell<Vec<Punto>> = const { RefCell::new(Vec::new()) };
        static FALLO: RefCell<Option<Punto>> = const { RefCell::new(None) };
    }

    /// Hace fallar (una vez) la próxima sincronización de `punto` en este hilo.
    pub(crate) fn fallar_una_vez(punto: Punto) {
        FALLO.with(|fallo| *fallo.borrow_mut() = Some(punto));
    }

    pub(super) fn fallo_inyectado(punto: Punto) -> std::io::Result<()> {
        FALLO.with(|fallo| {
            let mut fallo = fallo.borrow_mut();
            if *fallo == Some(punto) {
                *fallo = None;
                Err(std::io::Error::other("fsync inyectado"))
            } else {
                Ok(())
            }
        })
    }

    pub(super) fn anotar(punto: Punto) {
        EVENTOS.with(|eventos| eventos.borrow_mut().push(punto));
    }

    /// Devuelve y vacía los puntos registrados en este hilo.
    pub(crate) fn tomar() -> Vec<Punto> {
        EVENTOS.with(|eventos| std::mem::take(&mut *eventos.borrow_mut()))
    }
}

#[cfg(not(test))]
mod registro {
    #[inline(always)]
    pub(super) fn anotar(_: super::Punto) {}

    #[inline(always)]
    pub(super) fn fallo_inyectado(_: super::Punto) -> std::io::Result<()> {
        Ok(())
    }
}

use super::{DirectorioSiblingAnclado, ErrorRenombrarAnclado, EspacioTransaccionalAnclado};
use super::{mapear_error_directorio, metadata_directorio};
use std::ffi::OsStr;
use std::sync::Arc;

#[path = "renombrar_move_platform.rs"]
mod plataforma;

impl<'lock> EspacioTransaccionalAnclado<'lock> {
    /// Renombra el directorio observado a un basename ausente del mismo parent.
    ///
    /// La fuente queda ligada al nuevo basename al tener éxito; ante error
    /// conserva su nombre y handle para que el llamador pueda inspeccionarla o
    /// reintentar. El lock serializa el precheck y rename entre escritores
    /// cooperantes, pero un proceso externo todavía puede competir con ellos.
    pub(crate) fn renombrar_directorio(
        &mut self,
        fuente: &mut DirectorioSiblingAnclado<'lock>,
        destino: &OsStr,
    ) -> Result<(), ErrorRenombrarAnclado> {
        super::super::validar_componente(&fuente.nombre).map_err(mapear_error_directorio)?;
        super::super::validar_componente(destino).map_err(mapear_error_directorio)?;
        self.validar_parent()?;
        if fuente.identidad_parent != self.identidad_parent
            || !Arc::ptr_eq(&self.autoridad, &fuente.autoridad)
        {
            return Err(ErrorRenombrarAnclado::FuenteNoHermana);
        }
        if !self
            ._lock
            .ubicacion()
            .permite_rename_sibling(&fuente.nombre)
            || !self._lock.ubicacion().permite_rename_sibling(destino)
        {
            return Err(ErrorRenombrarAnclado::FuenteNoHermana);
        }
        if metadata_directorio(&fuente.raiz).map_err(mapear_error_directorio)?
            != fuente.identidad_raiz
        {
            return Err(ErrorRenombrarAnclado::IdentidadNoCoincide);
        }

        plataforma::renombrar_plataforma(
            &self.parent,
            &fuente.nombre,
            destino,
            &fuente.raiz,
            fuente.identidad_raiz,
        )?;
        fuente.nombre = destino.to_os_string();
        Ok(())
    }
}

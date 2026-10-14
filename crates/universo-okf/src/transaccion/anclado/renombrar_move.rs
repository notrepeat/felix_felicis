use super::super::{ErrorDirectorioAnclado, ScopeFilesystem, scope_filesystem};
use super::{DirectorioSiblingAnclado, ErrorRenombrarAnclado, EspacioTransaccionalAnclado};
use super::{mapear_error_directorio, metadata_directorio};
use std::ffi::OsStr;
use std::fs::File;
use std::sync::Arc;

#[path = "renombrar_move_platform.rs"]
mod plataforma;

impl<'lock> EspacioTransaccionalAnclado<'lock> {
    /// Renombra el directorio observado a un basename ausente del mismo parent.
    ///
    /// La fuente queda ligada al nuevo basename solo tras verificar el destino.
    /// Un resultado post-syscall no verificable invalida la capability y conserva
    /// el nombre previo, por lo que no puede reintentarse con ese basename. El
    /// token no cierra carreras externas entre comprobación y operación.
    pub(crate) fn renombrar_directorio(
        &mut self,
        fuente: &mut DirectorioSiblingAnclado<'lock>,
        destino: &OsStr,
    ) -> Result<(), ErrorRenombrarAnclado> {
        self.renombrar_directorio_interno(
            fuente,
            destino,
            &mut scope_filesystem,
            &mut plataforma::renombrar_plataforma,
        )
    }

    #[cfg(test)]
    pub(crate) fn renombrar_directorio_con_probe_y_operacion<S, R>(
        &mut self,
        fuente: &mut DirectorioSiblingAnclado<'lock>,
        destino: &OsStr,
        probe: &mut S,
        renombrar: &mut R,
    ) -> Result<(), ErrorRenombrarAnclado>
    where
        S: FnMut(&File) -> Result<bool, ErrorDirectorioAnclado>,
        R: FnMut(
            &File,
            &OsStr,
            &OsStr,
            &File,
            super::super::IdentidadDirectorio,
        ) -> Result<(), ErrorRenombrarAnclado>,
    {
        let mut probe_scope = |handle: &File| {
            let real = scope_filesystem(handle)?;
            if probe(handle)? {
                Ok(real)
            } else {
                Ok(ScopeFilesystem(real.0 ^ 1))
            }
        };
        self.renombrar_directorio_interno(fuente, destino, &mut probe_scope, renombrar)
    }

    fn renombrar_directorio_interno<S, R>(
        &mut self,
        fuente: &mut DirectorioSiblingAnclado<'lock>,
        destino: &OsStr,
        probe: &mut S,
        renombrar: &mut R,
    ) -> Result<(), ErrorRenombrarAnclado>
    where
        S: FnMut(&File) -> Result<ScopeFilesystem, ErrorDirectorioAnclado>,
        R: FnMut(
            &File,
            &OsStr,
            &OsStr,
            &File,
            super::super::IdentidadDirectorio,
        ) -> Result<(), ErrorRenombrarAnclado>,
    {
        if !fuente.utilizable {
            return Err(ErrorRenombrarAnclado::ResultadoNoVerificado);
        }
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
        validar_scope(&self.parent, self.scope_parent, probe)?;
        if metadata_directorio(&fuente.raiz).map_err(mapear_error_directorio)?
            != fuente.identidad_raiz
        {
            return Err(ErrorRenombrarAnclado::IdentidadNoCoincide);
        }
        validar_scope(&fuente.raiz, self.scope_parent, probe)?;

        renombrar(
            &self.parent,
            &fuente.nombre,
            destino,
            &fuente.raiz,
            fuente.identidad_raiz,
        )?;

        let confirmado = (|| {
            if metadata_directorio(&self.parent).ok() != Some(self.identidad_parent)
                || probe(&self.parent).ok() != Some(self.scope_parent)
                || metadata_directorio(&fuente.raiz).ok() != Some(fuente.identidad_raiz)
                || probe(&fuente.raiz).ok() != Some(self.scope_parent)
            {
                return false;
            }
            let Ok(destino_abierto) =
                super::super::abrir_directorio_hijo_lectura(&self.parent, destino)
            else {
                return false;
            };
            metadata_directorio(&destino_abierto).ok() == Some(fuente.identidad_raiz)
                && probe(&destino_abierto).ok() == Some(self.scope_parent)
        })();
        if !confirmado {
            fuente.utilizable = false;
            return Err(ErrorRenombrarAnclado::ResultadoNoVerificado);
        }
        fuente.nombre = destino.to_os_string();
        Ok(())
    }
}

fn validar_scope<S>(
    handle: &File,
    esperado: ScopeFilesystem,
    probe: &mut S,
) -> Result<(), ErrorRenombrarAnclado>
where
    S: FnMut(&File) -> Result<ScopeFilesystem, ErrorDirectorioAnclado>,
{
    if probe(handle).map_err(|_| ErrorRenombrarAnclado::ScopeNoVerificado)? == esperado {
        Ok(())
    } else {
        Err(ErrorRenombrarAnclado::ScopeNoVerificado)
    }
}

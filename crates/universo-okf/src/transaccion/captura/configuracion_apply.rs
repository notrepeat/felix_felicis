//! Externos acotados antes del lock; internos desde root recuperado y anclado.
use super::*;
use std::fs;

enum FuenteApply {
    Externa(Arc<[u8]>),
    Interna(String),
}
pub(crate) struct ConfiguracionApplyPendiente {
    manifiesto: FuenteApply,
    invariantes: FuenteApply,
}
impl ConfiguracionApplyPendiente {
    pub(crate) fn necesita_root(&self) -> bool {
        matches!(self.manifiesto, FuenteApply::Interna(_))
            || matches!(self.invariantes, FuenteApply::Interna(_))
    }
    pub(crate) fn resolver(
        self,
        entradas: &[EntradaSnapshot],
    ) -> Result<ConfigRawCapturada, ErrorSnapshot> {
        let mut presupuesto = PresupuestoConfiguracion::nuevo(PRESUPUESTO_BYTES_RAW);
        let mut resolver = |fuente: FuenteApply| -> Result<FuenteRaw, ErrorSnapshot> {
            match fuente {
                FuenteApply::Externa(bytes) => Ok(FuenteRaw {
                    bytes: cobrar_configuracion(&mut presupuesto, bytes)?,
                    ruta_root: None,
                }),
                FuenteApply::Interna(path) => {
                    let mut matches = entradas.iter().filter(|entry| {
                        entry.ruta() == path
                            || cfg!(windows) && entry.ruta().eq_ignore_ascii_case(&path)
                    });
                    let entrada = matches.next().ok_or(ErrorSnapshot::Io)?;
                    if matches.next().is_some() {
                        return Err(ErrorSnapshot::RutaNoRepresentable);
                    }
                    let bytes = entrada.bytes().ok_or(ErrorSnapshot::TipoNoAdmitido)?;
                    presupuesto.cobrar(bytes.len())?;
                    Ok(FuenteRaw {
                        bytes: Arc::from(bytes),
                        ruta_root: Some(entrada.ruta().to_owned()),
                    })
                }
            }
        };
        let manifest = resolver(self.manifiesto)?;
        let invariants = resolver(self.invariantes)?;
        ConfigRawCapturada::nueva(
            manifest.bytes,
            invariants.bytes,
            manifest.ruta_root,
            invariants.ruta_root,
        )
    }
}
pub(crate) fn capturar_configuracion_para_apply(
    raiz: &Path,
    manifiesto: &Path,
    invariantes: &Path,
) -> Result<ConfiguracionApplyPendiente, ErrorSnapshot> {
    capturar_con_limite(raiz, manifiesto, invariantes, PRESUPUESTO_BYTES_RAW)
}
fn capturar_con_limite(
    raiz: &Path,
    manifiesto: &Path,
    invariantes: &Path,
    limite: usize,
) -> Result<ConfiguracionApplyPendiente, ErrorSnapshot> {
    let mut cache = HashMap::<PathBuf, Arc<[u8]>>::new();
    let mut presupuesto = PresupuestoConfiguracion::nuevo(limite);
    let mut capturar = |path: &Path| -> Result<FuenteApply, ErrorSnapshot> {
        let absoluta = ruta_absoluta_lexica(path)?;
        let real = match fs::canonicalize(&absoluta) {
            Ok(real) => {
                validar_archivo_sin_enlaces(&absoluta)?;
                real
            }
            Err(_) => {
                if let Some(relative) = relativa_interna_prospectiva(raiz, &absoluta)? {
                    return Ok(FuenteApply::Interna(relative));
                }
                return Err(ErrorSnapshot::Io);
            }
        };
        if real.starts_with(raiz) {
            return Ok(FuenteApply::Interna(ruta_relativa(raiz, &real)?));
        }
        let bytes = if let Some(bytes) = cache.get(&real) {
            Arc::clone(bytes)
        } else {
            let bytes: Arc<[u8]> =
                leer_archivo_regular_limitado(&absoluta, presupuesto.maximo_lectura()?)?.into();
            cache.insert(real, Arc::clone(&bytes));
            bytes
        };
        Ok(FuenteApply::Externa(cobrar_configuracion(
            &mut presupuesto,
            bytes,
        )?))
    };
    Ok(ConfiguracionApplyPendiente {
        manifiesto: capturar(manifiesto)?,
        invariantes: capturar(invariantes)?,
    })
}
fn relativa_interna_prospectiva(raiz: &Path, path: &Path) -> Result<Option<String>, ErrorSnapshot> {
    if path.starts_with(raiz) {
        return ruta_relativa(raiz, path).map(Some);
    }
    #[cfg(windows)]
    {
        // Normaliza sólo prefijos de Win32 y caso ASCII para ubicar la fuente
        // prospectiva. Después recovery exige un único nombre inventariado.
        let normal = |p: &Path| -> Result<String, ErrorSnapshot> {
            let text = p
                .to_str()
                .ok_or(ErrorSnapshot::RutaNoUtf8)?
                .replace('\\', "/");
            Ok(text.strip_prefix("//?/").unwrap_or(&text).to_owned())
        };
        let root = normal(raiz)?;
        let absolute = normal(path)?;
        if absolute.len() > root.len()
            && absolute
                .get(..root.len())
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case(&root))
            && absolute.as_bytes().get(root.len()) == Some(&b'/')
        {
            return Ok(Some(absolute[root.len() + 1..].to_owned()));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn alias_misma_fuente_cobra_ambos_campos_y_limite_inclusivo() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("bundle");
        fs::create_dir(&root).unwrap();
        let file = dir.path().join("config");
        let alias = dir.path().join(".").join("config");
        fs::write(&file, b"12345678").unwrap();
        let captured = capturar_con_limite(&root, &file, &alias, 16)
            .unwrap()
            .resolver(&[])
            .unwrap();
        assert_eq!(captured.manifiesto_raw(), b"12345678");
        assert_eq!(captured.invariantes_raw(), b"12345678");
        fs::write(&file, b"123456789").unwrap();
        assert!(matches!(
            capturar_con_limite(&root, &file, &alias, 16),
            Err(ErrorSnapshot::LimiteInventarioExcedido)
        ));
    }
    #[test]
    fn fuentes_internas_se_difieren_con_root_ausente() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("bundle");
        let pending = capturar_configuracion_para_apply(
            &root,
            &root.join("manifest.toml"),
            &root.join("invariants.sx"),
        )
        .unwrap();
        assert!(pending.necesita_root());
        assert!(pending.resolver(&[]).is_err());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
    }
    #[cfg(windows)]
    #[test]
    fn prospectiva_windows_admite_prefijo_verbatim_y_alias_de_caso() {
        assert_eq!(
            relativa_interna_prospectiva(
                Path::new(r"\\?\C:\Root"),
                Path::new(r"c:\root\manifest.toml")
            )
            .unwrap()
            .as_deref(),
            Some("manifest.toml")
        );
    }
}

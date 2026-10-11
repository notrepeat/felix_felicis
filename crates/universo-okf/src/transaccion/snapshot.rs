//! Identidad raw versionada y token opaco de snapshots transaccionales.
//!
//! El token usa SHA-256 sobre framing etiquetado. `Debug` y `Display` omiten
//! el digest; `to_hex` es la salida explícita para el flujo opt-in del CLI.

use std::fmt;
use std::sync::Arc;

use sha2::{Digest, Sha256};

use super::journal::RawTreeDigest;

const DOMAIN: &[u8] = b"FELIX-SNAPSHOT-v1";
const CAPTURE_POLICY: &[u8] =
    b"files-and-directories-v1;utf8-exact;slash-paths;byte-order-no-unicode-normalization";
const RAW_TREE_DOMAIN: &[u8] = b"FELIX-RAW-TREE\0v1";

/// Token opaco de identidad de snapshot. La representación está redactada
/// salvo que el llamador solicite explícitamente [`SnapshotToken::to_hex`].
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct SnapshotToken([u8; 32]);

impl SnapshotToken {
    /// Codificación hexadecimal minúscula para un flujo que pidió el token.
    pub fn to_hex(self) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut salida = String::with_capacity(self.0.len() * 2);
        for byte in self.0 {
            salida.push(HEX[(byte >> 4) as usize] as char);
            salida.push(HEX[(byte & 0x0f) as usize] as char);
        }
        salida
    }
}

impl fmt::Debug for SnapshotToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SnapshotToken([redacted])")
    }
}

impl fmt::Display for SnapshotToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[snapshot token redacted]")
    }
}

/// Tipo de entrada del inventario incluido en el token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TipoEntradaSnapshot {
    /// Archivo regular; se incluyen todos sus bytes, incluso si está vacío.
    Archivo,
    /// Directorio; su entrada hace visibles también los directorios vacíos.
    Directorio,
}

/// Entrada raw del root, almacenada de forma inmutable.
#[derive(Clone)]
pub struct EntradaSnapshot {
    ruta: String,
    tipo: TipoEntradaSnapshot,
    bytes: Option<Arc<[u8]>>,
}

impl EntradaSnapshot {
    /// Entrada de archivo con contenido raw.
    pub fn archivo(ruta: impl Into<String>, bytes: impl Into<Arc<[u8]>>) -> Self {
        Self {
            ruta: ruta.into(),
            tipo: TipoEntradaSnapshot::Archivo,
            bytes: Some(bytes.into()),
        }
    }

    /// Entrada de directorio; el root no se representa, pero sí sus subdirectorios.
    pub fn directorio(ruta: impl Into<String>) -> Self {
        Self {
            ruta: ruta.into(),
            tipo: TipoEntradaSnapshot::Directorio,
            bytes: None,
        }
    }

    /// Path relativo normalizado, con `/`, preservando caso y Unicode exactos.
    pub fn ruta(&self) -> &str {
        &self.ruta
    }

    /// Clase de entrada del inventario.
    pub fn tipo(&self) -> TipoEntradaSnapshot {
        self.tipo
    }

    /// Bytes crudos de un archivo; un directorio no tiene contenido.
    pub fn bytes(&self) -> Option<&[u8]> {
        self.bytes.as_deref()
    }
}

impl fmt::Debug for EntradaSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("EntradaSnapshot { .. }")
    }
}

/// Error estable y redactado del cálculo/captura de snapshot.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ErrorSnapshot {
    /// No se pudo leer o inspeccionar una entrada.
    Io,
    /// El token actual no coincide con la identidad esperada.
    Stale,
    /// La identidad contiene una ruta no representable como UTF-8.
    RutaNoUtf8,
    /// La ruta no admite representación portable con separadores `/`.
    RutaNoRepresentable,
    /// La entrada es un enlace simbólico o un reparse point.
    EnlaceNoPermitido,
    /// La entrada no es un archivo regular o un directorio.
    TipoNoAdmitido,
    /// El manifiesto no se pudo leer, decodificar o parsear.
    ManifiestoInvalido,
    /// El bundle no se pudo cargar desde los bytes capturados.
    BundleInvalido,
    /// El programa de invariantes no se pudo parsear desde los bytes capturados.
    InvariantesInvalidas,
    /// Un tamaño no cabe en el framing u64.
    LongitudExcedida,
    /// La ruta raíz no identifica un directorio.
    RaizInvalida,
}

impl ErrorSnapshot {
    /// Código estable sin paths, valores, bytes ni digest.
    pub const fn codigo(self) -> &'static str {
        match self {
            Self::Io => "snapshot_io",
            Self::Stale => "stale",
            Self::RutaNoUtf8 => "snapshot_path_not_utf8",
            Self::RutaNoRepresentable => "snapshot_path_not_representable",
            Self::EnlaceNoPermitido => "snapshot_link_not_allowed",
            Self::TipoNoAdmitido => "snapshot_entry_type_not_allowed",
            Self::ManifiestoInvalido => "snapshot_manifest_invalid",
            Self::BundleInvalido => "snapshot_bundle_invalid",
            Self::InvariantesInvalidas => "snapshot_invariants_invalid",
            Self::LongitudExcedida => "snapshot_length_overflow",
            Self::RaizInvalida => "snapshot_root_invalid",
        }
    }
}

/// Compara una identidad esperada con la capturada sin exponer ninguno de los
/// tokens en caso de diferencia.
pub(crate) fn comparar_snapshot_esperado(
    esperado: SnapshotToken,
    actual: SnapshotToken,
) -> Result<(), ErrorSnapshot> {
    if esperado == actual {
        Ok(())
    } else {
        Err(ErrorSnapshot::Stale)
    }
}

impl fmt::Debug for ErrorSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ErrorSnapshot")
            .field(&self.codigo())
            .finish()
    }
}

impl fmt::Display for ErrorSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.codigo())
    }
}

impl std::error::Error for ErrorSnapshot {}

fn hash_len(hash: &mut Sha256, len: usize) -> Result<(), ErrorSnapshot> {
    let len = u64::try_from(len).map_err(|_| ErrorSnapshot::LongitudExcedida)?;
    hash.update(len.to_be_bytes());
    Ok(())
}

fn hash_field(hash: &mut Sha256, name: &[u8], value: &[u8]) -> Result<(), ErrorSnapshot> {
    hash_len(hash, name.len())?;
    hash.update(name);
    hash_len(hash, value.len())?;
    hash.update(value);
    Ok(())
}

/// Calcula la huella sobre buffers raw. Las entradas se ordenan por bytes de
/// ruta UTF-8; la captura de disco valida que esas rutas sean únicas y portables.
pub fn calcular_token_snapshot(
    version_motor: &str,
    policy_id: &str,
    manifiesto_raw: &[u8],
    invariantes_raw: &[u8],
    entradas: &[EntradaSnapshot],
) -> Result<SnapshotToken, ErrorSnapshot> {
    let mut ordenadas: Vec<_> = entradas.iter().collect();
    ordenadas.sort_by(|a, b| a.ruta.as_bytes().cmp(b.ruta.as_bytes()));

    let mut hash = Sha256::new();
    hash_field(&mut hash, b"domain", DOMAIN)?;
    hash_field(&mut hash, b"engine-version", version_motor.as_bytes())?;
    hash_field(&mut hash, b"policy-id", policy_id.as_bytes())?;
    hash_field(&mut hash, b"capture-policy", CAPTURE_POLICY)?;
    hash_field(&mut hash, b"manifest-raw", manifiesto_raw)?;
    hash_field(&mut hash, b"invariants-raw", invariantes_raw)?;
    hash_len(&mut hash, ordenadas.len())?;
    for entrada in ordenadas {
        let kind = match entrada.tipo {
            TipoEntradaSnapshot::Archivo => b"file".as_slice(),
            TipoEntradaSnapshot::Directorio => b"directory".as_slice(),
        };
        hash_field(&mut hash, b"entry", kind)?;
        hash_field(&mut hash, b"path", entrada.ruta.as_bytes())?;
        if let Some(bytes) = &entrada.bytes {
            hash_field(&mut hash, b"content", bytes)?;
        }
    }

    Ok(SnapshotToken(hash.finalize().into()))
}

/// Calcula la identidad raw del árbol sin agregar contexto externo ni metadatos.
///
/// El framing es independiente del token de snapshot y ordena por bytes UTF-8
/// de ruta. Se validan las rutas aquí porque la API recibe una slice que puede
/// haberse construido fuera del inventario del filesystem.
pub(crate) fn calcular_digest_raw_tree(
    entradas: &[EntradaSnapshot],
) -> Result<RawTreeDigest, ErrorSnapshot> {
    let mut ordenadas: Vec<_> = entradas.iter().collect();
    ordenadas.sort_by(|a, b| a.ruta.as_bytes().cmp(b.ruta.as_bytes()));

    for (indice, entrada) in ordenadas.iter().enumerate() {
        validar_ruta_raw_tree(entrada.ruta())?;
        if indice > 0 && ordenadas[indice - 1].ruta() == entrada.ruta() {
            return Err(ErrorSnapshot::RutaNoRepresentable);
        }
    }

    let mut hash = Sha256::new();
    hash.update(RAW_TREE_DOMAIN);
    hash_len(&mut hash, ordenadas.len())?;
    for entrada in ordenadas {
        hash.update([match entrada.tipo() {
            TipoEntradaSnapshot::Directorio => b'D',
            TipoEntradaSnapshot::Archivo => b'F',
        }]);
        hash_len(&mut hash, entrada.ruta().len())?;
        hash.update(entrada.ruta().as_bytes());

        match entrada.tipo() {
            TipoEntradaSnapshot::Directorio if entrada.bytes().is_none() => {}
            TipoEntradaSnapshot::Directorio => return Err(ErrorSnapshot::TipoNoAdmitido),
            TipoEntradaSnapshot::Archivo => {
                let contenido = entrada.bytes().ok_or(ErrorSnapshot::TipoNoAdmitido)?;
                hash_len(&mut hash, contenido.len())?;
                hash.update(contenido);
            }
        }
    }

    Ok(RawTreeDigest::from_digest(hash.finalize().into()))
}

fn validar_ruta_raw_tree(ruta: &str) -> Result<(), ErrorSnapshot> {
    if ruta.is_empty() || ruta.contains('\\') || ruta.contains('\0') {
        return Err(ErrorSnapshot::RutaNoRepresentable);
    }
    if ruta
        .split('/')
        .any(|componente| componente.is_empty() || matches!(componente, "." | ".."))
    {
        return Err(ErrorSnapshot::RutaNoRepresentable);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token(
        version: &str,
        policy: &str,
        manifest: &[u8],
        invariants: &[u8],
        entries: &[EntradaSnapshot],
    ) -> SnapshotToken {
        calcular_token_snapshot(version, policy, manifest, invariants, entries).unwrap()
    }

    fn archivo(path: &str, bytes: &[u8]) -> EntradaSnapshot {
        EntradaSnapshot::archivo(path, Arc::<[u8]>::from(bytes))
    }

    #[test]
    fn sha256_del_framing_es_determinista_y_redactado_por_defecto() {
        let entries = [archivo("n/á.md", b"CANARY-CONTENT")];
        let result = token("engine/1", "policy/1", b"manifest", b"rules", &entries);
        assert_eq!(
            result,
            token("engine/1", "policy/1", b"manifest", b"rules", &entries)
        );
        assert_eq!(
            result.to_hex(),
            "6cffaa8a4d9cfcf5e298d8f73d72d6ada739b383d4aa9bdf21d30bba5577fb72"
        );
        assert_eq!(result.to_hex().len(), 64);
        assert!(!format!("{result:?}").contains(&result.to_hex()));
        assert!(!result.to_string().contains(&result.to_hex()));
        assert!(!format!("{result:?}").contains("CANARY"));
    }

    #[test]
    fn cada_campo_y_entrada_del_framing_cambia_el_token() {
        let base = [archivo("a", b"bc")];
        let esperado = token("1", "p", b"m", b"i", &base);
        assert_ne!(esperado, token("2", "p", b"m", b"i", &base));
        assert_ne!(esperado, token("1", "q", b"m", b"i", &base));
        assert_ne!(esperado, token("1", "p", b"n", b"i", &base));
        assert_ne!(esperado, token("1", "p", b"m", b"j", &base));
        assert_ne!(
            esperado,
            token("1", "p", b"m", b"i", &[archivo("a", b"bd")])
        );
        assert_ne!(
            esperado,
            token("1", "p", b"m", b"i", &[archivo("c", b"bc")])
        );
        assert_ne!(esperado, token("1", "p", b"m", b"i", &[]));
    }

    #[test]
    fn framing_etiquetado_separa_campos_que_podrian_concatenarse_igual() {
        assert_ne!(
            token("ab", "c", b"m", b"i", &[]),
            token("a", "bc", b"m", b"i", &[])
        );
        assert_ne!(
            token("1", "p", b"m", b"i", &[archivo("a", b"")]),
            token("1", "p", b"m", b"i", &[EntradaSnapshot::directorio("a")])
        );
    }

    #[test]
    fn las_entradas_se_ordenan_por_bytes_utf8_sin_normalizacion_unicode() {
        let a = archivo("z", b"z");
        let b = archivo("a", b"a");
        assert_eq!(
            token("1", "p", b"m", b"i", &[a.clone(), b.clone()]),
            token("1", "p", b"m", b"i", &[b, a])
        );
        assert_ne!(
            token("1", "p", b"m", b"i", &[archivo("é", b"x")]),
            token("1", "p", b"m", b"i", &[archivo("e\u{301}", b"x")])
        );
    }

    #[test]
    fn cas_puro_acepta_match_y_devuelve_stale_fijo_y_redactado() {
        let base = token("engine/1", "policy/1", b"MANIFEST-CANARY", b"rules", &[]);
        assert_eq!(comparar_snapshot_esperado(base, base), Ok(()));

        let distinto = token("engine/2", "policy/1", b"MANIFEST-CANARY", b"rules", &[]);
        let error = comparar_snapshot_esperado(base, distinto).unwrap_err();
        assert_eq!(error, ErrorSnapshot::Stale);
        assert_eq!(error.codigo(), "stale");
        assert_eq!(error.to_string(), "stale");
        assert!(!format!("{error:?}").contains(&base.to_hex()));
        assert!(!format!("{error:?}").contains(&distinto.to_hex()));
        assert!(!format!("{error:?}").contains("MANIFEST-CANARY"));
    }

    #[test]
    fn raw_tree_sha256_v1_coincide_con_los_golden_vectors() {
        let casos = [
            (
                Vec::new(),
                "d8da747cd5c5b098a99bcda58579805009bf4a30330a2f5a68c41a4a8dcaa0d4",
            ),
            (
                vec![EntradaSnapshot::directorio("d")],
                "930bc81711f7b49fc32d54de8495e56003cf7f15c1988007e847e31ae26e0adf",
            ),
            (
                vec![EntradaSnapshot::archivo("d", Arc::<[u8]>::from([]))],
                "2c9b5c2fb775b77f46c7af2b3d956688807c6d479e00ddb2fda7ac71bf033647",
            ),
            (
                vec![archivo("a", b"bc")],
                "e7bdbcc08e6537f54d828106109839223eb40076ba679455138235d077ebb1dd",
            ),
            (
                vec![EntradaSnapshot::archivo(
                    "n/á.md",
                    Arc::<[u8]>::from([0x00, 0xff]),
                )],
                "abacd0996831e8d000b59ef712ed96c0f056efe1faaa63de241acb5a7b4ce6a2",
            ),
            (
                vec![
                    EntradaSnapshot::directorio("a"),
                    EntradaSnapshot::archivo("b", Arc::<[u8]>::from([b'x'])),
                ],
                "0c4adc7e2e9ed6a44b253f3a97b4116930c605df4e4aebfa0705ff617bd53e7a",
            ),
        ];

        for (entradas, esperado) in casos {
            let esperado = RawTreeDigest::from_hex(esperado).unwrap();
            assert!(calcular_digest_raw_tree(&entradas).unwrap() == esperado);
        }
    }

    #[test]
    fn raw_tree_sha256_v1_ordena_bytes_utf8_y_rechaza_rutas_invalidas_o_duplicadas() {
        let entradas = [archivo("z", b"z"), archivo("a", b"a")];
        let invertidas = [archivo("a", b"a"), archivo("z", b"z")];
        assert!(
            calcular_digest_raw_tree(&entradas).unwrap()
                == calcular_digest_raw_tree(&invertidas).unwrap()
        );

        for ruta in [
            "", "/a", "a/", "a//b", "../a", "a/..", "a/./b", "a\\b", "a\0b",
        ] {
            let entrada = archivo(ruta, b"x");
            assert!(calcular_digest_raw_tree(&[entrada]).is_err());
        }

        let duplicadas = [EntradaSnapshot::directorio("a"), archivo("a", b"x")];
        assert!(calcular_digest_raw_tree(&duplicadas).is_err());

        let mayuscula = archivo("A", b"x");
        let minuscula = archivo("a", b"x");
        assert!(calcular_digest_raw_tree(&[mayuscula, minuscula]).is_ok());

        let compuesto = archivo("e\u{301}", b"x");
        let precompuesto = archivo("é", b"x");
        assert!(
            calcular_digest_raw_tree(&[compuesto.clone()]).unwrap()
                != calcular_digest_raw_tree(&[precompuesto.clone()]).unwrap()
        );
        assert!(calcular_digest_raw_tree(&[precompuesto, compuesto]).is_ok());
    }
}

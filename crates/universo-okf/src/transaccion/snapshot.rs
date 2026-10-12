//! Identidad raw versionada y token opaco de snapshots transaccionales.
//!
//! El token usa SHA-256 sobre framing etiquetado. `Debug` y `Display` omiten
//! el digest; `to_hex` es la salida explícita para el flujo opt-in del CLI.

use std::fmt;
use std::sync::Arc;

use sha2::{Digest, Sha256};

use super::journal::RawTreeDigest;
use super::limites::LimitesInventario;

const DOMAIN: &[u8] = b"FELIX-SNAPSHOT-v1";
const CAPTURE_POLICY: &[u8] =
    b"files-and-directories-v1;utf8-exact;slash-paths;byte-order-no-unicode-normalization";
const RAW_TREE_DOMAIN: &[u8] = b"FELIX-RAW-TREE\0v1";

/// Token opaco de identidad de snapshot. La representación está redactada
/// salvo que el llamador solicite explícitamente [`SnapshotToken::to_hex`].
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct SnapshotToken([u8; 32]);

impl SnapshotToken {
    /// Decodifica una representación hexadecimal canónica en minúsculas.
    ///
    /// Solo acepta exactamente 64 bytes ASCII entre `0-9` y `a-f`; el error
    /// no conserva ni expone el texto recibido.
    pub fn from_hex(hex: &str) -> Result<Self, ErrorParseSnapshotToken> {
        let bytes = hex.as_bytes();
        if bytes.len() != 64 {
            return Err(ErrorParseSnapshotToken);
        }

        let mut token = [0; 32];
        for (indice, par) in bytes.chunks_exact(2).enumerate() {
            let alto = nibble_hex_minuscula(par[0]).ok_or(ErrorParseSnapshotToken)?;
            let bajo = nibble_hex_minuscula(par[1]).ok_or(ErrorParseSnapshotToken)?;
            token[indice] = (alto << 4) | bajo;
        }

        Ok(Self(token))
    }

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

fn nibble_hex_minuscula(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

/// Error fijo y redactado al parsear un token de snapshot.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ErrorParseSnapshotToken;

impl ErrorParseSnapshotToken {
    /// Código estable sin incluir el texto recibido.
    pub const fn codigo(self) -> &'static str {
        "snapshot_token_invalid"
    }
}

impl fmt::Debug for ErrorParseSnapshotToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ErrorParseSnapshotToken")
            .field(&self.codigo())
            .finish()
    }
}

impl fmt::Display for ErrorParseSnapshotToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.codigo())
    }
}

impl std::error::Error for ErrorParseSnapshotToken {}

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
///
/// Este enum público es exhaustivo; agregar variantes puede requerir cambios
/// en `match` exhaustivos de consumidores. El crate sigue en pre-1.0.
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
    /// El inventario excedió un límite estructural o de contenido.
    LimiteInventarioExcedido,
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
            Self::LimiteInventarioExcedido => "snapshot_resource_limit",
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
/// ruta UTF-8. Rechaza las rutas vacías, las que tengan un componente vacío
/// (por `/` inicial, final o repetida), un componente `.` o `..`, o contengan
/// una barra inversa o NUL. También rechaza rutas exactamente duplicadas,
/// comparadas sin normalizar caso ni Unicode; todos esos casos devuelven
/// [`ErrorSnapshot::RutaNoRepresentable`].
pub fn calcular_token_snapshot(
    version_motor: &str,
    policy_id: &str,
    manifiesto_raw: &[u8],
    invariantes_raw: &[u8],
    entradas: &[EntradaSnapshot],
) -> Result<SnapshotToken, ErrorSnapshot> {
    calcular_token_snapshot_con_limites(
        version_motor,
        policy_id,
        manifiesto_raw,
        invariantes_raw,
        entradas,
        LimitesInventario::CONTRATO,
    )
}

fn calcular_token_snapshot_con_limites(
    version_motor: &str,
    policy_id: &str,
    manifiesto_raw: &[u8],
    invariantes_raw: &[u8],
    entradas: &[EntradaSnapshot],
    limites: LimitesInventario,
) -> Result<SnapshotToken, ErrorSnapshot> {
    let ordenadas = ordenar_y_validar_rutas_con_limites(entradas, limites)?;

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
    calcular_digest_raw_tree_con_limites(entradas, LimitesInventario::CONTRATO)
}

fn calcular_digest_raw_tree_con_limites(
    entradas: &[EntradaSnapshot],
    limites: LimitesInventario,
) -> Result<RawTreeDigest, ErrorSnapshot> {
    let ordenadas = ordenar_y_validar_rutas_con_limites(entradas, limites)?;

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

fn ordenar_y_validar_rutas_con_limites(
    entradas: &[EntradaSnapshot],
    limites: LimitesInventario,
) -> Result<Vec<&EntradaSnapshot>, ErrorSnapshot> {
    validar_presupuesto_arbol(entradas, limites)?;

    let mut ordenadas: Vec<_> = entradas.iter().collect();
    ordenadas.sort_by(|a, b| a.ruta().as_bytes().cmp(b.ruta().as_bytes()));

    for (indice, entrada) in ordenadas.iter().enumerate() {
        if indice > 0 && ordenadas[indice - 1].ruta() == entrada.ruta() {
            return Err(ErrorSnapshot::RutaNoRepresentable);
        }
    }

    Ok(ordenadas)
}

fn validar_presupuesto_arbol(
    entradas: &[EntradaSnapshot],
    limites: LimitesInventario,
) -> Result<(), ErrorSnapshot> {
    if entradas.len() > limites.entradas {
        return Err(ErrorSnapshot::LimiteInventarioExcedido);
    }

    let mut bytes_raw = 0;
    let mut bytes_rutas = 0;
    for entrada in entradas {
        let ruta = entrada.ruta();
        validar_ruta_snapshot(ruta)?;

        let profundidad = ruta.bytes().try_fold(1usize, |profundidad, byte| {
            if byte == b'/' {
                profundidad
                    .checked_add(1)
                    .ok_or(ErrorSnapshot::LimiteInventarioExcedido)
            } else {
                Ok(profundidad)
            }
        })?;
        if profundidad > limites.profundidad {
            return Err(ErrorSnapshot::LimiteInventarioExcedido);
        }

        bytes_rutas = sumar_presupuesto(bytes_rutas, ruta.len(), limites.bytes_rutas)?;
        let bytes_entrada = entrada.bytes().map_or(0, <[u8]>::len);
        bytes_raw = sumar_presupuesto(bytes_raw, bytes_entrada, limites.bytes_raw)?;
    }

    Ok(())
}

fn sumar_presupuesto(
    total: usize,
    incremento: usize,
    limite: usize,
) -> Result<usize, ErrorSnapshot> {
    total
        .checked_add(incremento)
        .filter(|total| *total <= limite)
        .ok_or(ErrorSnapshot::LimiteInventarioExcedido)
}

fn validar_ruta_snapshot(ruta: &str) -> Result<(), ErrorSnapshot> {
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
    use proptest::prelude::*;

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

    fn limites_prueba(
        bytes_raw: usize,
        entradas: usize,
        profundidad: usize,
        bytes_rutas: usize,
    ) -> LimitesInventario {
        LimitesInventario {
            bytes_raw,
            entradas,
            profundidad,
            bytes_rutas,
        }
    }

    fn assert_error_limite<T>(resultado: Result<T, ErrorSnapshot>) {
        let error = resultado
            .err()
            .expect("el árbol debe exceder el presupuesto");
        assert_eq!(error, ErrorSnapshot::LimiteInventarioExcedido);
        assert_eq!(error.codigo(), "snapshot_resource_limit");
        assert_eq!(error.to_string(), "snapshot_resource_limit");
        assert_eq!(
            format!("{error:?}"),
            "ErrorSnapshot(\"snapshot_resource_limit\")"
        );
        assert!(!error.to_string().contains("CANARY"));
        assert!(!format!("{error:?}").contains("CANARY"));
    }

    fn assert_presupuesto_arbol(
        entradas: &[EntradaSnapshot],
        limites: LimitesInventario,
        aceptado: bool,
    ) {
        let token = calcular_token_snapshot_con_limites(
            "CANARY-engine",
            "CANARY-policy",
            b"CANARY-manifest",
            b"CANARY-invariants",
            entradas,
            limites,
        );
        let digest = calcular_digest_raw_tree_con_limites(entradas, limites);

        if aceptado {
            assert!(token.is_ok(), "el token debe aceptar el límite inclusivo");
            assert!(digest.is_ok(), "el digest debe aceptar el límite inclusivo");
        } else {
            assert_error_limite(token);
            assert_error_limite(digest);
        }
    }

    #[test]
    fn parsear_token_hex_acepta_vector_conocido_y_ceros() {
        let cero = SnapshotToken::from_hex(&"0".repeat(64)).unwrap();
        assert_eq!(cero.0, [0; 32]);
        assert_eq!(cero.to_hex(), "0".repeat(64));

        let vector = "6cffaa8a4d9cfcf5e298d8f73d72d6ada739b383d4aa9bdf21d30bba5577fb72";
        let calculado = token(
            "engine/1",
            "policy/1",
            b"manifest",
            b"rules",
            &[archivo("n/á.md", b"CANARY-CONTENT")],
        );
        assert_eq!(SnapshotToken::from_hex(vector), Ok(calculado));
    }

    #[test]
    fn parsear_token_hex_rechaza_representaciones_no_canonicas() {
        let invalidos = [
            "0".repeat(63),
            "0".repeat(65),
            format!("{}A", "0".repeat(63)),
            format!("{}g{}", "0".repeat(31), "0".repeat(32)),
            format!("0x{}", "0".repeat(62)),
            format!("{} {}", "0".repeat(31), "0".repeat(32)),
            format!("{}é{}", "0".repeat(31), "0".repeat(31)),
            format!("{}\0{}", "0".repeat(31), "0".repeat(32)),
            format!("{}-{}", "0".repeat(31), "0".repeat(32)),
        ];

        for invalido in invalidos {
            assert_eq!(
                SnapshotToken::from_hex(&invalido),
                Err(ErrorParseSnapshotToken),
                "la entrada no canónica debe rechazarse"
            );
        }
    }

    #[test]
    fn error_de_parseo_es_estable_y_no_revela_entrada() {
        let canario = format!("{}CANARY{}", "0".repeat(20), "0".repeat(38));
        let error = SnapshotToken::from_hex(&canario).unwrap_err();

        assert_eq!(error.codigo(), "snapshot_token_invalid");
        assert_eq!(error.to_string(), "snapshot_token_invalid");
        assert_eq!(
            format!("{error:?}"),
            "ErrorParseSnapshotToken(\"snapshot_token_invalid\")"
        );
        assert!(!error.to_string().contains("CANARY"));
        assert!(!format!("{error:?}").contains("CANARY"));
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        #[test]
        fn token_hex_redondea_bytes(bytes in any::<[u8; 32]>()) {
            let token = SnapshotToken(bytes);
            let hex = token.to_hex();
            prop_assert_eq!(SnapshotToken::from_hex(&hex), Ok(token));
        }

        #[test]
        fn parsear_hex_canonico_acepta_64_caracteres(entrada in prop::collection::vec(
            prop::sample::select(b"0123456789abcdef".to_vec()),
            64
        )) {
            let entrada = String::from_utf8(entrada).expect("el alfabeto es ASCII");
            let token = SnapshotToken::from_hex(&entrada);
            prop_assert!(token.is_ok());
            prop_assert_eq!(token.unwrap().to_hex(), entrada);
        }

        #[test]
        fn parsear_string_arbitrario_no_panic_y_solo_acepta_canonico(entrada in any::<String>()) {
            if let Ok(token) = SnapshotToken::from_hex(&entrada) {
                prop_assert_eq!(entrada.len(), 64);
                prop_assert!(entrada.bytes().all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f')));
                prop_assert_eq!(token.to_hex(), entrada);
            }
        }
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
    fn token_rechaza_rutas_no_representables_y_duplicadas_con_error_redactado() {
        for ruta in [
            "", "/a", "a/", "a//b", ".", "..", "./a", "a/.", "../a", "a/..", "a/../b", "a\\b",
            "a\0b",
        ] {
            assert_eq!(
                calcular_token_snapshot("1", "p", b"m", b"i", &[archivo(ruta, b"x")]),
                Err(ErrorSnapshot::RutaNoRepresentable),
                "la ruta inválida debe rechazarse: {ruta:?}"
            );
        }

        let duplicadas = [archivo("CANARY/path", b"x"), archivo("CANARY/path", b"y")];
        let error = calcular_token_snapshot("1", "p", b"m", b"i", &duplicadas).unwrap_err();
        assert_eq!(error, ErrorSnapshot::RutaNoRepresentable);
        assert_eq!(error.codigo(), "snapshot_path_not_representable");
        assert_eq!(error.to_string(), "snapshot_path_not_representable");
        assert_eq!(
            format!("{error:?}"),
            "ErrorSnapshot(\"snapshot_path_not_representable\")"
        );
        assert!(!error.to_string().contains("CANARY"));
        assert!(!format!("{error:?}").contains("CANARY"));
    }

    #[test]
    fn presupuesto_del_arbol_acepta_limites_inclusivos_y_rechaza_mas_uno() {
        assert_presupuesto_arbol(
            &[archivo("CANARY/a", b"")],
            limites_prueba(0, 1, 10, usize::MAX),
            true,
        );
        assert_presupuesto_arbol(
            &[archivo("CANARY/a", b""), archivo("CANARY/b", b"")],
            limites_prueba(0, 1, 10, usize::MAX),
            false,
        );

        assert_presupuesto_arbol(
            &[archivo("é", b""), archivo("x", b"")],
            limites_prueba(0, 10, 10, 3),
            true,
        );
        assert_presupuesto_arbol(
            &[archivo("é", b""), archivo("xy", b"")],
            limites_prueba(0, 10, 10, 3),
            false,
        );

        assert_presupuesto_arbol(
            &[archivo("a/b/c", b"")],
            limites_prueba(0, 10, 3, usize::MAX),
            true,
        );
        assert_presupuesto_arbol(
            &[archivo("a/b/c/d", b"")],
            limites_prueba(0, 10, 3, usize::MAX),
            false,
        );

        assert_presupuesto_arbol(
            &[archivo("a", b"xx"), archivo("b", b"yy")],
            limites_prueba(4, 10, 10, usize::MAX),
            true,
        );
        assert_presupuesto_arbol(
            &[archivo("a", b"xx"), archivo("b", b"yyz")],
            limites_prueba(4, 10, 10, usize::MAX),
            false,
        );

        assert_presupuesto_arbol(&[], limites_prueba(0, 0, 0, 0), true);
    }

    #[test]
    fn presupuesto_del_arbol_detecta_overflow_sin_wraparound() {
        assert_eq!(sumar_presupuesto(usize::MAX, 0, usize::MAX), Ok(usize::MAX));
        assert_eq!(
            sumar_presupuesto(usize::MAX, 1, usize::MAX),
            Err(ErrorSnapshot::LimiteInventarioExcedido)
        );
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

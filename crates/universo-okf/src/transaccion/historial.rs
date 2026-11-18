//! Historial mínimo de procedencia: quién, cuándo y por qué de cada apply.
//!
//! Cada apply admitido añade, bajo el lock de escritor y después de sellar,
//! una línea JSON a `.<raiz>.felix.log`:
//! `{"schema":"felix-log-v1","time":"<UTC RFC 3339>","author":..,"reason":..,
//! "sealed":bool,"state":"<token hex>"|null,"delta":{..}|null}`. `delta` es el
//! delta aplicado en forma canónica (`felix-delta-v1` o `v2`); se omite (`null`,
//! con `"delta_omitted":true`) si su serialización supera 256 KiB, y las líneas
//! anteriores a este campo simplemente no lo tienen. Contiene datos del bundle,
//! como el propio archivo del bundle; `universo log` no lo muestra y
//! `universo show` sí. El autor y el motivo los da quien
//! aplica (opcionales, sin verificación de identidad). `state` es el token del
//! estado sellado; es sensible y no se muestra al listar. El historial es
//! informativo: no participa en el recovery y un fallo al escribirlo no deshace
//! el apply (se informa en `ResultadoAplicacion::registrado`; con `false` la
//! línea puede haberse escrito a medias). Nunca se escribe a través de un
//! enlace ni en algo que no sea un archivo regular. Al leer, las líneas
//! ilegibles o fuera de formato se descartan y se cuentan.
use super::{durabilidad, raiz::UbicacionRaiz, snapshot::SnapshotToken};
use serde_json::{Value, json};
use std::{
    fmt, fs,
    io::{self, Read, Write},
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

const SCHEMA: &str = "felix-log-v1";
const MAX_AUTOR: usize = 128;
const MAX_MOTIVO: usize = 500;
const MAX_HISTORIAL_BYTES: u64 = 16 * 1024 * 1024;
/// Tamaño máximo del delta guardado en una línea del historial.
const MAX_DELTA_EN_HISTORIAL: usize = 256 * 1024;
/// A partir de este tamaño del historial ya no se guardan deltas, para que las
/// entradas de procedencia (unos cientos de bytes) sigan cabiendo bajo
/// `MAX_HISTORIAL_BYTES` y `log` no deje de poder leerse.
const HISTORIAL_SIN_DELTAS_DESDE: u64 = MAX_HISTORIAL_BYTES / 2;

/// Procedencia declarada por quien aplica un cambio.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Procedencia {
    pub autor: Option<String>,
    pub motivo: Option<String>,
}

impl Procedencia {
    /// Una línea imprimible (sin controles ni caracteres invisibles o bidi),
    /// no vacía y con longitud acotada.
    pub fn validar(&self) -> Result<(), ErrorHistorial> {
        if texto_valido(&self.autor, MAX_AUTOR) && texto_valido(&self.motivo, MAX_MOTIVO) {
            Ok(())
        } else {
            Err(ErrorHistorial("provenance_invalid"))
        }
    }
}

fn texto_valido(texto: &Option<String>, maximo: usize) -> bool {
    texto.as_ref().is_none_or(|t| {
        !t.trim().is_empty()
            && t.chars().count() <= maximo
            && !t.chars().any(universo_core::sexpr::caracter_no_imprimible)
    })
}

/// `AAAA-MM-DDTHH:MM:SSZ` con dígitos en su sitio.
fn fecha_valida(fecha: &str) -> bool {
    let b = fecha.as_bytes();
    b.len() == 20
        && b.iter().enumerate().all(|(i, c)| match i {
            4 | 7 => *c == b'-',
            10 => *c == b'T',
            13 | 16 => *c == b':',
            19 => *c == b'Z',
            _ => c.is_ascii_digit(),
        })
}

/// Entrada pública del historial (sin el token del estado).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntradaHistorial {
    pub fecha: String,
    pub autor: Option<String>,
    pub motivo: Option<String>,
    pub sellado: bool,
    /// Delta aplicado en JSON canónico, si la línea lo guarda y es válido.
    pub delta: Option<String>,
    /// El apply omitió el delta por tamaño (del delta o del historial).
    pub delta_omitido: bool,
}

/// Historial leído. `descartadas` cuenta líneas ilegibles o fuera de formato
/// (escrituras interrumpidas o ediciones manuales); no se muestran.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Historial {
    pub entradas: Vec<EntradaHistorial>,
    pub descartadas: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ErrorHistorial(&'static str);
impl ErrorHistorial {
    pub fn codigo(&self) -> &'static str {
        self.0
    }
}
impl fmt::Debug for ErrorHistorial {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl fmt::Display for ErrorHistorial {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for ErrorHistorial {}

fn ruta_historial(ubicacion: &UbicacionRaiz) -> Option<(std::path::PathBuf, std::path::PathBuf)> {
    let ruta = ubicacion.ruta();
    let parent = ruta.parent()?.to_path_buf();
    let mut nombre = std::ffi::OsString::from(".");
    nombre.push(ruta.file_name()?);
    nombre.push(".felix.log");
    Some((parent.join(&nombre), parent))
}

/// Fecha UTC RFC 3339 con segundos (`AAAA-MM-DDTHH:MM:SSZ`). Un reloj anterior
/// a 1970 se registra como `1970-01-01T00:00:00Z`.
pub(crate) fn fecha_utc(instante: SystemTime) -> String {
    let segundos = instante
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs()) as i64;
    let (dias, resto) = (segundos.div_euclid(86_400), segundos.rem_euclid(86_400));
    // Algoritmo civil de Howard Hinnant (días desde 1970-01-01 a fecha).
    let z = dias + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let dia = doy - (153 * mp + 2) / 5 + 1;
    let mes = if mp < 10 { mp + 3 } else { mp - 9 };
    let anio = yoe + era * 400 + i64::from(mes <= 2);
    format!(
        "{anio:04}-{mes:02}-{dia:02}T{:02}:{:02}:{:02}Z",
        resto / 3_600,
        resto % 3_600 / 60,
        resto % 60
    )
}

/// Añade una entrada de forma durable. El llamador retiene el lock de escritor.
pub(crate) fn anotar(
    ubicacion: &UbicacionRaiz,
    procedencia: &Procedencia,
    estado: Option<SnapshotToken>,
    sellado: bool,
    delta: Option<&str>,
) -> io::Result<()> {
    let (ruta, parent) = ruta_historial(ubicacion).ok_or_else(|| io::Error::other("log"))?;
    // Nunca seguir un enlace ni escribir en algo que no sea un archivo regular
    // (un FIFO bloquearía el apply con el lock retenido).
    let previo = match fs::symlink_metadata(&ruta) {
        Ok(m) if m.file_type().is_file() => Some(m.len()),
        Ok(_) => return Err(io::Error::other("log_not_regular")),
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => return Err(e),
    };
    let mut opciones = fs::OpenOptions::new();
    opciones.append(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opciones.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let mut archivo = opciones.open(&ruta)?;
    if !archivo.metadata()?.file_type().is_file() {
        return Err(io::Error::other("log_not_regular"));
    }
    let mut linea = json!({
        "schema": SCHEMA,
        "time": fecha_utc(SystemTime::now()),
        "author": procedencia.autor,
        "reason": procedencia.motivo,
        "sealed": sellado,
        "state": estado.map(SnapshotToken::to_hex),
    });
    if let Some(delta) = delta {
        let cabe = delta.len() <= MAX_DELTA_EN_HISTORIAL
            && previo.unwrap_or(0) < HISTORIAL_SIN_DELTAS_DESDE;
        match serde_json::from_str::<Value>(delta) {
            Ok(valor) if cabe => linea["delta"] = valor,
            _ => {
                linea["delta"] = Value::Null;
                linea["delta_omitted"] = Value::Bool(true);
            }
        }
    }
    // Si una escritura anterior quedó a medias, se cierra con un salto para que
    // la línea nueva no se pegue a la parcial.
    let mut texto = format!("{linea}\n");
    if previo.is_some_and(|len| len > 0) && !termina_en_salto(&ruta)? {
        texto.insert(0, '\n');
    }
    archivo.write_all(texto.as_bytes())?;
    durabilidad::sincronizar_archivo(&archivo, durabilidad::Punto::Historial)?;
    if previo.is_none() {
        #[cfg(unix)]
        {
            let directorio = fs::File::open(&parent)?;
            durabilidad::sincronizar_directorio(&directorio, durabilidad::Punto::Historial)?;
        }
    }
    let _ = parent;
    Ok(())
}

fn termina_en_salto(ruta: &Path) -> io::Result<bool> {
    use std::io::{Seek, SeekFrom};
    let mut archivo = fs::File::open(ruta)?;
    archivo.seek(SeekFrom::End(-1))?;
    let mut ultimo = [0u8; 1];
    archivo.read_exact(&mut ultimo)?;
    Ok(ultimo[0] == b'\n')
}

/// Lee el historial, de la entrada más antigua a la más reciente. Las líneas
/// ilegibles (UTF-8 inválido, JSON roto, campos fuera de formato o con
/// caracteres no imprimibles) se descartan y se cuentan, sin mostrarlas.
pub fn leer(raiz: &Path) -> Result<Historial, ErrorHistorial> {
    let ubicacion = UbicacionRaiz::nueva(raiz).map_err(|_| ErrorHistorial("log_root_invalid"))?;
    let (ruta, _) = ruta_historial(&ubicacion).ok_or(ErrorHistorial("log_root_invalid"))?;
    match fs::symlink_metadata(&ruta) {
        Ok(m) if m.file_type().is_file() => {}
        Ok(_) => return Err(ErrorHistorial("log_invalid")),
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Historial::default()),
        Err(_) => return Err(ErrorHistorial("log_io")),
    }
    let mut bytes = Vec::new();
    fs::File::open(&ruta)
        .map_err(|_| ErrorHistorial("log_io"))?
        .take(MAX_HISTORIAL_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ErrorHistorial("log_io"))?;
    if bytes.len() as u64 > MAX_HISTORIAL_BYTES {
        return Err(ErrorHistorial("log_resource_limit"));
    }
    let mut historial = Historial::default();
    for linea in bytes.split(|b| *b == b'\n').filter(|l| !l.is_empty()) {
        match std::str::from_utf8(linea).ok().and_then(parsear_linea) {
            Some(entrada) => historial.entradas.push(entrada),
            None => historial.descartadas += 1,
        }
    }
    Ok(historial)
}

fn parsear_linea(linea: &str) -> Option<EntradaHistorial> {
    let valor: Value = serde_json::from_str(linea).ok()?;
    if valor.get("schema")?.as_str()? != SCHEMA {
        return None;
    }
    let texto = |clave: &str| match valor.get(clave) {
        Some(Value::Null) | None => Some(None),
        Some(Value::String(s)) => Some(Some(s.clone())),
        _ => None,
    };
    let entrada = EntradaHistorial {
        fecha: valor.get("time")?.as_str()?.to_string(),
        autor: texto("author")?,
        motivo: texto("reason")?,
        sellado: valor.get("sealed")?.as_bool()?,
        // Un delta ilegible no invalida la entrada: sólo no se puede mostrar.
        delta: valor
            .get("delta")
            .filter(|d| d.is_object())
            .map(Value::to_string)
            .filter(|d| crate::parsear_delta_json(d).is_ok()),
        delta_omitido: valor
            .get("delta_omitted")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    };
    // Lo leído se revalida: el archivo puede haberse editado a mano.
    (fecha_valida(&entrada.fecha)
        && texto_valido(&entrada.autor, MAX_AUTOR)
        && texto_valido(&entrada.motivo, MAX_MOTIVO))
    .then_some(entrada)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn raiz() -> (tempfile::TempDir, std::path::PathBuf, UbicacionRaiz) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("bundle");
        fs::create_dir(&root).unwrap();
        let ubicacion = UbicacionRaiz::nueva(&root).unwrap();
        (dir, root, ubicacion)
    }

    #[test]
    fn un_historial_grande_deja_de_guardar_deltas_y_sigue_legible() {
        let (_dir, root, ubicacion) = raiz();
        let delta = r#"{"schema":"felix-delta-v1","add":[],"remove":[]}"#;
        anotar(&ubicacion, &Procedencia::default(), None, true, Some(delta)).unwrap();
        let (ruta, _) = ruta_historial(&ubicacion).unwrap();
        // Relleno con líneas descartables hasta pasar el umbral.
        let relleno = "x".repeat(1023) + "\n";
        let mut archivo = fs::OpenOptions::new().append(true).open(&ruta).unwrap();
        for _ in 0..(HISTORIAL_SIN_DELTAS_DESDE / 1024 + 1) {
            archivo.write_all(relleno.as_bytes()).unwrap();
        }
        drop(archivo);
        anotar(&ubicacion, &Procedencia::default(), None, true, Some(delta)).unwrap();
        let historial = leer(&root).unwrap();
        assert_eq!(historial.entradas.len(), 2);
        assert!(historial.entradas[0].delta.is_some() && !historial.entradas[0].delta_omitido);
        assert!(historial.entradas[1].delta.is_none() && historial.entradas[1].delta_omitido);
    }

    #[test]
    fn fecha_utc_en_fechas_conocidas() {
        let en = |s: u64| fecha_utc(UNIX_EPOCH + Duration::from_secs(s));
        assert_eq!(fecha_utc(UNIX_EPOCH), "1970-01-01T00:00:00Z");
        assert_eq!(en(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(en(1_794_873_600 + 3_661), "2026-11-17T01:01:01Z");
        assert_eq!(en(2_147_483_647), "2038-01-19T03:14:07Z");
        assert_eq!(en(4_107_542_400), "2100-03-01T00:00:00Z"); // 2100 no es bisiesto
        assert_eq!(en(1_798_761_599), "2026-12-31T23:59:59Z");
    }

    #[test]
    fn procedencia_valida_y_rechaza_control_vacio_o_invisible() {
        assert!(Procedencia::default().validar().is_ok());
        let ok = Procedencia {
            autor: Some("José".into()),
            motivo: Some("Corrige el tipo".into()),
        };
        assert!(ok.validar().is_ok());
        for autor in [
            " ",
            "dos\nlíneas",
            "Ro\u{202E}meo",
            "a\u{200B}b",
            "ok\u{2028}x",
        ] {
            let mala = Procedencia {
                autor: Some(autor.into()),
                motivo: None,
            };
            assert_eq!(
                mala.validar().unwrap_err().codigo(),
                "provenance_invalid",
                "{autor:?}"
            );
        }
        let larga = Procedencia {
            autor: Some("a".repeat(129)),
            motivo: None,
        };
        assert!(larga.validar().is_err());
    }

    #[test]
    fn linea_parcial_no_rompe_ni_contamina_el_historial() {
        let (dir, root, ubicacion) = raiz();
        assert_eq!(leer(&root).unwrap(), Historial::default());
        let p = Procedencia {
            autor: Some("José".into()),
            motivo: None,
        };
        anotar(&ubicacion, &p, None, false, None).unwrap();
        let log = dir.path().join(".bundle.felix.log");
        // Escritura cortada a mitad de un carácter UTF-8.
        fs::OpenOptions::new()
            .append(true)
            .open(&log)
            .unwrap()
            .write_all(b"{\"schema\":\"felix-log-v1\",\"author\":\"Jos\xc3")
            .unwrap();
        let historial = leer(&root).unwrap();
        assert_eq!((historial.entradas.len(), historial.descartadas), (1, 1));
        // El apply siguiente no se pega a la línea parcial.
        anotar(&ubicacion, &Procedencia::default(), None, true, None).unwrap();
        let historial = leer(&root).unwrap();
        assert_eq!((historial.entradas.len(), historial.descartadas), (2, 1));
        assert_eq!(historial.entradas[0].autor.as_deref(), Some("José"));
        assert!(!historial.entradas[0].sellado && historial.entradas[1].sellado);
    }

    #[test]
    fn lectura_descarta_texto_no_imprimible_o_fecha_falsa() {
        let (dir, root, _) = raiz();
        let malas = [
            r#"{"schema":"felix-log-v1","time":"2026-11-17T00:00:00Z","author":"a\u001b]0;x\u0007","reason":null,"sealed":true}"#,
            r#"{"schema":"felix-log-v1","time":"x\u001b[2J","author":null,"reason":null,"sealed":true}"#,
            "{\"schema\":\"felix-log-v1\",\"time\":\"2026-11-17T00:00:00Z\",\"author\":\"Ro\\u202emeo\",\"reason\":null,\"sealed\":true}",
            "basura",
        ];
        fs::write(
            dir.path().join(".bundle.felix.log"),
            malas.join("\n") + "\n",
        )
        .unwrap();
        let historial = leer(&root).unwrap();
        assert_eq!((historial.entradas.len(), historial.descartadas), (0, 4));
    }

    #[cfg(unix)]
    #[test]
    fn anotar_no_sigue_symlinks() {
        let (dir, _root, ubicacion) = raiz();
        let victima = dir.path().join("victima.txt");
        fs::write(&victima, b"intacto").unwrap();
        let log = dir.path().join(".bundle.felix.log");
        std::os::unix::fs::symlink(&victima, &log).unwrap();
        assert!(anotar(&ubicacion, &Procedencia::default(), None, true, None).is_err());
        assert_eq!(fs::read(&victima).unwrap(), b"intacto");
        fs::remove_file(&log).unwrap();
        std::os::unix::fs::symlink(dir.path().join("colgante"), &log).unwrap();
        assert!(anotar(&ubicacion, &Procedencia::default(), None, true, None).is_err());
        assert!(!dir.path().join("colgante").exists());
    }
}

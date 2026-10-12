//! Lectura raw del árbol del bundle sin seguir enlaces observables.
//!
//! El inventario incluye archivos regulares y directorios, incluidos los
//! vacíos. La identidad de ruta es UTF-8 exacto, separador `/`, sin casefold
//! ni normalización Unicode. El backend por ruta conserva un límite TOCTOU.

use std::fs::{self, File, Metadata, OpenOptions};
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use super::snapshot::{EntradaSnapshot, ErrorSnapshot};

enum EntradaAbierta<D, A> {
    Directorio(D),
    Archivo(A),
}

/// Presupuesto estructural común a los inventarios por ruta y por handle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LimitesInventario {
    pub(crate) bytes_raw: usize,
    pub(crate) entradas: usize,
    pub(crate) profundidad: usize,
    pub(crate) bytes_rutas: usize,
}

impl LimitesInventario {
    pub(crate) const CONTRATO: Self = Self {
        bytes_raw: 128 * 1024 * 1024,
        entradas: 100_000,
        profundidad: 128,
        bytes_rutas: 16 * 1024 * 1024,
    };
}

#[derive(Debug)]
struct PresupuestoInventario {
    limites: LimitesInventario,
    bytes_raw: usize,
    entradas: usize,
    bytes_rutas: usize,
}

impl PresupuestoInventario {
    fn nuevo(limites: LimitesInventario) -> Self {
        Self {
            limites,
            bytes_raw: 0,
            entradas: 0,
            bytes_rutas: 0,
        }
    }

    fn reservar_ruta(&mut self, bytes: usize, profundidad: usize) -> Result<(), ErrorSnapshot> {
        if profundidad > self.limites.profundidad || self.entradas >= self.limites.entradas {
            return Err(ErrorSnapshot::LimiteInventarioExcedido);
        }
        let total = self
            .bytes_rutas
            .checked_add(bytes)
            .filter(|total| *total <= self.limites.bytes_rutas)
            .ok_or(ErrorSnapshot::LimiteInventarioExcedido)?;
        self.entradas += 1;
        self.bytes_rutas = total;
        Ok(())
    }

    fn bytes_raw_restantes(&self) -> usize {
        self.limites.bytes_raw - self.bytes_raw
    }

    fn registrar_bytes_archivo(&mut self, bytes: usize) -> Result<(), ErrorSnapshot> {
        let total = self
            .bytes_raw
            .checked_add(bytes)
            .filter(|total| *total <= self.limites.bytes_raw)
            .ok_or(ErrorSnapshot::LimiteInventarioExcedido)?;
        self.bytes_raw = total;
        Ok(())
    }
}

/// Único recorrido del inventario; los backends solo enumeran, abren y leen.
trait BackendInventario {
    type Directorio;
    type Archivo;

    fn nombres_con<F>(
        &mut self,
        directorio: &mut Self::Directorio,
        visitar: &mut F,
    ) -> Result<(), ErrorSnapshot>
    where
        F: FnMut(&mut Self, &mut Self::Directorio, std::ffi::OsString) -> Result<(), ErrorSnapshot>;

    fn abrir(
        &mut self,
        directorio: &Self::Directorio,
        nombre: &str,
    ) -> Result<EntradaAbierta<Self::Directorio, Self::Archivo>, ErrorSnapshot>;

    fn leer(
        &mut self,
        archivo: Self::Archivo,
        maximo_lectura: usize,
    ) -> Result<Vec<u8>, ErrorSnapshot>;
}

/// Lee una vez cada archivo y devuelve el inventario ordenado por bytes UTF-8.
pub(crate) fn inventariar_root(raiz: &Path) -> Result<Vec<EntradaSnapshot>, ErrorSnapshot> {
    inventariar_root_con_lector_limitado(raiz, leer_archivo_regular_limitado)
}

pub(crate) fn inventariar_root_con_lector<F>(
    raiz: &Path,
    leer: F,
) -> Result<Vec<EntradaSnapshot>, ErrorSnapshot>
where
    F: FnMut(&Path) -> Result<Vec<u8>, ErrorSnapshot>,
{
    let mut leer = leer;
    inventariar_root_con_lector_y_limites(
        raiz,
        move |ruta, _maximo_lectura| leer(ruta),
        LimitesInventario::CONTRATO,
    )
}

pub(crate) fn inventariar_root_con_lector_limitado<F>(
    raiz: &Path,
    leer: F,
) -> Result<Vec<EntradaSnapshot>, ErrorSnapshot>
where
    F: FnMut(&Path, usize) -> Result<Vec<u8>, ErrorSnapshot>,
{
    inventariar_root_con_lector_y_limites(raiz, leer, LimitesInventario::CONTRATO)
}

fn inventariar_root_con_lector_y_limites<F>(
    raiz: &Path,
    leer: F,
    limites: LimitesInventario,
) -> Result<Vec<EntradaSnapshot>, ErrorSnapshot>
where
    F: FnMut(&Path, usize) -> Result<Vec<u8>, ErrorSnapshot>,
{
    let raiz = ruta_absoluta_lexica(raiz)?;
    validar_directorio_sin_enlaces(&raiz)?;
    caminar_inventario_con_limites(&mut BackendRuta { leer }, raiz, limites)
}

#[cfg(test)]
fn caminar_inventario<B: BackendInventario>(
    backend: &mut B,
    raiz: B::Directorio,
) -> Result<Vec<EntradaSnapshot>, ErrorSnapshot> {
    caminar_inventario_con_limites(backend, raiz, LimitesInventario::CONTRATO)
}

fn caminar_inventario_con_limites<B: BackendInventario>(
    backend: &mut B,
    raiz: B::Directorio,
    limites: LimitesInventario,
) -> Result<Vec<EntradaSnapshot>, ErrorSnapshot> {
    let mut entradas = Vec::new();
    let mut presupuesto = PresupuestoInventario::nuevo(limites);
    caminar_directorio(backend, raiz, "", 0, &mut presupuesto, &mut entradas)?;
    entradas.sort_by(|a, b| a.ruta().as_bytes().cmp(b.ruta().as_bytes()));
    Ok(entradas)
}

fn caminar_directorio<B: BackendInventario>(
    backend: &mut B,
    mut directorio: B::Directorio,
    prefijo: &str,
    profundidad: usize,
    presupuesto: &mut PresupuestoInventario,
    entradas: &mut Vec<EntradaSnapshot>,
) -> Result<(), ErrorSnapshot> {
    // Procesa cada hijo antes de pedir el siguiente nombre al backend: no se
    // acumulan nombres ni handles de directorios hermanos.
    backend.nombres_con(&mut directorio, &mut |backend, directorio, nombre| {
        let nombre = nombre
            .into_string()
            .map_err(|_| ErrorSnapshot::RutaNoUtf8)?;
        validar_nombre_inventario(&nombre)?;
        let profundidad_entrada = profundidad
            .checked_add(1)
            .ok_or(ErrorSnapshot::LimiteInventarioExcedido)?;
        let bytes_separador = usize::from(!prefijo.is_empty());
        let bytes_ruta = prefijo
            .len()
            .checked_add(bytes_separador)
            .and_then(|bytes| bytes.checked_add(nombre.len()))
            .ok_or(ErrorSnapshot::LimiteInventarioExcedido)?;
        presupuesto.reservar_ruta(bytes_ruta, profundidad_entrada)?;
        entradas.try_reserve(1).map_err(|_| ErrorSnapshot::Io)?;
        let mut ruta = String::new();
        ruta.try_reserve_exact(bytes_ruta)
            .map_err(|_| ErrorSnapshot::Io)?;
        if prefijo.is_empty() {
            ruta.push_str(&nombre);
        } else {
            ruta.push_str(prefijo);
            ruta.push('/');
            ruta.push_str(&nombre);
        }

        match backend.abrir(directorio, &nombre)? {
            EntradaAbierta::Directorio(hijo) => {
                caminar_directorio(
                    backend,
                    hijo,
                    &ruta,
                    profundidad_entrada,
                    presupuesto,
                    entradas,
                )?;
                entradas.try_reserve(1).map_err(|_| ErrorSnapshot::Io)?;
                entradas.push(EntradaSnapshot::directorio(ruta));
            }
            EntradaAbierta::Archivo(archivo) => {
                let maximo_lectura = presupuesto
                    .bytes_raw_restantes()
                    .checked_add(1)
                    .ok_or(ErrorSnapshot::LongitudExcedida)?;
                let bytes = backend.leer(archivo, maximo_lectura)?;
                presupuesto.registrar_bytes_archivo(bytes.len())?;
                entradas.try_reserve(1).map_err(|_| ErrorSnapshot::Io)?;
                entradas.push(EntradaSnapshot::archivo(ruta, bytes));
            }
        }
        Ok(())
    })
}

fn validar_nombre_inventario(nombre: &str) -> Result<(), ErrorSnapshot> {
    if nombre.is_empty()
        || matches!(nombre, "." | "..")
        || nombre.contains('/')
        || nombre.contains('\\')
    {
        return Err(ErrorSnapshot::RutaNoRepresentable);
    }
    Ok(())
}

// P3: enumera nombres antes de abrirlos; bajo mutación externa conserva
// TOCTOU y no promete el mismo interleaving entre ambos pasos.
struct BackendRuta<F> {
    leer: F,
}

impl<F> BackendInventario for BackendRuta<F>
where
    F: FnMut(&Path, usize) -> Result<Vec<u8>, ErrorSnapshot>,
{
    type Directorio = PathBuf;
    type Archivo = PathBuf;

    fn nombres_con<V>(
        &mut self,
        directorio: &mut Self::Directorio,
        visitar: &mut V,
    ) -> Result<(), ErrorSnapshot>
    where
        V: FnMut(&mut Self, &mut Self::Directorio, std::ffi::OsString) -> Result<(), ErrorSnapshot>,
    {
        validar_directorio_sin_enlaces(directorio)?;
        for entrada in fs::read_dir(&*directorio).map_err(|_| ErrorSnapshot::Io)? {
            let entrada = entrada.map_err(|_| ErrorSnapshot::Io)?;
            visitar(self, directorio, entrada.file_name())?;
        }
        Ok(())
    }

    fn abrir(
        &mut self,
        directorio: &Self::Directorio,
        nombre: &str,
    ) -> Result<EntradaAbierta<Self::Directorio, Self::Archivo>, ErrorSnapshot> {
        let camino = directorio.join(nombre);
        let metadata = fs::symlink_metadata(&camino).map_err(|_| ErrorSnapshot::Io)?;
        if es_enlace_o_reparse(&metadata) {
            return Err(ErrorSnapshot::EnlaceNoPermitido);
        }
        if metadata.is_dir() {
            Ok(EntradaAbierta::Directorio(camino))
        } else if metadata.is_file() {
            Ok(EntradaAbierta::Archivo(camino))
        } else {
            Err(ErrorSnapshot::TipoNoAdmitido)
        }
    }

    fn leer(
        &mut self,
        archivo: Self::Archivo,
        maximo_lectura: usize,
    ) -> Result<Vec<u8>, ErrorSnapshot> {
        (self.leer)(&archivo, maximo_lectura)
    }
}

pub(crate) fn ruta_absoluta_lexica(ruta: &Path) -> Result<PathBuf, ErrorSnapshot> {
    let ruta = if ruta.is_absolute() {
        ruta.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|_| ErrorSnapshot::Io)?
            .join(ruta)
    };
    let mut normalizada = PathBuf::new();
    for componente in ruta.components() {
        match componente {
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalizada.pop() {
                    return Err(ErrorSnapshot::RaizInvalida);
                }
            }
            otro => normalizada.push(otro.as_os_str()),
        }
    }
    Ok(normalizada)
}

pub(crate) fn validar_directorio_sin_enlaces(ruta: &Path) -> Result<(), ErrorSnapshot> {
    let metadata = validar_componentes_sin_enlaces(ruta)?;
    if metadata.is_dir() {
        Ok(())
    } else {
        Err(ErrorSnapshot::RaizInvalida)
    }
}

fn validar_componentes_sin_enlaces(ruta: &Path) -> Result<Metadata, ErrorSnapshot> {
    let mut ancestros: Vec<_> = ruta.ancestors().collect();
    ancestros.reverse();
    let mut final_metadata = None;
    for componente in ancestros {
        let metadata = fs::symlink_metadata(componente).map_err(|_| ErrorSnapshot::Io)?;
        if es_enlace_o_reparse(&metadata) {
            return Err(ErrorSnapshot::EnlaceNoPermitido);
        }
        if componente != ruta && !metadata.is_dir() {
            return Err(ErrorSnapshot::TipoNoAdmitido);
        }
        if componente == ruta {
            final_metadata = Some(metadata);
        }
    }
    final_metadata.ok_or(ErrorSnapshot::Io)
}

pub(crate) fn validar_archivo_sin_enlaces(ruta: &Path) -> Result<(), ErrorSnapshot> {
    let metadata = validar_componentes_sin_enlaces(ruta)?;
    if metadata.is_file() {
        Ok(())
    } else {
        Err(ErrorSnapshot::TipoNoAdmitido)
    }
}

fn es_enlace_o_reparse(metadata: &Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        false
    }
}

pub(crate) fn leer_archivo_regular(ruta: &Path) -> Result<Vec<u8>, ErrorSnapshot> {
    let maximo_lectura = LimitesInventario::CONTRATO
        .bytes_raw
        .checked_add(1)
        .ok_or(ErrorSnapshot::LongitudExcedida)?;
    leer_archivo_regular_limitado(ruta, maximo_lectura)
}

pub(crate) fn leer_archivo_regular_limitado(
    ruta: &Path,
    maximo_lectura: usize,
) -> Result<Vec<u8>, ErrorSnapshot> {
    let mut opciones = OpenOptions::new();
    opciones.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opciones.custom_flags(flags_lectura_regular_unix());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        opciones.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let archivo: File = opciones.open(ruta).map_err(|_| ErrorSnapshot::Io)?;
    let metadata = archivo.metadata().map_err(|_| ErrorSnapshot::Io)?;
    if es_enlace_o_reparse(&metadata) {
        return Err(ErrorSnapshot::EnlaceNoPermitido);
    }
    if !metadata.is_file() {
        return Err(ErrorSnapshot::TipoNoAdmitido);
    }
    let bytes_disponibles = maximo_lectura
        .checked_sub(1)
        .ok_or(ErrorSnapshot::LimiteInventarioExcedido)?;
    leer_hasta_presupuesto(archivo, metadata.len(), bytes_disponibles)
}

#[cfg(unix)]
fn flags_lectura_regular_unix() -> libc::c_int {
    libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_NOCTTY | libc::O_CLOEXEC
}

fn leer_hasta_presupuesto<R: Read>(
    lector: R,
    tamano_preflight: u64,
    bytes_disponibles: usize,
) -> Result<Vec<u8>, ErrorSnapshot> {
    if tamano_preflight
        > u64::try_from(bytes_disponibles).map_err(|_| ErrorSnapshot::LongitudExcedida)?
    {
        return Err(ErrorSnapshot::LimiteInventarioExcedido);
    }
    let maximo_lectura = bytes_disponibles
        .checked_add(1)
        .ok_or(ErrorSnapshot::LongitudExcedida)?;
    let maximo_lectura =
        u64::try_from(maximo_lectura).map_err(|_| ErrorSnapshot::LongitudExcedida)?;
    let mut bytes = Vec::new();
    lector
        .take(maximo_lectura)
        .read_to_end(&mut bytes)
        .map_err(|_| ErrorSnapshot::Io)?;
    if bytes.len() > bytes_disponibles {
        return Err(ErrorSnapshot::LimiteInventarioExcedido);
    }
    Ok(bytes)
}

/// Inventaría desde un root ya abierto; no reconstruye rutas absolutas.
///
/// Serializar llamadas sobre clones del mismo handle: `fs_at` comparte cursor
/// de enumeración entre esos clones, aunque cada llamada individual reinicie
/// el cursor antes de recorrerlo.
pub(crate) fn inventariar_root_anclado(
    raiz: super::anclado::DirectorioAnclado,
) -> Result<Vec<EntradaSnapshot>, ErrorSnapshot> {
    inventariar_root_anclado_con_limites(raiz, LimitesInventario::CONTRATO)
}

pub(crate) fn inventariar_root_anclado_con_limites(
    raiz: super::anclado::DirectorioAnclado,
    limites: LimitesInventario,
) -> Result<Vec<EntradaSnapshot>, ErrorSnapshot> {
    caminar_inventario_con_limites(
        &mut BackendAnclado {
            leer: leer_handle_limitado,
        },
        raiz,
        limites,
    )
}

pub(crate) fn inventariar_root_anclado_con_lector<F>(
    raiz: super::anclado::DirectorioAnclado,
    leer: F,
) -> Result<Vec<EntradaSnapshot>, ErrorSnapshot>
where
    F: FnMut(&mut File) -> Result<Vec<u8>, ErrorSnapshot>,
{
    let mut leer = leer;
    caminar_inventario_con_limites(
        &mut BackendAnclado {
            leer: |archivo: &mut File, _maximo_lectura| leer(archivo),
        },
        raiz,
        LimitesInventario::CONTRATO,
    )
}

struct BackendAnclado<F> {
    leer: F,
}

impl<F> BackendInventario for BackendAnclado<F>
where
    F: FnMut(&mut File, usize) -> Result<Vec<u8>, ErrorSnapshot>,
{
    type Directorio = super::anclado::DirectorioAnclado;
    type Archivo = File;

    fn nombres_con<V>(
        &mut self,
        directorio: &mut Self::Directorio,
        visitar: &mut V,
    ) -> Result<(), ErrorSnapshot>
    where
        V: FnMut(&mut Self, &mut Self::Directorio, std::ffi::OsString) -> Result<(), ErrorSnapshot>,
    {
        let mut enumeracion = directorio.clonar().map_err(mapear_error_anclado)?;
        let resultado = enumeracion.read_dir_con(|nombre| visitar(self, directorio, nombre));
        match resultado {
            Ok(()) => Ok(()),
            Err(super::anclado::ErrorLecturaNombres::Directorio(error)) => {
                Err(mapear_error_anclado(error))
            }
            Err(super::anclado::ErrorLecturaNombres::Visitante(error)) => Err(error),
        }
    }

    fn abrir(
        &mut self,
        directorio: &Self::Directorio,
        nombre: &str,
    ) -> Result<EntradaAbierta<Self::Directorio, Self::Archivo>, ErrorSnapshot> {
        match directorio
            .abrir_entrada_inventario(nombre)
            .map_err(mapear_error_anclado)?
        {
            super::anclado::EntradaLecturaAnclada::Directorio(directorio) => {
                Ok(EntradaAbierta::Directorio(directorio))
            }
            super::anclado::EntradaLecturaAnclada::Archivo(archivo) => {
                Ok(EntradaAbierta::Archivo(archivo))
            }
        }
    }

    fn leer(
        &mut self,
        mut archivo: Self::Archivo,
        maximo_lectura: usize,
    ) -> Result<Vec<u8>, ErrorSnapshot> {
        (self.leer)(&mut archivo, maximo_lectura)
    }
}

fn mapear_error_anclado(error: super::anclado::ErrorDirectorioAnclado) -> ErrorSnapshot {
    match error {
        super::anclado::ErrorDirectorioAnclado::EnlaceNoPermitido => {
            ErrorSnapshot::EnlaceNoPermitido
        }
        super::anclado::ErrorDirectorioAnclado::TipoNoAdmitido => ErrorSnapshot::TipoNoAdmitido,
        super::anclado::ErrorDirectorioAnclado::ComponenteInvalido => {
            ErrorSnapshot::RutaNoRepresentable
        }
        super::anclado::ErrorDirectorioAnclado::RaizAusente => ErrorSnapshot::RaizInvalida,
        super::anclado::ErrorDirectorioAnclado::Io
        | super::anclado::ErrorDirectorioAnclado::EntradaAusente
        | super::anclado::ErrorDirectorioAnclado::EntradaExistente => ErrorSnapshot::Io,
    }
}

fn leer_handle_limitado(
    archivo: &mut File,
    maximo_lectura: usize,
) -> Result<Vec<u8>, ErrorSnapshot> {
    let tamano_preflight = archivo.metadata().map_err(|_| ErrorSnapshot::Io)?.len();
    let bytes_disponibles = maximo_lectura
        .checked_sub(1)
        .ok_or(ErrorSnapshot::LimiteInventarioExcedido)?;
    leer_hasta_presupuesto(archivo, tamano_preflight, bytes_disponibles)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transaccion::anclado::DirectorioAnclado;
    use crate::transaccion::raiz::UbicacionRaiz;
    use crate::transaccion::snapshot::{TipoEntradaSnapshot, calcular_token_snapshot};
    use std::cell::Cell;
    use std::collections::HashMap;
    use std::io::{Cursor, Read};
    use std::rc::Rc;

    fn anclar(root: &Path) -> DirectorioAnclado {
        DirectorioAnclado::desde_ubicacion(&UbicacionRaiz::nueva(root).unwrap()).unwrap()
    }

    fn resumen(
        entradas: &[EntradaSnapshot],
    ) -> Vec<(String, TipoEntradaSnapshot, Option<Vec<u8>>)> {
        entradas
            .iter()
            .map(|entrada| {
                (
                    entrada.ruta().to_owned(),
                    entrada.tipo(),
                    entrada.bytes().map(<[u8]>::to_vec),
                )
            })
            .collect()
    }

    fn token(entradas: &[EntradaSnapshot]) -> super::super::snapshot::SnapshotToken {
        calcular_token_snapshot("engine/1", "policy/1", b"manifest", b"invariants", entradas)
            .unwrap()
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

    fn inventariar_path_con_limites(
        root: &Path,
        limites: LimitesInventario,
    ) -> Result<Vec<EntradaSnapshot>, ErrorSnapshot> {
        inventariar_root_con_lector_y_limites(root, leer_archivo_regular_limitado, limites)
    }

    fn inventariar_handle_con_limites(
        root: &Path,
        limites: LimitesInventario,
    ) -> Result<Vec<EntradaSnapshot>, ErrorSnapshot> {
        inventariar_root_anclado_con_limites(anclar(root).abrir_raiz().unwrap(), limites)
    }

    fn inventariar_ambos(
        root: &Path,
        limites: LimitesInventario,
    ) -> [Result<Vec<EntradaSnapshot>, ErrorSnapshot>; 2] {
        [
            inventariar_path_con_limites(root, limites),
            inventariar_handle_con_limites(root, limites),
        ]
    }

    fn crear_arbol_estable(root: &Path) {
        fs::create_dir_all(root.join("a")).unwrap();
        fs::create_dir(root.join("vacio")).unwrap();
        fs::write(root.join("a/MiXeD-é.bin"), [0, 255, 1]).unwrap();
        fs::write(root.join("0.empty"), []).unwrap();
    }

    fn crear_par_si_el_filesystem_distingue(
        root: &Path,
        primero: &str,
        segundo: &str,
        identidad: &str,
    ) -> Option<[String; 2]> {
        let primera = root.join(primero);
        let segunda = root.join(segundo);
        drop(
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&primera)
                .unwrap_or_else(|error| panic!("no se pudo crear {primero}: {error}")),
        );

        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&segunda)
        {
            Ok(archivo) => {
                drop(archivo);
                Some([primero.to_owned(), segundo.to_owned()])
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                fs::remove_file(primera).unwrap();
                eprintln!("SKIP explícito: el filesystem colisiona el par {identidad}");
                None
            }
            Err(error) => panic!("no se pudo crear {segundo}: {error}"),
        }
    }

    fn rechazo_path_sin_lectura(root: &Path) -> ErrorSnapshot {
        let mut lecturas = 0;
        let resultado = inventariar_root_con_lector(root, |_| {
            lecturas += 1;
            Ok(Vec::new())
        });
        assert_eq!(lecturas, 0);
        resultado.unwrap_err()
    }

    #[cfg(unix)]
    fn rechazo_handle_sin_lectura(root: &Path) -> ErrorSnapshot {
        let mut lecturas = 0;
        let resultado =
            inventariar_root_anclado_con_lector(anclar(root).abrir_raiz().unwrap(), |_| {
                lecturas += 1;
                Ok(Vec::new())
            });
        assert_eq!(lecturas, 0);
        resultado.unwrap_err()
    }

    #[test]
    fn incluye_binarios_vacios_y_directorios_en_orden_utf8_exact() {
        let root = tempfile::tempdir().unwrap();
        crear_arbol_estable(root.path());

        let entries = inventariar_root(root.path()).unwrap();
        let paths: Vec<_> = entries.iter().map(EntradaSnapshot::ruta).collect();
        assert_eq!(paths, ["0.empty", "a", "a/MiXeD-é.bin", "vacio"]);
        assert_eq!(entries[0].bytes(), Some([].as_slice()));
        assert_eq!(entries[2].bytes(), Some([0, 255, 1].as_slice()));
        assert_eq!(entries[3].tipo(), TipoEntradaSnapshot::Directorio);
    }

    #[test]
    fn camino_y_handle_comparten_orden_bytes_y_token_en_arbol_estable() {
        let root = tempfile::tempdir().unwrap();
        crear_arbol_estable(root.path());

        let path_entries = inventariar_root(root.path()).unwrap();
        let handle_entries =
            inventariar_root_anclado(anclar(root.path()).abrir_raiz().unwrap()).unwrap();

        assert_eq!(resumen(&path_entries), resumen(&handle_entries));
        assert_eq!(token(&path_entries), token(&handle_entries));
        assert_eq!(
            path_entries
                .iter()
                .map(EntradaSnapshot::ruta)
                .collect::<Vec<_>>(),
            ["0.empty", "a", "a/MiXeD-é.bin", "vacio"]
        );
    }

    #[test]
    fn replay_path_y_handle_conservan_entradas_y_token_con_identidades_exactas() {
        let root = tempfile::tempdir().unwrap();
        crear_arbol_estable(root.path());
        let mut adicionales = Vec::new();
        if let Some(par) = crear_par_si_el_filesystem_distingue(
            root.path(),
            "Case-sibling.txt",
            "case-sibling.txt",
            "case",
        ) {
            adicionales.extend(par);
        }
        if let Some(par) = crear_par_si_el_filesystem_distingue(
            root.path(),
            "é-NFC.txt",
            "e\u{301}-NFD.txt",
            "Unicode NFC/NFD",
        ) {
            adicionales.extend(par);
        }

        let path_primera = inventariar_root(root.path()).unwrap();
        let path_replay = inventariar_root(root.path()).unwrap();
        let handle_primera =
            inventariar_root_anclado(anclar(root.path()).abrir_raiz().unwrap()).unwrap();
        let handle_replay =
            inventariar_root_anclado(anclar(root.path()).abrir_raiz().unwrap()).unwrap();

        assert_eq!(resumen(&path_primera), resumen(&path_replay));
        assert_eq!(resumen(&handle_primera), resumen(&handle_replay));
        assert_eq!(resumen(&path_primera), resumen(&handle_primera));
        assert_eq!(token(&path_primera), token(&path_replay));
        assert_eq!(token(&handle_primera), token(&handle_replay));
        assert_eq!(token(&path_primera), token(&handle_primera));
        for nombre in adicionales {
            assert!(
                path_primera.iter().any(|entrada| entrada.ruta() == nombre),
                "no se preservó el nombre exacto {nombre:?}"
            );
        }
    }

    #[test]
    fn ambos_backends_leen_una_vez_cada_archivo() {
        let root = tempfile::tempdir().unwrap();
        crear_arbol_estable(root.path());

        let mut lecturas_path = HashMap::<PathBuf, usize>::new();
        let path_entries = inventariar_root_con_lector(root.path(), |path| {
            *lecturas_path.entry(path.to_path_buf()).or_default() += 1;
            leer_archivo_regular(path)
        })
        .unwrap();
        let mut lecturas_handle = 0;
        let handle_entries = inventariar_root_anclado_con_lector(
            anclar(root.path()).abrir_raiz().unwrap(),
            |archivo| {
                lecturas_handle += 1;
                let mut bytes = Vec::new();
                archivo
                    .take(1024)
                    .read_to_end(&mut bytes)
                    .map_err(|_| ErrorSnapshot::Io)?;
                Ok(bytes)
            },
        )
        .unwrap();

        assert_eq!(lecturas_path.len(), 2);
        assert!(lecturas_path.values().all(|&conteo| conteo == 1));
        assert_eq!(lecturas_handle, 2);
        assert_eq!(resumen(&path_entries), resumen(&handle_entries));
    }

    struct BackendConteoDirectorios {
        enumeraciones: [usize; 2],
    }

    impl BackendInventario for BackendConteoDirectorios {
        type Directorio = usize;
        type Archivo = ();

        fn nombres_con<V>(
            &mut self,
            directorio: &mut Self::Directorio,
            visitar: &mut V,
        ) -> Result<(), ErrorSnapshot>
        where
            V: FnMut(
                &mut Self,
                &mut Self::Directorio,
                std::ffi::OsString,
            ) -> Result<(), ErrorSnapshot>,
        {
            self.enumeraciones[*directorio] += 1;
            if *directorio == 0 {
                visitar(self, directorio, "sub".into())?;
            }
            Ok(())
        }

        fn abrir(
            &mut self,
            _directorio: &Self::Directorio,
            nombre: &str,
        ) -> Result<EntradaAbierta<Self::Directorio, Self::Archivo>, ErrorSnapshot> {
            assert_eq!(nombre, "sub");
            Ok(EntradaAbierta::Directorio(1))
        }

        fn leer(
            &mut self,
            (): Self::Archivo,
            _maximo_lectura: usize,
        ) -> Result<Vec<u8>, ErrorSnapshot> {
            unreachable!("el backend solo enumera directorios")
        }
    }

    #[test]
    fn enumera_cada_directorio_una_sola_vez() {
        let mut backend = BackendConteoDirectorios {
            enumeraciones: [0; 2],
        };

        caminar_inventario(&mut backend, 0).unwrap();

        assert_eq!(backend.enumeraciones, [1, 1]);
    }

    #[test]
    fn limites_estructurales_aceptan_exactitud_y_rechazan_mas_uno_en_ambos_backends() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("a"), []).unwrap();
        fs::write(root.path().join("b"), []).unwrap();

        let exacto = limites_prueba(0, 2, 1, 2);
        for resultado in inventariar_ambos(root.path(), exacto) {
            assert_eq!(resultado.unwrap().len(), 2);
        }

        fs::write(root.path().join("secret-canary-name"), []).unwrap();
        let limite_entradas = limites_prueba(0, 2, 1, 32);
        for resultado in inventariar_ambos(root.path(), limite_entradas) {
            let error = resultado.unwrap_err();
            assert_eq!(error, ErrorSnapshot::LimiteInventarioExcedido);
            assert_eq!(error.codigo(), "snapshot_resource_limit");
            assert_eq!(error.to_string(), "snapshot_resource_limit");
            assert!(!format!("{error:?}").contains("secret-canary-name"));
            assert!(!error.to_string().contains("secret-canary-name"));
        }

        fs::remove_file(root.path().join("secret-canary-name")).unwrap();
        fs::write(root.path().join("é"), []).unwrap();
        let limite_ruta_utf8 = limites_prueba(0, 3, 1, 4);
        for resultado in inventariar_ambos(root.path(), limite_ruta_utf8) {
            assert_eq!(resultado.unwrap().len(), 3);
        }
        let ruta_utf8_mas_uno = limites_prueba(0, 3, 1, 3);
        for resultado in inventariar_ambos(root.path(), ruta_utf8_mas_uno) {
            assert_eq!(
                resultado.unwrap_err(),
                ErrorSnapshot::LimiteInventarioExcedido
            );
        }
    }

    #[test]
    fn ambos_backends_aceptan_limite_raw_exact_y_rechazan_mas_uno() {
        let root = tempfile::tempdir().unwrap();
        let archivo = root.path().join("data.bin");
        fs::write(&archivo, b"1234").unwrap();
        let limites = limites_prueba(4, 10, 10, 128);

        for inventariar in [inventariar_path_con_limites, inventariar_handle_con_limites] {
            let entries = inventariar(root.path(), limites).unwrap();
            assert_eq!(entries[0].bytes(), Some(b"1234".as_slice()));

            fs::write(&archivo, b"12345").unwrap();
            let error = inventariar(root.path(), limites).unwrap_err();
            assert_eq!(error, ErrorSnapshot::LimiteInventarioExcedido);
            assert_eq!(error.codigo(), "snapshot_resource_limit");
            assert_eq!(error.to_string(), "snapshot_resource_limit");
            assert!(!format!("{error:?}").contains("data.bin"));
            fs::write(&archivo, b"1234").unwrap();
        }
    }

    struct LectorContado<'a> {
        cursor: Cursor<Vec<u8>>,
        bytes_leidos: &'a Cell<usize>,
    }

    impl Read for LectorContado<'_> {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            let leidos = self.cursor.read(buffer)?;
            self.bytes_leidos.set(self.bytes_leidos.get() + leidos);
            Ok(leidos)
        }
    }

    #[test]
    fn lectura_raw_preflight_y_remaining_mas_uno_cubren_crecimiento_concurrente() {
        let leidos = Cell::new(0);
        let exacto = leer_hasta_presupuesto(
            LectorContado {
                cursor: Cursor::new(b"1234".to_vec()),
                bytes_leidos: &leidos,
            },
            4,
            4,
        )
        .unwrap();
        assert_eq!(exacto, b"1234");
        assert_eq!(leidos.get(), 4);

        leidos.set(0);
        let error = leer_hasta_presupuesto(
            LectorContado {
                cursor: Cursor::new(b"12345".to_vec()),
                bytes_leidos: &leidos,
            },
            4,
            4,
        )
        .unwrap_err();
        assert_eq!(error, ErrorSnapshot::LimiteInventarioExcedido);
        assert_eq!(leidos.get(), 5, "solo se permite leer el byte centinela");

        leidos.set(0);
        let error = leer_hasta_presupuesto(
            LectorContado {
                cursor: Cursor::new(b"12345".to_vec()),
                bytes_leidos: &leidos,
            },
            5,
            4,
        )
        .unwrap_err();
        assert_eq!(error, ErrorSnapshot::LimiteInventarioExcedido);
        assert_eq!(leidos.get(), 0, "preflight rechaza antes de leer");
    }

    #[test]
    fn limites_de_profundidad_inclusivos_comparten_semantica_en_ambos_backends() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("a/b")).unwrap();
        fs::write(root.path().join("a/file"), []).unwrap();

        let profundidad_exacta = limites_prueba(0, 10, 2, 64);
        for resultado in inventariar_ambos(root.path(), profundidad_exacta) {
            assert_eq!(resultado.unwrap().len(), 3);
        }

        fs::write(root.path().join("a/b/file"), []).unwrap();
        for resultado in inventariar_ambos(root.path(), profundidad_exacta) {
            assert_eq!(
                resultado.unwrap_err(),
                ErrorSnapshot::LimiteInventarioExcedido
            );
        }
    }

    struct DirectorioContado {
        id: usize,
        estado: Rc<Cell<(usize, usize)>>,
    }

    impl DirectorioContado {
        fn nuevo(id: usize, estado: Rc<Cell<(usize, usize)>>) -> Self {
            let (activos, maximos) = estado.get();
            let activos = activos + 1;
            estado.set((activos, maximos.max(activos)));
            Self { id, estado }
        }
    }

    impl Drop for DirectorioContado {
        fn drop(&mut self) {
            let (activos, maximos) = self.estado.get();
            self.estado.set((activos - 1, maximos));
        }
    }

    struct BackendFanout {
        estado: Rc<Cell<(usize, usize)>>,
        siblings: usize,
    }

    impl BackendInventario for BackendFanout {
        type Directorio = DirectorioContado;
        type Archivo = ();

        fn nombres_con<V>(
            &mut self,
            directorio: &mut Self::Directorio,
            visitar: &mut V,
        ) -> Result<(), ErrorSnapshot>
        where
            V: FnMut(
                &mut Self,
                &mut Self::Directorio,
                std::ffi::OsString,
            ) -> Result<(), ErrorSnapshot>,
        {
            if directorio.id == 0 {
                for index in 0..self.siblings {
                    visitar(self, directorio, format!("d{index:03}").into())?;
                }
            }
            Ok(())
        }

        fn abrir(
            &mut self,
            _directorio: &Self::Directorio,
            nombre: &str,
        ) -> Result<EntradaAbierta<Self::Directorio, Self::Archivo>, ErrorSnapshot> {
            assert!(nombre.starts_with('d'));
            Ok(EntradaAbierta::Directorio(DirectorioContado::nuevo(
                1,
                Rc::clone(&self.estado),
            )))
        }

        fn leer(
            &mut self,
            (): Self::Archivo,
            _maximo_lectura: usize,
        ) -> Result<Vec<u8>, ErrorSnapshot> {
            unreachable!("fanout solo contiene directorios")
        }
    }

    #[test]
    fn fanout_dfs_no_retiene_handles_de_directorios_hermanos() {
        let estado = Rc::new(Cell::new((0, 0)));
        let backend = &mut BackendFanout {
            estado: Rc::clone(&estado),
            siblings: 500,
        };
        let root = DirectorioContado::nuevo(0, Rc::clone(&estado));

        let entries = caminar_inventario(backend, root).unwrap();

        assert_eq!(entries.len(), 500);
        assert_eq!(estado.get(), (0, 2));
    }

    #[cfg(unix)]
    #[test]
    fn rechaza_symlink_path_sin_leer_su_destino() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        fs::write(outside.path(), b"PRIVATE-CANARY").unwrap();
        symlink(outside.path(), root.path().join("link")).unwrap();

        let error = inventariar_root(root.path()).unwrap_err();
        assert_eq!(error, ErrorSnapshot::EnlaceNoPermitido);
        assert!(!error.to_string().contains("PRIVATE-CANARY"));
    }

    #[test]
    fn path_rechaza_enlace_en_ancestro_del_root_sin_leer_canary() {
        let temp = tempfile::tempdir().unwrap();
        let real = temp.path().join("real");
        let root_real = real.join("bundle");
        let alias = temp.path().join("alias");
        fs::create_dir_all(&root_real).unwrap();
        fs::write(root_real.join("sentinel.txt"), b"OUTSIDE-CANARY").unwrap();
        crear_junction(&alias, &real);

        let root_via_link = alias.join("bundle");
        let error = rechazo_path_sin_lectura(&root_via_link);

        assert_eq!(error, ErrorSnapshot::EnlaceNoPermitido);
        assert!(!error.to_string().contains("OUTSIDE-CANARY"));
        assert_eq!(
            fs::read(root_real.join("sentinel.txt")).unwrap(),
            b"OUTSIDE-CANARY"
        );
    }

    #[cfg(unix)]
    #[test]
    fn rechaza_symlink_leaf_sin_leer_su_destino_desde_handle() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        fs::write(outside.path(), b"PRIVATE-CANARY").unwrap();
        symlink(outside.path(), root.path().join("link")).unwrap();

        assert_eq!(
            inventariar_root_anclado(anclar(root.path()).abrir_raiz().unwrap()).unwrap_err(),
            ErrorSnapshot::EnlaceNoPermitido
        );
    }

    #[cfg(unix)]
    #[test]
    fn rechaza_fifo_desde_handle_sin_bloquear() {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;

        let root = tempfile::tempdir().unwrap();
        let fifo = root.path().join("fifo");
        let fifo_c = CString::new(fifo.as_os_str().as_bytes()).unwrap();
        // SAFETY: `fifo_c` is nul-terminated and mkfifo does not retain it.
        let status = unsafe { libc::mkfifo(fifo_c.as_ptr(), 0o600) };
        assert_eq!(
            status,
            0,
            "mkfifo failed: {}",
            std::io::Error::last_os_error()
        );

        assert_eq!(
            inventariar_root_anclado(anclar(root.path()).abrir_raiz().unwrap()).unwrap_err(),
            ErrorSnapshot::TipoNoAdmitido
        );
    }

    #[cfg(unix)]
    #[test]
    fn path_rechaza_fifo_sin_leerlo() {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;

        let root = tempfile::tempdir().unwrap();
        let fifo = root.path().join("fifo");
        let fifo_c = CString::new(fifo.as_os_str().as_bytes()).unwrap();
        // SAFETY: `fifo_c` is nul-terminated and mkfifo does not retain it.
        let status = unsafe { libc::mkfifo(fifo_c.as_ptr(), 0o600) };
        assert_eq!(
            status,
            0,
            "mkfifo failed: {}",
            std::io::Error::last_os_error()
        );

        assert_eq!(
            rechazo_path_sin_lectura(root.path()),
            ErrorSnapshot::TipoNoAdmitido
        );
    }

    #[cfg(unix)]
    #[test]
    fn lector_path_rechaza_fifo_sin_bloquear() {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;
        use std::sync::mpsc;
        use std::time::Duration;

        let root = tempfile::tempdir().unwrap();
        let fifo = root.path().join("fifo");
        let fifo_c = CString::new(fifo.as_os_str().as_bytes()).unwrap();
        // SAFETY: `fifo_c` is nul-terminated and mkfifo does not retain it.
        let status = unsafe { libc::mkfifo(fifo_c.as_ptr(), 0o600) };
        assert_eq!(
            status,
            0,
            "mkfifo failed: {}",
            std::io::Error::last_os_error()
        );

        let flags = flags_lectura_regular_unix();
        assert_ne!(flags & libc::O_NONBLOCK, 0);
        assert_ne!(flags & libc::O_NOFOLLOW, 0);
        assert_ne!(flags & libc::O_NOCTTY, 0);
        assert_ne!(flags & libc::O_CLOEXEC, 0);

        // Ejecutar la apertura en un worker permite que el test señale una
        // regresión que vuelva bloqueante al open de una FIFO sin writer.
        let (enviar, recibir) = mpsc::channel();
        std::thread::spawn(move || {
            let resultado = leer_archivo_regular_limitado(&fifo, 16);
            let _ = enviar.send(resultado);
        });
        let resultado = recibir
            .recv_timeout(Duration::from_secs(2))
            .expect("abrir una FIFO sin writer quedó bloqueado");
        assert_eq!(resultado.unwrap_err(), ErrorSnapshot::TipoNoAdmitido);
    }

    #[cfg(unix)]
    #[test]
    fn path_y_handle_rechazan_socket_sin_leerlo() {
        use std::os::unix::net::UnixListener;

        let root = tempfile::tempdir().unwrap();
        let _listener = UnixListener::bind(root.path().join("socket")).unwrap();

        assert_eq!(
            rechazo_path_sin_lectura(root.path()),
            ErrorSnapshot::TipoNoAdmitido
        );
        assert!(matches!(
            rechazo_handle_sin_lectura(root.path()),
            ErrorSnapshot::TipoNoAdmitido | ErrorSnapshot::Io
        ));
    }

    #[cfg(windows)]
    #[test]
    fn rechaza_reparse_leaf_sin_leer_su_destino_desde_handle() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("sentinel.txt"), b"PRIVATE-CANARY").unwrap();
        crear_junction(&root.path().join("junction"), outside.path());

        assert_eq!(
            inventariar_root_anclado(anclar(root.path()).abrir_raiz().unwrap()).unwrap_err(),
            ErrorSnapshot::EnlaceNoPermitido
        );
    }

    #[test]
    fn reemplazar_ancestro_por_junction_no_desvia_un_root_ya_anclado() {
        let temp = tempfile::tempdir().unwrap();
        let parent = temp.path().join("parent");
        let moved = temp.path().join("parent-moved");
        let outside = temp.path().join("outside");
        let root = parent.join("bundle");
        let outside_root = outside.join("bundle");
        fs::create_dir(&parent).unwrap();
        fs::create_dir_all(&outside_root).unwrap();
        fs::write(outside_root.join("sentinel.txt"), b"outside only").unwrap();

        let ubicacion = UbicacionRaiz::nueva(&root).unwrap();
        let ancla = DirectorioAnclado::desde_ubicacion(&ubicacion).unwrap();
        fs::rename(&parent, &moved).unwrap();
        crear_junction(&parent, &outside);
        fs::create_dir_all(moved.join("bundle")).unwrap();
        fs::write(moved.join("bundle/original.txt"), b"inside original").unwrap();

        let entries = inventariar_root_anclado(ancla.abrir_raiz().unwrap()).unwrap();
        assert_eq!(
            entries
                .iter()
                .map(EntradaSnapshot::ruta)
                .collect::<Vec<_>>(),
            ["original.txt"]
        );
        assert_eq!(entries[0].bytes(), Some(b"inside original".as_slice()));
        assert!(
            !entries
                .iter()
                .any(|entrada| entrada.ruta().contains("sentinel"))
        );
        assert_eq!(
            fs::read(moved.join("bundle/original.txt")).unwrap(),
            b"inside original"
        );
        assert_eq!(
            fs::read(outside_root.join("sentinel.txt")).unwrap(),
            b"outside only"
        );
    }

    #[cfg(unix)]
    fn crear_junction(link: &Path, target: &Path) {
        std::os::unix::fs::symlink(target, link).unwrap();
    }

    #[cfg(windows)]
    fn crear_junction(link: &Path, target: &Path) {
        let output = std::process::Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .output()
            .expect("cmd crea junctions");
        assert!(
            output.status.success(),
            "no se pudo crear junction: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[cfg(unix)]
    #[test]
    fn rechaza_nombres_no_utf8_sin_normalizarlos_con_replacement() {
        use std::os::unix::ffi::OsStrExt;

        let root = tempfile::tempdir().unwrap();
        let path = root.path().join(std::ffi::OsStr::from_bytes(b"bad-\xff"));
        fs::write(path, b"raw").unwrap();
        assert_eq!(
            inventariar_root(root.path()).unwrap_err(),
            ErrorSnapshot::RutaNoUtf8
        );
        assert_eq!(
            inventariar_root_anclado(anclar(root.path()).abrir_raiz().unwrap()).unwrap_err(),
            ErrorSnapshot::RutaNoUtf8
        );
    }
}

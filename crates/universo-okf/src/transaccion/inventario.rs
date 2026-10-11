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

/// Único recorrido del inventario; los backends solo enumeran, abren y leen.
trait BackendInventario {
    type Directorio;
    type Archivo;

    fn nombres(
        &mut self,
        directorio: &mut Self::Directorio,
    ) -> Result<Vec<std::ffi::OsString>, ErrorSnapshot>;

    fn abrir(
        &mut self,
        directorio: &Self::Directorio,
        nombre: &str,
    ) -> Result<EntradaAbierta<Self::Directorio, Self::Archivo>, ErrorSnapshot>;

    fn leer(&mut self, archivo: Self::Archivo) -> Result<Vec<u8>, ErrorSnapshot>;
}

/// Lee una vez cada archivo y devuelve el inventario ordenado por bytes UTF-8.
pub(crate) fn inventariar_root(raiz: &Path) -> Result<Vec<EntradaSnapshot>, ErrorSnapshot> {
    inventariar_root_con_lector(raiz, leer_archivo_regular)
}

pub(crate) fn inventariar_root_con_lector<F>(
    raiz: &Path,
    leer: F,
) -> Result<Vec<EntradaSnapshot>, ErrorSnapshot>
where
    F: FnMut(&Path) -> Result<Vec<u8>, ErrorSnapshot>,
{
    let raiz = ruta_absoluta_lexica(raiz)?;
    validar_directorio_sin_enlaces(&raiz)?;
    caminar_inventario(&mut BackendRuta { leer }, raiz)
}

fn caminar_inventario<B: BackendInventario>(
    backend: &mut B,
    raiz: B::Directorio,
) -> Result<Vec<EntradaSnapshot>, ErrorSnapshot> {
    let mut pendientes = vec![(raiz, String::new())];
    let mut entradas = Vec::new();

    while let Some((mut directorio, prefijo)) = pendientes.pop() {
        // El backend hace una sola enumeración y devuelve solo nombres. La
        // validación de nombre y la ruta lógica son compartidas por ambos.
        for nombre in backend.nombres(&mut directorio)? {
            let nombre = nombre
                .into_string()
                .map_err(|_| ErrorSnapshot::RutaNoUtf8)?;
            validar_nombre_inventario(&nombre)?;
            let ruta = if prefijo.is_empty() {
                nombre.clone()
            } else {
                format!("{prefijo}/{nombre}")
            };

            match backend.abrir(&directorio, &nombre)? {
                EntradaAbierta::Directorio(hijo) => {
                    entradas.push(EntradaSnapshot::directorio(ruta.clone()));
                    pendientes.push((hijo, ruta));
                }
                EntradaAbierta::Archivo(archivo) => {
                    entradas.push(EntradaSnapshot::archivo(ruta, backend.leer(archivo)?));
                }
            }
        }
    }

    entradas.sort_by(|a, b| a.ruta().as_bytes().cmp(b.ruta().as_bytes()));
    Ok(entradas)
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
    F: FnMut(&Path) -> Result<Vec<u8>, ErrorSnapshot>,
{
    type Directorio = PathBuf;
    type Archivo = PathBuf;

    fn nombres(
        &mut self,
        directorio: &mut Self::Directorio,
    ) -> Result<Vec<std::ffi::OsString>, ErrorSnapshot> {
        validar_directorio_sin_enlaces(directorio)?;
        fs::read_dir(directorio)
            .map_err(|_| ErrorSnapshot::Io)?
            .map(|entrada| {
                entrada
                    .map(|entrada| entrada.file_name())
                    .map_err(|_| ErrorSnapshot::Io)
            })
            .collect()
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

    fn leer(&mut self, archivo: Self::Archivo) -> Result<Vec<u8>, ErrorSnapshot> {
        (self.leer)(&archivo)
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
    let mut opciones = OpenOptions::new();
    opciones.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opciones.custom_flags(libc::O_NOFOLLOW);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        opciones.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let mut archivo: File = opciones.open(ruta).map_err(|_| ErrorSnapshot::Io)?;
    let metadata = archivo.metadata().map_err(|_| ErrorSnapshot::Io)?;
    if es_enlace_o_reparse(&metadata) {
        return Err(ErrorSnapshot::EnlaceNoPermitido);
    }
    if !metadata.is_file() {
        return Err(ErrorSnapshot::TipoNoAdmitido);
    }
    let mut bytes = Vec::new();
    archivo
        .read_to_end(&mut bytes)
        .map_err(|_| ErrorSnapshot::Io)?;
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
    inventariar_root_anclado_con_lector(raiz, leer_handle)
}

pub(crate) fn inventariar_root_anclado_con_lector<F>(
    raiz: super::anclado::DirectorioAnclado,
    leer: F,
) -> Result<Vec<EntradaSnapshot>, ErrorSnapshot>
where
    F: FnMut(&mut File) -> Result<Vec<u8>, ErrorSnapshot>,
{
    caminar_inventario(&mut BackendAnclado { leer }, raiz)
}

struct BackendAnclado<F> {
    leer: F,
}

impl<F> BackendInventario for BackendAnclado<F>
where
    F: FnMut(&mut File) -> Result<Vec<u8>, ErrorSnapshot>,
{
    type Directorio = super::anclado::DirectorioAnclado;
    type Archivo = File;

    fn nombres(
        &mut self,
        directorio: &mut Self::Directorio,
    ) -> Result<Vec<std::ffi::OsString>, ErrorSnapshot> {
        directorio.read_dir().map_err(mapear_error_anclado)
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

    fn leer(&mut self, mut archivo: Self::Archivo) -> Result<Vec<u8>, ErrorSnapshot> {
        (self.leer)(&mut archivo)
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

fn leer_handle(archivo: &mut File) -> Result<Vec<u8>, ErrorSnapshot> {
    let mut bytes = Vec::new();
    archivo
        .read_to_end(&mut bytes)
        .map_err(|_| ErrorSnapshot::Io)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transaccion::anclado::DirectorioAnclado;
    use crate::transaccion::raiz::UbicacionRaiz;
    use crate::transaccion::snapshot::{TipoEntradaSnapshot, calcular_token_snapshot};
    use std::collections::HashMap;
    use std::io::Read;

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

        fn nombres(
            &mut self,
            directorio: &mut Self::Directorio,
        ) -> Result<Vec<std::ffi::OsString>, ErrorSnapshot> {
            self.enumeraciones[*directorio] += 1;
            Ok(if *directorio == 0 {
                vec!["sub".into()]
            } else {
                Vec::new()
            })
        }

        fn abrir(
            &mut self,
            _directorio: &Self::Directorio,
            nombre: &str,
        ) -> Result<EntradaAbierta<Self::Directorio, Self::Archivo>, ErrorSnapshot> {
            assert_eq!(nombre, "sub");
            Ok(EntradaAbierta::Directorio(1))
        }

        fn leer(&mut self, (): Self::Archivo) -> Result<Vec<u8>, ErrorSnapshot> {
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

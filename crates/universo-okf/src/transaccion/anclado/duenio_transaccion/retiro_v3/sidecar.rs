use super::{DuenioTransaccionAnclado, ErrorPasoRetiroV3, Fase};
use crate::transaccion::anclado::identidad_archivo::persistida::IdentidadArchivoPersistida;
use crate::transaccion::anclado::intent_persistido_v3::IntentRetiroPersistidoV3;
use crate::transaccion::anclado::{identidad_archivo, metadata_es_reparse, scope_filesystem};
use std::ffi::OsString;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

pub(super) fn nombre(
    owner: &DuenioTransaccionAnclado<'_>,
    fase: Fase,
) -> Result<OsString, ErrorPasoRetiroV3> {
    let ubicacion = owner._lock.ubicacion();
    match fase {
        Fase::A => ubicacion
            .journal()
            .as_path()
            .file_name()
            .map(OsString::from)
            .ok_or(ErrorPasoRetiroV3),
        Fase::B => crate::transaccion::anclado::contexto_admision::nombre_canonico(
            ubicacion.ruta().file_name().ok_or(ErrorPasoRetiroV3)?,
        )
        .map_err(|_| ErrorPasoRetiroV3),
        Fase::C => Err(ErrorPasoRetiroV3),
    }
}

pub(super) fn abrir(
    owner: &DuenioTransaccionAnclado<'_>,
    fase: Fase,
    intent: &IntentRetiroPersistidoV3,
) -> Result<File, ErrorPasoRetiroV3> {
    let parent = owner.parent_revalidado().map_err(|_| ErrorPasoRetiroV3)?;
    let file =
        super::super::lector_intent_canonico::abrir_intent_canonico(&parent, &nombre(owner, fase)?)
            .map_err(|_| ErrorPasoRetiroV3)?
            .ok_or(ErrorPasoRetiroV3)?;
    verificar(file, owner, fase, intent)
}

pub(super) fn verificar(
    mut file: File,
    owner: &DuenioTransaccionAnclado<'_>,
    fase: Fase,
    intent: &IntentRetiroPersistidoV3,
) -> Result<File, ErrorPasoRetiroV3> {
    let before = observar(&file, owner)?;
    file.seek(SeekFrom::Start(0))
        .map_err(|_| ErrorPasoRetiroV3)?;
    let journal_bytes;
    let expected = match fase {
        Fase::A => {
            journal_bytes = intent
                .metadata()
                .journal()
                .encode()
                .map_err(|_| ErrorPasoRetiroV3)?;
            journal_bytes.as_slice()
        }
        Fase::B => intent.contexto_original(),
        Fase::C => return Err(ErrorPasoRetiroV3),
    };
    if file.metadata().map_err(|_| ErrorPasoRetiroV3)?.len() != expected.len() as u64 {
        return Err(ErrorPasoRetiroV3);
    }
    let mut buffer = [0; 8192];
    for bytes in expected.chunks(buffer.len()) {
        file.read_exact(&mut buffer[..bytes.len()])
            .map_err(|_| ErrorPasoRetiroV3)?;
        if bytes != &buffer[..bytes.len()] {
            return Err(ErrorPasoRetiroV3);
        }
    }
    loop {
        match file.read(&mut buffer[..1]) {
            Ok(0) => break,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            _ => return Err(ErrorPasoRetiroV3),
        }
    }
    if before != observar(&file, owner)? {
        return Err(ErrorPasoRetiroV3);
    }
    match fase {
        Fase::A => intent
            .metadata()
            .validar_vinculo_journal_datos(&before, intent.metadata().journal()),
        Fase::B => {
            let (length, digest) = intent.metadata().evidencia_contexto_original();
            intent
                .metadata()
                .validar_vinculo_contexto_datos(&before, length, digest)
        }
        Fase::C => return Err(ErrorPasoRetiroV3),
    }
    .map_err(|_| ErrorPasoRetiroV3)?;
    Ok(file)
}

fn observar(
    file: &File,
    owner: &DuenioTransaccionAnclado<'_>,
) -> Result<IdentidadArchivoPersistida, ErrorPasoRetiroV3> {
    let meta = file.metadata().map_err(|_| ErrorPasoRetiroV3)?;
    if !meta.is_file()
        || metadata_es_reparse(&meta)
        || scope_filesystem(file).map_err(|_| ErrorPasoRetiroV3)? != owner.scope_parent
    {
        return Err(ErrorPasoRetiroV3);
    }
    IdentidadArchivoPersistida::try_from(identidad_archivo(file).map_err(|_| ErrorPasoRetiroV3)?)
        .map_err(|_| ErrorPasoRetiroV3)
}

//! Sólo detecta nombres reservados; nunca infiere autoridad para eliminarlos.
use super::DuenioTransaccionAnclado;
use std::ffi::OsStr;
impl DuenioTransaccionAnclado<'_> {
    pub(in crate::transaccion::anclado::duenio_transaccion) fn exigir_stages_ausentes(
        &self,
        propio: Option<&OsStr>,
    ) -> Result<(), ()> {
        self.exigir_stages_ausentes_con_limite(
            propio,
            crate::transaccion::inventario::LimitesInventario::CONTRATO.entradas,
        )
    }
    fn exigir_stages_ausentes_con_limite(
        &self,
        propio: Option<&OsStr>,
        max: usize,
    ) -> Result<(), ()> {
        let mut parent = self.parent_revalidado().map_err(|_| ())?;
        let result = (|| {
            let location = self._lock.ubicacion();
            let root = location.ruta().file_name().ok_or(())?;
            let backup_path = location.backup();
            let backup = backup_path.as_path().file_name().ok_or(())?;
            let mut entries = fs_at::read_dir(&mut parent).map_err(|_| ())?;
            for (count, entry) in (&mut entries).enumerate() {
                if count >= max {
                    return Err(());
                }
                let entry = entry.map_err(|_| ())?;
                let name = entry.name();
                if name != root
                    && name != backup
                    && location.permite_rename_sibling(name)
                    && Some(name) != propio
                {
                    return Err(());
                }
            }
            Ok(())
        })();
        self.parent_revalidado().map_err(|_| ())?;
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transaccion::anclado::duenio_transaccion::preparar_apply::{
        ResultadoPreparacionApply,
        tests::{config, delta, fixture},
    };
    use crate::transaccion::{captura::capturar_snapshot_soportado_anclado, lock::LockEscritura};
    #[test]
    fn namespace_reservado_bloquea_sin_borrar_y_enum_es_acotada() {
        let _serial = crate::transaccion::lock::serializar_forks_y_reacquisiciones_test();
        let (dir, location) = fixture();
        std::fs::write(dir.path().join("unrelated"), b"canary").unwrap();
        let mut lock = LockEscritura::adquirir_en(&location).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        owner.recuperar_pendiente_v2().unwrap();
        assert!(owner.exigir_stages_ausentes_con_limite(None, 0).is_err());
        let orphan = location.stage("0123456789abcdef0123456789abcdef").unwrap();
        std::fs::write(orphan.as_path(), b"unknown-reserved").unwrap();
        assert!(owner.recuperar_pendiente_v2().is_err());
        assert_eq!(
            std::fs::read(orphan.as_path()).unwrap(),
            b"unknown-reserved"
        );
        assert_eq!(
            std::fs::read(dir.path().join("unrelated")).unwrap(),
            b"canary"
        );
    }
    #[test]
    #[ignore = "writer exits before Context publication"]
    fn proceso_stage_huerfano() {
        let root = std::env::var_os("FELIX_TEST_ORPHAN_STAGE_ROOT").unwrap();
        let mut lock = LockEscritura::adquirir(std::path::Path::new(&root)).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let snapshot = capturar_snapshot_soportado_anclado(
            &owner.abrir_root_lectura().unwrap().unwrap(),
            config(),
        )
        .unwrap();
        let ResultadoPreparacionApply::Preparada(_prepared) = owner
            .preparar_apply(config(), snapshot.token(), &delta())
            .unwrap()
        else {
            panic!("reject");
        };
        std::process::exit(94);
    }
    #[test]
    fn corte_prejournal_preserva_stage_y_bloquea_nueva_aplicacion() {
        let _serial = crate::transaccion::lock::serializar_forks_y_reacquisiciones_test();
        let (dir, location) = fixture();
        let old = std::fs::read(location.ruta().join("n/node.md")).unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "transaccion::anclado::duenio_transaccion::recovery_v2::stages::tests::proceso_stage_huerfano", "--ignored", "--nocapture"])
            .env("FELIX_TEST_ORPHAN_STAGE_ROOT", location.ruta()).output().unwrap();
        assert_eq!(
            output.status.code(),
            Some(94),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let mut before: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        let mut lock = LockEscritura::adquirir_en(&location).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        assert!(owner.recuperar_pendiente_v2().is_err());
        let snapshot = capturar_snapshot_soportado_anclado(
            &owner.abrir_root_lectura().unwrap().unwrap(),
            config(),
        )
        .unwrap();
        assert!(
            owner
                .preparar_apply(config(), snapshot.token(), &delta())
                .is_err()
        );
        let mut after: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        before.sort();
        after.sort();
        assert_eq!(before, after);
        assert_eq!(
            std::fs::read(location.ruta().join("n/node.md")).unwrap(),
            old
        );
        assert!(!location.journal().as_path().exists());
    }
}

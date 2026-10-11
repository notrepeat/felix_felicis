use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use tempfile::{TempDir, tempdir};
use universo_core::{Atomo, BundlePath, Delta, Etiqueta, Sujeto, construir_candidato};

use crate::serializar::serializar_candidato;
use crate::transaccion::anclado::{DirectorioAnclado, IdentidadDirectorio};
use crate::transaccion::captura::{
    ConfigRawCapturada, Snapshot, capturar_snapshot, capturar_snapshot_con_cas_anclado,
};
use crate::transaccion::raiz::UbicacionRaiz;
use crate::transaccion::snapshot::{SnapshotToken, TipoEntradaSnapshot, calcular_digest_raw_tree};

use super::{
    StageAnclado, materializar_staging_anclado, planificar_staging, recargar_staging_anclado,
};

const VERSION_MOTOR: &str = "stage-bundle-test/1";
const POLICY_ID: &str = "stage-bundle-policy/1";
const DOCUMENTO_ORIGINAL: &str = "---\ntype: a\ntitle: Original\n---\ncanary body\n";
const DOCUMENTO_EDITADO: &str = "Edited";
const AUXILIARES_RELEASE: usize = 10_000;
const BYTES_AUXILIARES_RELEASE: u64 = 104_857_600;

struct BundleFixture {
    _temp: TempDir,
    root: PathBuf,
    manifiesto_raw: Arc<[u8]>,
    invariantes_raw: Arc<[u8]>,
    ancla: DirectorioAnclado,
}

impl BundleFixture {
    fn crear() -> Self {
        let temp = tempdir().unwrap();
        let root = temp.path().join("bundle");
        fs::create_dir_all(root.join("knowledge")).unwrap();
        fs::create_dir(root.join("auxiliary")).unwrap();
        fs::create_dir(root.join("empty-dir")).unwrap();
        fs::write(root.join("knowledge/a.md"), DOCUMENTO_ORIGINAL).unwrap();

        let manifiesto_raw: Arc<[u8]> = Arc::from(
            &b"version = \"stage-bundle-test/1\"\n\n[nodos]\nsortes = [\"a\", \"Retirado\", \"Patr\xc3\xb3n\"]\n\n[layout]\ndirectorios = [\"knowledge\"]\nsin_iota = []\nignorados = []\n"[..],
        );
        let invariantes_raw: Arc<[u8]> = Arc::from(&b"\n"[..]);
        let manifiesto_path = temp.path().join("manifest.toml");
        let invariantes_path = temp.path().join("invariants.smt2");
        fs::write(&manifiesto_path, &manifiesto_raw).unwrap();
        fs::write(&invariantes_path, &invariantes_raw).unwrap();

        let ubicacion = UbicacionRaiz::nueva(&root).unwrap();
        let ancla = DirectorioAnclado::desde_ubicacion(&ubicacion).unwrap();

        Self {
            _temp: temp,
            root,
            manifiesto_raw,
            invariantes_raw,
            ancla,
        }
    }

    fn configuracion(
        manifiesto_raw: &Arc<[u8]>,
        invariantes_raw: &Arc<[u8]>,
    ) -> ConfigRawCapturada {
        ConfigRawCapturada::nueva(
            Arc::clone(manifiesto_raw),
            Arc::clone(invariantes_raw),
            None,
            None,
        )
    }

    fn capturar_snapshot_actual(&self) -> Snapshot {
        let manifiesto_path = self._temp.path().join("manifest.toml");
        let invariantes_path = self._temp.path().join("invariants.smt2");
        let token_esperado = capturar_snapshot(
            &self.root,
            &manifiesto_path,
            &invariantes_path,
            VERSION_MOTOR,
            POLICY_ID,
        )
        .unwrap()
        .token();
        capturar_snapshot_con_cas_anclado(
            &self.ancla,
            Self::configuracion(&self.manifiesto_raw, &self.invariantes_raw),
            VERSION_MOTOR,
            POLICY_ID,
            token_esperado,
        )
        .unwrap()
    }

    fn recapturar_root(&self, esperado: SnapshotToken) -> Snapshot {
        capturar_snapshot_con_cas_anclado(
            &self.ancla,
            Self::configuracion(&self.manifiesto_raw, &self.invariantes_raw),
            VERSION_MOTOR,
            POLICY_ID,
            esperado,
        )
        .unwrap()
    }

    fn stage_sibling_path(&self, stage: &StageAnclado) -> PathBuf {
        self.root.parent().unwrap().join(stage.componente())
    }
}

#[test]
fn materializa_bundle_anclado_con_recibo_y_cleanup_explicito() {
    let fixture = BundleFixture::crear();
    fs::write(fixture.root.join("auxiliary/canary.bin"), [0, 255, 17, 42]).unwrap();
    fs::write(fixture.root.join("auxiliary/empty.bin"), []).unwrap();

    // La captura se toma después de preparar los auxiliares y usa la identidad
    // de root que conservará la capability durante toda la operación.
    let snapshot = fixture.capturar_snapshot_actual();
    let digest_snapshot = calcular_digest_raw_tree(snapshot.entradas()).unwrap();
    assert!(snapshot.raw_tree_digest() == &digest_snapshot);
    let identidad_root = fixture.ancla.identidad_raiz();
    let path = BundlePath::nuevo("knowledge/a").unwrap();
    let anterior = Atomo::HasLab {
        sujeto: Sujeto::Nodo(path.clone()),
        etiqueta: Etiqueta::nuevo("Original"),
    };
    let actualizado = Atomo::HasLab {
        sujeto: Sujeto::Nodo(path),
        etiqueta: Etiqueta::nuevo(DOCUMENTO_EDITADO),
    };
    let delta = Delta::nuevo([actualizado], [anterior]).unwrap();
    let candidato = construir_candidato(snapshot.estructura(), &delta).unwrap();
    let documentos = serializar_candidato(snapshot.estructura(), &candidato, &[]).unwrap();
    let plan = planificar_staging(&snapshot, &documentos).unwrap();
    let root_token = snapshot.token();

    let stage = materializar_staging_anclado(&fixture.ancla, &snapshot, &plan, &candidato).unwrap();
    let sibling = fixture.stage_sibling_path(&stage);
    let recibo = stage.recibo().unwrap();
    let stage_reabierto =
        DirectorioAnclado::desde_ubicacion(&UbicacionRaiz::nueva(&sibling).unwrap()).unwrap();

    assert_eq!(recibo.stage_id(), stage.stage_id());
    assert_eq!(recibo.identidad(), stage.identidad());
    assert_eq!(recibo.identidad(), stage_reabierto.identidad_raiz());
    let stage_snapshot = recargar_staging_anclado(&snapshot, &plan, &stage)
        .unwrap_or_else(|_| panic!("el stage recién escrito debe recargar"));
    assert!(recibo.raw_tree_digest() == stage_snapshot.raw_tree_digest());
    let identidad_journal = stage.identidad().map(IdentidadDirectorio::journal_identity);
    assert!(recibo.identidad_journal() == identidad_journal.as_ref().and_then(Option::as_ref));
    assert_eq!(recibo.snapshot(), stage_snapshot.token());
    assert_ne!(recibo.snapshot(), root_token);
    assert!(sibling.join("empty-dir").is_dir());
    assert_eq!(
        fs::read(sibling.join("auxiliary/canary.bin")).unwrap(),
        [0, 255, 17, 42]
    );
    assert_eq!(fs::read(sibling.join("auxiliary/empty.bin")).unwrap(), b"");
    assert!(
        String::from_utf8(fs::read(sibling.join("knowledge/a.md")).unwrap())
            .unwrap()
            .contains(DOCUMENTO_EDITADO)
    );
    assert_eq!(fixture.ancla.identidad_raiz(), identidad_root);

    stage.limpiar().unwrap();
    assert!(!sibling.exists());
    assert_eq!(
        fs::read(fixture.root.join("knowledge/a.md")).unwrap(),
        DOCUMENTO_ORIGINAL.as_bytes()
    );
    assert_eq!(
        fs::read(fixture.root.join("auxiliary/canary.bin")).unwrap(),
        [0, 255, 17, 42]
    );
    assert_eq!(
        fs::read(fixture.root.join("auxiliary/empty.bin")).unwrap(),
        b""
    );
    assert!(fixture.root.join("empty-dir").is_dir());
    let root_recapturado = fixture.recapturar_root(root_token);
    assert_eq!(root_recapturado.token(), root_token);
    assert_eq!(root_recapturado.identidad_root(), identidad_root);
}

#[test]
#[ignore = "medición de staging release: 10.000 auxiliares y 100 MiB exactos"]
fn runtime_staging_anclado_mide_10000_auxiliares_100_mib_exactos() {
    let fixture = BundleFixture::crear();
    let auxiliary = fixture.root.join("auxiliary");
    let mut bytes_preparados = 0_u64;
    for indice in 0..AUXILIARES_RELEASE {
        let longitud = 10_485 + usize::from(indice < 7_600);
        let contenido = vec![u8::try_from(indice % 251).unwrap(); longitud];
        let ruta = auxiliary.join(format!("aux-{indice:05}.bin"));
        fs::write(ruta, &contenido).unwrap();
        bytes_preparados += u64::try_from(contenido.len()).unwrap();
    }
    assert_eq!(bytes_preparados, BYTES_AUXILIARES_RELEASE);

    // Captura, plan y candidato se preparan fuera del temporizador.
    let snapshot = fixture.capturar_snapshot_actual();
    assert_auxiliary_snapshot_shape(&snapshot);
    let path = BundlePath::nuevo("knowledge/a").unwrap();
    let anterior = Atomo::HasLab {
        sujeto: Sujeto::Nodo(path.clone()),
        etiqueta: Etiqueta::nuevo("Original"),
    };
    let actualizado = Atomo::HasLab {
        sujeto: Sujeto::Nodo(path),
        etiqueta: Etiqueta::nuevo(DOCUMENTO_EDITADO),
    };
    let delta = Delta::nuevo([actualizado], [anterior]).unwrap();
    let candidato = construir_candidato(snapshot.estructura(), &delta).unwrap();
    let documentos = serializar_candidato(snapshot.estructura(), &candidato, &[]).unwrap();
    let plan = planificar_staging(&snapshot, &documentos).unwrap();
    let count_plan = plan.auxiliares.len();
    let bytes_plan = plan
        .auxiliares
        .iter()
        .map(|entrada| u64::try_from(entrada.bytes().unwrap().len()).unwrap())
        .sum::<u64>();
    assert_eq!(count_plan, AUXILIARES_RELEASE);
    assert_eq!(bytes_plan, BYTES_AUXILIARES_RELEASE);

    let identidad_root = fixture.ancla.identidad_raiz();
    let root_token = snapshot.token();
    let inicio = Instant::now();
    let stage = materializar_staging_anclado(&fixture.ancla, &snapshot, &plan, &candidato).unwrap();
    let elapsed = inicio.elapsed();

    let sibling = fixture.stage_sibling_path(&stage);
    let auxiliares_stage = sibling.join("auxiliary");
    let mut count_stage = 0_usize;
    let mut bytes_stage = 0_u64;
    for entry in fs::read_dir(&auxiliares_stage).unwrap() {
        let entry = entry.unwrap();
        let metadata = entry.metadata().unwrap();
        assert!(metadata.is_file());
        count_stage += 1;
        bytes_stage += metadata.len();
    }
    assert_eq!(count_stage, AUXILIARES_RELEASE);
    assert_eq!(bytes_stage, BYTES_AUXILIARES_RELEASE);
    assert_eq!(stage.recibo().unwrap().stage_id(), stage.stage_id());
    assert_eq!(stage.recibo().unwrap().identidad(), stage.identidad());
    assert_ne!(stage.recibo().unwrap().snapshot(), root_token);
    drop(plan);
    drop(snapshot);
    drop(candidato);

    let segundos = elapsed.as_secs_f64();
    let mib = BYTES_AUXILIARES_RELEASE as f64 / (1024.0 * 1024.0);
    let mb = BYTES_AUXILIARES_RELEASE as f64 / 1_000_000.0;
    let mib_por_segundo = if segundos > 0.0 {
        mib / segundos
    } else {
        f64::INFINITY
    };
    let mb_por_segundo = if segundos > 0.0 {
        mb / segundos
    } else {
        f64::INFINITY
    };
    println!(
        "runtime_staging_anclado_mide_10000_auxiliares_100_mib_exactos: auxiliares={} bytes={} ms={:.3} MB/s={:.3} MiB/s={:.3} (orientativo; sin SLA)",
        count_stage,
        bytes_stage,
        elapsed.as_secs_f64() * 1_000.0,
        mb_por_segundo,
        mib_por_segundo,
    );

    stage.limpiar().unwrap();
    assert!(!sibling.exists());
    let root_recapturado = fixture.recapturar_root(root_token);
    assert_eq!(root_recapturado.token(), root_token);
    assert_eq!(fixture.ancla.identidad_raiz(), identidad_root);
    assert_eq!(root_recapturado.identidad_root(), identidad_root);
    assert_eq!(
        fs::read(auxiliary.join("aux-00000.bin")).unwrap().len(),
        10_486
    );
    assert_eq!(
        fs::read(auxiliary.join("aux-09999.bin")).unwrap().len(),
        10_485
    );
}

fn assert_auxiliary_snapshot_shape(snapshot: &Snapshot) {
    let auxiliares = snapshot
        .entradas()
        .iter()
        .filter(|entrada| {
            entrada.ruta().starts_with("auxiliary/")
                && entrada.tipo() == TipoEntradaSnapshot::Archivo
        })
        .collect::<Vec<_>>();
    assert_eq!(auxiliares.len(), AUXILIARES_RELEASE);
    assert_eq!(
        auxiliares
            .iter()
            .map(|entrada| u64::try_from(entrada.bytes().unwrap().len()).unwrap())
            .sum::<u64>(),
        BYTES_AUXILIARES_RELEASE
    );
}

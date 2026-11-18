//! Recovery de un apply que crea notas (delta v2), cortado en cada paso de la
//! instalación: el journal guarda el delta v2 y el replay lo reproduce.
use super::super::preparar_apply::{
    ResultadoPreparacionApply,
    tests::{config, fixture},
};
use super::*;
use crate::transaccion::{
    captura::capturar_snapshot_soportado_anclado, lock::LockEscritura, snapshot::SnapshotToken,
};
use std::fs;
use universo_core::{Atomo, BundlePath, Delta, Etiqueta, NotaNueva, Sorte};

fn delta_notas() -> Delta {
    Delta::nuevo(
        [Atomo::Type {
            sujeto: BundlePath::nuevo("n/node").unwrap(),
            sorte: Sorte::nuevo("b"),
        }],
        [Atomo::Type {
            sujeto: BundlePath::nuevo("n/node").unwrap(),
            sorte: Sorte::nuevo("a"),
        }],
    )
    .unwrap()
    .con_notas([NotaNueva {
        path: BundlePath::nuevo("n/nueva").unwrap(),
        tipo: Sorte::nuevo("b"),
        etiqueta: Etiqueta::nuevo("Nueva"),
        cuerpo: "Creada por delta.\n".to_string(),
    }])
    .unwrap()
}

#[test]
#[ignore = "writer exits at an install step of a note-creating apply"]
fn proceso_apply_notas_corte() {
    let root = std::env::var_os("FELIX_TEST_NOTAS_ROOT").unwrap();
    let paso: u8 = std::env::var("FELIX_TEST_NOTAS_PASO")
        .unwrap()
        .parse()
        .unwrap();
    let mut lock = LockEscritura::adquirir(std::path::Path::new(&root)).unwrap();
    let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
    let snapshot = capturar_snapshot_soportado_anclado(
        &owner.abrir_root_lectura().unwrap().unwrap(),
        config(),
    )
    .unwrap();
    let token: SnapshotToken = snapshot.token();
    let ResultadoPreparacionApply::Preparada(prepared) = owner
        .preparar_apply(config(), token, &delta_notas())
        .unwrap()
    else {
        panic!("reject");
    };
    let result = prepared
        .publicar_prepared()
        .unwrap()
        .instalar_con_hook(|step| {
            if step == paso {
                std::process::exit(100 + i32::from(step));
            }
        });
    assert!(result.is_err(), "cut not reached");
}

#[test]
fn recovery_de_un_apply_que_crea_notas_en_cada_corte() {
    for paso in 1..=4u8 {
        let _serial = crate::transaccion::lock::serializar_forks_y_reacquisiciones_test();
        let (_dir, location) = fixture();
        let root = location.ruta().to_path_buf();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "transaccion::anclado::duenio_transaccion::instalar_prepared::notas_recovery_tests::proceso_apply_notas_corte",
                "--ignored",
                "--nocapture",
            ])
            .env("FELIX_TEST_NOTAS_ROOT", &root)
            .env("FELIX_TEST_NOTAS_PASO", paso.to_string())
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(100 + i32::from(paso)),
            "paso {paso}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        crate::transaccion::estado::recuperar(&root)
            .unwrap_or_else(|e| panic!("paso {paso}: recover {}", e.codigo()));
        assert!(
            crate::transaccion::estado::inspeccionar(&root)
                .unwrap()
                .limpio(),
            "paso {paso}"
        );
        let nodo = fs::read_to_string(root.join("n/node.md")).unwrap();
        let nueva = root.join("n/nueva.md");
        let nuevo = nueva.exists();
        // Todo o nada: nota y cambio de tipo juntos.
        assert_eq!(nuevo, nodo.contains("type: b"), "paso {paso}: {nodo}");
        if nuevo {
            let texto = fs::read_to_string(&nueva).unwrap();
            assert!(texto.contains("title: Nueva") && texto.ends_with("Creada por delta.\n"));
        }
        // 1-3: el journal no llegó a Installed, se descarta New y se restaura
        // Old. 4: Installed, el recovery reproduce el delta v2 (replay) y
        // conserva New sólo si coincide con la huella del journal.
        assert_eq!(nuevo, paso == 4, "paso {paso}");
        assert_eq!(
            fs::read(root.join("payload.bin")).unwrap(),
            b"raw auxiliary"
        );
        // El bundle resultante vuelve a cargar sin errores.
        let manifiesto =
            crate::parsear_manifiesto(std::str::from_utf8(config().manifiesto_raw()).unwrap())
                .unwrap();
        crate::cargar_bundle(&root, manifiesto).unwrap_or_else(|e| panic!("paso {paso}: {e:?}"));
    }
}

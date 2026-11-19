use super::*;
use crate::transaccion::lock::LockEscritura;
use crate::transaccion::raiz::UbicacionRaiz;
use std::cell::Cell;
use std::fs;
use std::rc::Rc;
use tempfile::tempdir;

struct CuerpoConDescarte {
    resultado: Result<(), super::super::journal_temporal::ErrorJournalAnclado>,
    descartado: Rc<Cell<bool>>,
}

impl Drop for CuerpoConDescarte {
    fn drop(&mut self) {
        self.descartado.set(true);
    }
}

#[test]
fn helper_retiene_cuerpo_y_error_hasta_despues_del_postcheck() {
    for falla_postcheck in [false, true] {
        let temp = tempdir().unwrap();
        let raiz = temp.path().join("bundle");
        fs::create_dir(&raiz).unwrap();
        let ubicacion = UbicacionRaiz::nueva(&raiz).unwrap();
        let mut lock = LockEscritura::adquirir_en(&ubicacion).unwrap();
        let mut owner = DuenioTransaccionAnclado::desde_lock(&mut lock).unwrap();
        let descartado = Rc::new(Cell::new(false));
        let observado_en_post = Rc::clone(&descartado);

        let resultado = ejecutar_con_owner_y_postcheck(
            &mut owner,
            |_| {},
            |_| CuerpoConDescarte {
                resultado: Err(
                    super::super::journal_temporal::ErrorJournalAnclado::ParentIdentityMismatch,
                ),
                descartado: Rc::clone(&descartado),
            },
            |_, cuerpo| {
                assert!(matches!(
                    &cuerpo.resultado,
                    Err(
                        super::super::journal_temporal::ErrorJournalAnclado::ParentIdentityMismatch
                    )
                ));
                assert!(!observado_en_post.get());
                if falla_postcheck {
                    Err(super::super::journal_temporal::ErrorJournalAnclado::Io)
                } else {
                    Ok(())
                }
            },
        );

        match (&resultado, falla_postcheck) {
            (ResultadoOwnerPostcheck::PostcheckNoVerificado { cuerpo, error }, true) => {
                assert_eq!(
                    *error,
                    super::super::journal_temporal::ErrorJournalAnclado::Io
                );
                assert!(matches!(
                    &cuerpo.resultado,
                    Err(
                        super::super::journal_temporal::ErrorJournalAnclado::ParentIdentityMismatch
                    )
                ));
            }
            (ResultadoOwnerPostcheck::PostcheckVerificado { cuerpo }, false) => {
                assert!(matches!(
                    &cuerpo.resultado,
                    Err(
                        super::super::journal_temporal::ErrorJournalAnclado::ParentIdentityMismatch
                    )
                ));
            }
            _ => panic!(),
        }
        assert!(!descartado.get());
        drop(resultado);
        assert!(descartado.get());
    }
}

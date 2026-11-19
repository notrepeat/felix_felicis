//! Cursor binario prestado para leer longitudes y payloads con cotas.

use std::str;

use super::ErrorCodecAdmision;

pub(super) struct Cursor<'a> {
    bytes: &'a [u8],
    posicion: usize,
}

impl<'a> Cursor<'a> {
    pub(super) fn nuevo(bytes: &'a [u8]) -> Self {
        Self { bytes, posicion: 0 }
    }

    pub(super) fn posicion(&self) -> usize {
        self.posicion
    }

    pub(super) fn take(&mut self, longitud: usize) -> Result<&'a [u8], ErrorCodecAdmision> {
        let fin = self
            .posicion
            .checked_add(longitud)
            .ok_or(ErrorCodecAdmision::LongitudExcedida)?;
        let valor = self
            .bytes
            .get(self.posicion..fin)
            .ok_or(ErrorCodecAdmision::FramingInvalido)?;
        self.posicion = fin;
        Ok(valor)
    }

    pub(super) fn u8(&mut self) -> Result<u8, ErrorCodecAdmision> {
        Ok(self.take(1)?[0])
    }

    pub(super) fn u16(&mut self) -> Result<u16, ErrorCodecAdmision> {
        let bytes: [u8; 2] = self
            .take(2)?
            .try_into()
            .map_err(|_| ErrorCodecAdmision::FramingInvalido)?;
        Ok(u16::from_be_bytes(bytes))
    }

    pub(super) fn u32(&mut self) -> Result<u32, ErrorCodecAdmision> {
        let bytes: [u8; 4] = self
            .take(4)?
            .try_into()
            .map_err(|_| ErrorCodecAdmision::FramingInvalido)?;
        Ok(u32::from_be_bytes(bytes))
    }

    pub(super) fn u64(&mut self) -> Result<u64, ErrorCodecAdmision> {
        let bytes: [u8; 8] = self
            .take(8)?
            .try_into()
            .map_err(|_| ErrorCodecAdmision::FramingInvalido)?;
        Ok(u64::from_be_bytes(bytes))
    }

    pub(super) fn longitud_u64(&mut self) -> Result<usize, ErrorCodecAdmision> {
        usize::try_from(self.u64()?).map_err(|_| ErrorCodecAdmision::LongitudExcedida)
    }

    pub(super) fn bytes_u64(
        &mut self,
        usados: &mut usize,
        limite: usize,
    ) -> Result<&'a [u8], ErrorCodecAdmision> {
        let longitud = self.longitud_u64()?;
        cobrar(usados, longitud, limite)?;
        self.take(longitud)
    }

    pub(super) fn utf8_u32_bounded(
        &mut self,
        usados: &mut usize,
        limite: usize,
    ) -> Result<&'a str, ErrorCodecAdmision> {
        let longitud =
            usize::try_from(self.u32()?).map_err(|_| ErrorCodecAdmision::LongitudExcedida)?;
        cobrar(usados, longitud, limite)?;
        str::from_utf8(self.take(longitud)?).map_err(|_| ErrorCodecAdmision::FramingInvalido)
    }

    pub(super) fn utf8_u64(&mut self, limite: usize) -> Result<&'a str, ErrorCodecAdmision> {
        let longitud = self.longitud_u64()?;
        if longitud > limite {
            return Err(ErrorCodecAdmision::LimiteComponenteExcedido);
        }
        str::from_utf8(self.take(longitud)?).map_err(|_| ErrorCodecAdmision::FramingInvalido)
    }

    pub(super) fn terminar(&self) -> Result<(), ErrorCodecAdmision> {
        if self.posicion == self.bytes.len() {
            Ok(())
        } else {
            Err(ErrorCodecAdmision::FramingInvalido)
        }
    }
}

fn cobrar(total: &mut usize, incremento: usize, limite: usize) -> Result<(), ErrorCodecAdmision> {
    let nuevo_total = total
        .checked_add(incremento)
        .ok_or(ErrorCodecAdmision::LongitudExcedida)?;
    if nuevo_total > limite {
        return Err(ErrorCodecAdmision::LimiteComponenteExcedido);
    }
    *total = nuevo_total;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lee_enteros_big_endian_y_payload_prestado() {
        let bytes = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];
        let mut cursor = Cursor::nuevo(&bytes);
        assert_eq!(cursor.u16().unwrap(), 0x0102);
        assert_eq!(cursor.u32().unwrap(), 0x0304_0506);
        assert_eq!(cursor.u16().unwrap(), 0x0708);
        cursor.terminar().unwrap();

        let texto = b"abc";
        let mut wire = (texto.len() as u64).to_be_bytes().to_vec();
        wire.extend_from_slice(texto);
        let mut cursor = Cursor::nuevo(&wire);
        let prestado = cursor.utf8_u64(3).unwrap();
        assert_eq!(prestado, "abc");
        assert_eq!(prestado.as_ptr(), wire[8..].as_ptr());
        cursor.terminar().unwrap();
    }

    #[test]
    fn rechaza_truncado_y_longitud_fuera_de_cuota_antes_de_leer_payload() {
        let mut truncado = Cursor::nuevo(&[0, 0, 0]);
        assert_eq!(
            truncado.u32().err(),
            Some(ErrorCodecAdmision::FramingInvalido)
        );

        let mut wire = 3_u64.to_be_bytes().to_vec();
        wire.extend_from_slice(b"ab");
        let mut cursor = Cursor::nuevo(&wire);
        let mut usados = 0;
        assert_eq!(
            cursor.bytes_u64(&mut usados, 2).err(),
            Some(ErrorCodecAdmision::LimiteComponenteExcedido)
        );
        assert_eq!(usados, 0);
        assert_eq!(cursor.posicion(), 8);
    }

    #[test]
    fn aplica_cuota_inclusiva_y_rechaza_utf8_invalido() {
        let mut wire = 2_u32.to_be_bytes().to_vec();
        wire.extend_from_slice(b"ok");
        let mut cursor = Cursor::nuevo(&wire);
        let mut usados = 0;
        assert_eq!(cursor.utf8_u32_bounded(&mut usados, 2).unwrap(), "ok");
        assert_eq!(usados, 2);

        let mut invalido = 1_u32.to_be_bytes().to_vec();
        invalido.push(0xff);
        let mut cursor = Cursor::nuevo(&invalido);
        assert_eq!(
            cursor.utf8_u32_bounded(&mut 0, 1).err(),
            Some(ErrorCodecAdmision::FramingInvalido)
        );
    }
}

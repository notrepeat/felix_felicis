#!/bin/sh
# Instalador de Felix Felicis para Linux y macOS. No necesita Rust: baja los
# binarios precompilados del release, verifica su suma SHA-256 y deja
# `universo` y `universo-mcp` en ~/.local/bin (o en FELIX_BIN_DIR).
#
# El workflow de release reemplaza __REPO__ y __VERSION__ y publica este
# archivo (y su .sha256) junto a los binarios; el de la rama no sirve tal cual.
#
# Variables: FELIX_BIN_DIR (carpeta de los binarios), FELIX_VERSION (otro
# tag), FELIX_BASE_URL (otra fuente de descarga) y FELIX_TARGET (forzar la
# plataforma; también salta el control de glibc, para pruebas).
set -eu

REPO="__REPO__"
VERSION="${FELIX_VERSION:-__VERSION__}"
DESTINO="${FELIX_BIN_DIR:-$HOME/.local/bin}"
BINARIOS="universo universo-mcp"

falla() {
  echo "felix: $*" >&2
  exit 1
}

case "$REPO$VERSION" in
  *__*)
    if [ -z "${FELIX_BASE_URL:-}" ]; then
      falla "este instalador no fue preparado por el release; descárgalo desde la página del release"
    fi
    ;;
esac
BASE="${FELIX_BASE_URL:-https://github.com/$REPO/releases/download/$VERSION}"

if [ -n "${FELIX_TARGET:-}" ]; then
  TARGET="$FELIX_TARGET"
else
  case "$(uname -s)-$(uname -m)" in
    Linux-x86_64) TARGET=x86_64-unknown-linux-gnu ;;
    Darwin-arm64) TARGET=aarch64-apple-darwin ;;
    *) TARGET="$(uname -s)-$(uname -m)" ;;
  esac
fi
case "$TARGET" in
  x86_64-unknown-linux-gnu | aarch64-apple-darwin) ;;
  *) falla "no hay binario para $TARGET; las opciones están en la guía de instalación" ;;
esac

# El binario de Linux necesita glibc 2.39 o posterior (no corre con musl).
if [ "$TARGET" = x86_64-unknown-linux-gnu ] && [ -z "${FELIX_TARGET:-}" ]; then
  if ls /lib/ld-musl-* >/dev/null 2>&1; then
    falla "este sistema usa musl (por ejemplo, Alpine); el binario necesita glibc 2.39 o posterior"
  fi
  glibc="$(getconf GNU_LIBC_VERSION 2>/dev/null | awk '{print $2}')"
  if [ -n "$glibc" ] && ! awk -v v="$glibc" 'BEGIN { split(v, p, "."); exit !(p[1] > 2 || (p[1] == 2 && p[2] >= 39)) }'; then
    falla "el binario necesita glibc 2.39 o posterior y este sistema tiene $glibc"
  fi
fi

descargar() {
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$1" -o "$2"
  elif command -v wget >/dev/null 2>&1; then
    wget -q -O "$2" "$1"
  else
    falla "hace falta curl o wget para descargar"
  fi
}

ARCHIVO="felix-felicis-$VERSION-$TARGET.tar.gz"
TEMPORAL="$(mktemp -d)"
limpiar() {
  rm -rf "$TEMPORAL"
  for binario in $BINARIOS; do rm -f "$DESTINO/.$binario.felix-nuevo"; done
}
trap limpiar EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

echo "felix: descargando $ARCHIVO"
descargar "$BASE/$ARCHIVO" "$TEMPORAL/$ARCHIVO" || falla "no se pudo descargar $BASE/$ARCHIVO"
descargar "$BASE/$ARCHIVO.sha256" "$TEMPORAL/$ARCHIVO.sha256" || falla "no se pudo descargar la suma"

if command -v sha256sum >/dev/null 2>&1; then
  verificar="sha256sum -c"
elif command -v shasum >/dev/null 2>&1; then
  verificar="shasum -a 256 -c"
else
  falla "hace falta sha256sum o shasum para verificar la descarga"
fi
(cd "$TEMPORAL" && $verificar "$ARCHIVO.sha256" >/dev/null 2>&1) ||
  falla "la suma SHA-256 no coincide; no se instaló nada"

tar -xzf "$TEMPORAL/$ARCHIVO" -C "$TEMPORAL"
ORIGEN="$TEMPORAL/felix-felicis-$VERSION-$TARGET"
for binario in $BINARIOS; do
  [ -f "$ORIGEN/$binario" ] || falla "el archivo no trae $binario; no se instaló nada"
done

# Primero se copian los dos al destino con nombre temporal (mismo sistema de
# archivos); recién entonces se renombran, así el reemplazo es atómico y no
# queda una versión mezclada.
mkdir -p "$DESTINO"
for binario in $BINARIOS; do
  cp "$ORIGEN/$binario" "$DESTINO/.$binario.felix-nuevo"
  chmod 755 "$DESTINO/.$binario.felix-nuevo"
done
for binario in $BINARIOS; do
  mv -f "$DESTINO/.$binario.felix-nuevo" "$DESTINO/$binario"
done

echo "felix: instalado en $DESTINO"
"$DESTINO/universo" --version || falla "universo no arranca en este sistema"
case ":$PATH:" in
  *":$DESTINO:"*) ;;
  *)
    case "${SHELL:-}" in
      */zsh) rc="~/.zshrc" ;;
      */fish) rc="" ;;
      *) rc="~/.bashrc" ;;
    esac
    echo "felix: $DESTINO no está en tu PATH; agrégalo a tu shell:"
    if [ -n "$rc" ]; then
      echo "  echo 'export PATH=\"$DESTINO:\$PATH\"' >> $rc"
    else
      echo "  fish_add_path $DESTINO"
    fi
    ;;
esac
echo "felix: para empezar:"
echo "  universo init mi-criterio --git && cd mi-criterio && universo conectar"

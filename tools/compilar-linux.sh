#!/usr/bin/env bash
# Compila la Biblioteca de traducciones para Linux y crea el AppImage (experimental).
#
# Para el AppImage que se reparte, mejor tools/compilar-linux-contenedor.sh: compila en Ubuntu 22.04 y
# mete dentro 7-Zip, xdelta3 y chdman. Este, solo, usa las herramientas del sistema donde se abra.
#
# Como tools/compilar.ps1, le dice al compilador que cambie por nombres genéricos las rutas de esta máquina
# (tu carpeta de usuario, la de Rust y la del proyecto), para que el ejecutable no las lleve dentro.
#
# Uso: tools/compilar-linux.sh            el AppImage, en src-tauri/target/release/bundle/appimage
#      tools/compilar-linux.sh --prueba   solo el ejecutable, sin optimizar y sin AppImage
set -euo pipefail

proyecto="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
her="$proyecto/src-tauri/recursos/herramientas-linux"
prueba=0
[[ "${1:-}" == "--prueba" ]] && prueba=1

aviso() { printf '\033[33mAVISO:\033[0m %s\n' "$*" >&2; }
fallo() { printf '\033[31mERROR:\033[0m %s\n' "$*" >&2; exit 1; }

# ---------------------------------------------------------------- lo que hace falta para compilar
command -v cargo >/dev/null || fallo "Falta Rust (cargo). Instálalo con rustup o con el paquete rust de tu distribución."
command -v npm >/dev/null || fallo "Falta Node.js (npm)."
command -v pkg-config >/dev/null || fallo "Falta pkg-config."
faltan=()
for p in webkit2gtk-4.1 javascriptcoregtk-4.1 libsoup-3.0 gtk+-3.0; do
    pkg-config --exists "$p" || faltan+=("$p")
done
if ((${#faltan[@]})); then
    fallo "Faltan bibliotecas de desarrollo: ${faltan[*]}.
  Arch/CachyOS:   sudo pacman -S --needed webkit2gtk-4.1 libsoup3 gtk3 base-devel
  Debian/Ubuntu:  sudo apt install libwebkit2gtk-4.1-dev libsoup-3.0-dev libgtk-3-dev build-essential
  Fedora:         sudo dnf install webkit2gtk4.1-devel libsoup3-devel gtk3-devel"
fi

# ---------------------------------------------------------------- herramientas de parcheo
# El AppImage solo lleva 7-Zip, xdelta3 y chdman al compilarlo en el contenedor (tools/compilar-linux-contenedor.sh,
# que pone BIBLIOTECA_HERRAMIENTAS=1): allí son binarios para cualquier x86-64 y glibc 2.35. Los de otras
# distribuciones pueden pedir una CPU o una glibc más nuevas (CachyOS, por ejemplo, los compila para AVX-512),
# o ser un chdman antiguo o que arrastra SDL2, PulseAudio y X11.
mkdir -p "$her"
find "$her" -mindepth 1 ! -name LICENCIAS.txt -delete
if [[ "${BIBLIOTECA_HERRAMIENTAS:-0}" == 1 ]]; then
    install -m 755 "$(command -v 7zz)" "$her/7zz"
    install -m 755 "$(command -v xdelta3)" "$her/xdelta3"
    install -m 755 "$(command -v chdman)" "$her/chdman"
    echo "Herramientas en el AppImage:"
    ls -l "$her"
else
    aviso "El AppImage no lleva herramientas: usará 7z, xdelta3 y chdman del sistema donde se abra."
fi

# ---------------------------------------------------------------- compilar
cargo_home="${CARGO_HOME:-$HOME/.cargo}"
rustup_home="${RUSTUP_HOME:-$HOME/.rustup}"
# CARGO_ENCODED_RUSTFLAGS separa las opciones con el carácter 0x1F: así valen rutas con espacios.
sep=$'\x1f'
export CARGO_ENCODED_RUSTFLAGS="--remap-path-prefix=$cargo_home=cargo$sep--remap-path-prefix=$rustup_home=rustup$sep--remap-path-prefix=$proyecto=biblioteca$sep--remap-path-prefix=$HOME=usuario"

# linuxdeploy (lo usa Tauri para el AppImage) es a su vez un AppImage: así funciona aunque no haya FUSE.
export APPIMAGE_EXTRACT_AND_RUN="${APPIMAGE_EXTRACT_AND_RUN:-1}"
# Su strip es antiguo y falla con las bibliotecas de Arch/CachyOS («unknown type [0x13] section
# `.relr.dyn'»): sin strip, el AppImage sale algo más grande pero sale.
export NO_STRIP="${NO_STRIP:-true}"

cd "$proyecto"
[[ -x node_modules/.bin/tauri ]] || npm ci
if ((prueba)); then
    npx tauri build --debug --no-bundle
else
    npx tauri build --bundles appimage
    echo
    echo "AppImage:"
    ls -l src-tauri/target/release/bundle/appimage/*.AppImage
fi

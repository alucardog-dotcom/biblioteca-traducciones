#!/usr/bin/env bash
# Crea el AppImage dentro de un contenedor de Ubuntu 22.04 (tools/linux/Containerfile), para que abra en
# todas las distribuciones con glibc 2.35 o más nueva. Usa Podman (o Docker, si no hay Podman).
#
# Dentro del contenedor se llama a tools/compilar-linux.sh. La compilación y node_modules van a volúmenes
# propios, así que no se mezclan con los de tu sistema; las herramientas que lleva el AppImage se copian,
# como siempre, a src-tauri/recursos/herramientas-linux (fuera de git).
#
# Uso: tools/compilar-linux-contenedor.sh            el AppImage, en src-tauri/target/contenedor
#      tools/compilar-linux-contenedor.sh --pruebas  antes, las pruebas (cargo test), como en GitHub Actions
set -euo pipefail

orden='tools/compilar-linux.sh && cp -v src-tauri/target/release/bundle/appimage/*.AppImage /salida/'
[[ "${1:-}" == "--pruebas" ]] && orden="cargo test --lib --manifest-path src-tauri/Cargo.toml && $orden"

proyecto="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
motor="$(command -v podman || command -v docker || true)"
[[ -n "$motor" ]] || { echo "ERROR: hace falta Podman (o Docker)." >&2; exit 1; }

imagen=biblioteca-traducciones-linux
salida="$proyecto/src-tauri/target/contenedor"
mkdir -p "$salida"

"$motor" build -t "$imagen" -f "$proyecto/tools/linux/Containerfile" "$proyecto/tools/linux"

# :z para SELinux (Fedora). Con Podman sin root, root dentro del contenedor es tu usuario fuera, así que lo
# que se escribe en el proyecto queda a tu nombre.
"$motor" run --rm \
    -v "$proyecto:/proyecto:z" \
    -v biblioteca-linux-target:/proyecto/src-tauri/target \
    -v biblioteca-linux-node:/proyecto/node_modules \
    -v biblioteca-linux-cargo:/usr/local/cargo/registry \
    -v "$salida:/salida:z" \
    -e BIBLIOTECA_HERRAMIENTAS=1 \
    "$imagen" \
    bash -c "$orden"

echo
echo "AppImage en $salida:"
ls -l "$salida"/*.AppImage

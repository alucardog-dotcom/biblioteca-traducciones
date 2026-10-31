<#
.SYNOPSIS
  Compila la Biblioteca de traducciones sin meter en el .exe las rutas de esta máquina (la carpeta
  del usuario, la de Rust y la del proyecto): el compilador las cambia por nombres genéricos.

.PARAMETER Prueba  Compila la versión de prueba (sin optimizar y sin instalador).
#>
param([switch]$Prueba)

$ErrorActionPreference = 'Stop'
$proyecto = Split-Path $PSScriptRoot -Parent
$cargo  = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE '.cargo' }
$rustup = if ($env:RUSTUP_HOME) { $env:RUSTUP_HOME } else { Join-Path $env:USERPROFILE '.rustup' }
$env:PATH = "$cargo\bin;$env:PATH"

# CARGO_ENCODED_RUSTFLAGS separa las opciones con el carácter 0x1F: así valen rutas con espacios.
$opciones = @(
    "--remap-path-prefix=$cargo=cargo",
    "--remap-path-prefix=$rustup=rustup",
    "--remap-path-prefix=$proyecto=biblioteca",
    "--remap-path-prefix=$env:USERPROFILE=usuario"
)
$env:CARGO_ENCODED_RUSTFLAGS = $opciones -join [char]0x1F

Set-Location $proyecto
if ($Prueba) { npx tauri build --debug --no-bundle } else { npx tauri build }
# Que un fallo al compilar no pase por bueno (en GitHub Actions, por ejemplo).
exit $LASTEXITCODE

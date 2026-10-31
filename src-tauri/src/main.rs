// Sin ventana de consola en Windows al abrir el programa.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    biblioteca_lib::run()
}

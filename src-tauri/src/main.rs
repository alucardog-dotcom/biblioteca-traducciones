// Sin ventana de consola en Windows al abrir el programa.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // WebKitGTK con su renderizador DMABUF pinta a veces mal la rejilla (tarjetas estrechadas o fuera
    // de sitio) en Wayland. Se desactiva salvo que el usuario ya haya fijado la variable.
    #[cfg(target_os = "linux")]
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }
    biblioteca_lib::run()
}

# Copia dentro del programa (src-tauri/recursos) el catálogo y las herramientas de parcheo.
# Hay que ejecutarlo antes de compilar cada vez que cambie el catálogo.
# Uso: python tools/preparar_recursos.py [carpeta de RetroBat, C:\RetroBat por defecto]
import os, shutil, sys

retrobat = sys.argv[1] if len(sys.argv) > 1 else r'C:\RetroBat'
aqui = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
destino = os.path.join(aqui, 'src-tauri', 'recursos')
trad = os.path.join(retrobat, 'traducciones')

# El parcheo lo hace el propio programa (motor.rs y parcheo.rs): de fuera solo necesita 7-Zip, chdman y
# xdelta3. Sus licencias están en recursos/herramientas/LICENCIAS.txt.
ficheros = {
    'catalogo': [os.path.join(trad, 'catalogo', n) for n in ('traducciones_fan.csv', 'centro_datos.json')],
    'herramientas': [os.path.join(trad, 'herramientas', 'xdelta3.exe')]
                    + [os.path.join(retrobat, 'emulationstation', n) for n in ('7z.exe', '7z.dll')]
                    + [os.path.join(retrobat, 'emulators', 'mame', 'chdman.exe')],
}
for carpeta, lista in ficheros.items():
    os.makedirs(os.path.join(destino, carpeta), exist_ok=True)
    for f in lista:
        shutil.copy2(f, os.path.join(destino, carpeta, os.path.basename(f)))
        print('%-12s %8d KB  %s' % (carpeta, os.path.getsize(f) // 1024, os.path.basename(f)))
    # Lo que ya no se usa no debe acabar dentro del instalador.
    quedan = {os.path.basename(f) for f in lista} | {'LICENCIAS.txt'}
    for n in os.listdir(os.path.join(destino, carpeta)):
        if n not in quedan:
            os.remove(os.path.join(destino, carpeta, n))
            print('%-12s quitado    %s' % (carpeta, n))

# Las portadas elegidas a mano para el catálogo (importar_portadas.py) van con la ventana, en ui/portadas:
# así salen sin conexión y sin depender de la página de donde se sacaron.
origen = os.path.join(trad, 'catalogo', 'portadas')
dest = os.path.join(aqui, 'ui', 'portadas')
if os.path.isdir(dest):
    shutil.rmtree(dest)
if os.path.isdir(origen):
    shutil.copytree(origen, dest)
    print('portadas     %8d KB  %d imágenes' % (sum(os.path.getsize(os.path.join(dest, n)) for n in os.listdir(dest)) // 1024, len(os.listdir(dest))))

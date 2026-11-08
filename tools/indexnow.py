# Avisa a IndexNow (Bing, DuckDuckGo, Ecosia, Yandex, Seznam, Naver…) de las páginas nuevas o cambiadas de la web.
# Compara el sitemap.xml recién generado por tools/web.py con el publicado y manda a api.indexnow.org solo las
# direcciones nuevas o con otra fecha (lastmod), más la portada, /catalogo/ y /novedades/. Solo usa la biblioteca
# estándar de Python.
#
# La clave es el único fichero de web/ con nombre de 32 cifras hexadecimales (<clave>.txt, con la clave dentro) y
# tiene que estar en la raíz de la web: IndexNow solo acepta direcciones que cuelguen de la carpeta de la clave.
# tools/web.py no la copia; lo hace --preparar en web.yml, antes de subir la web a GitHub Pages.
#
# Uso:
#   python tools/indexnow.py --sin-enviar [carpeta]   dice cuántas direcciones mandaría (carpeta: la de web.py, _web)
#   python tools/indexnow.py [carpeta]                las manda
#   python tools/indexnow.py --todas [carpeta]        manda todas las del sitemap, no solo las nuevas o cambiadas
# En web.yml, como después de desplegar el sitemap publicado ya es el nuevo, se guarda antes:
#   python3 tools/indexnow.py --preparar "$RUNNER_TEMP/sitemap-anterior.xml" _site     (antes de subir la web)
#   python3 tools/indexnow.py --anterior "$RUNNER_TEMP/sitemap-anterior.xml" _site     (después de desplegar)
import argparse, json, os, re, shutil, sys, time
import urllib.error, urllib.request
import xml.etree.ElementTree as ET
from urllib.parse import urlparse

RAIZ = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
WEB = os.path.join(RAIZ, 'web')
API = 'https://api.indexnow.org/indexnow'
MAXIMO = 10000  # direcciones por envío, según el protocolo
FIJAS = ('', 'catalogo/', 'novedades/')  # siempre se avisan: cambian con cada ficha nueva
AGENTE = 'biblioteca-traducciones-indexnow'
NS = '{http://www.sitemaps.org/schemas/sitemap/0.9}'


def base_web():
    """La dirección de la web, como la calcula tools/web.py (dominio de web/CNAME o la de GitHub Pages)."""
    cname = os.path.join(WEB, 'CNAME')
    dominio = open(cname, encoding='utf-8').read().strip() if os.path.isfile(cname) else ''
    return f'https://{dominio}/' if dominio else 'https://alucardog-dotcom.github.io/biblioteca-traducciones/'


def clave():
    """La clave de web/<clave>.txt (32 cifras hexadecimales, el mismo texto dentro)."""
    halladas = []
    for nombre in os.listdir(WEB):
        m = re.fullmatch(r'([0-9a-f]{32})\.txt', nombre)
        if m and open(os.path.join(WEB, nombre), encoding='utf-8').read().strip() == m.group(1):
            halladas.append(m.group(1))
    if len(halladas) != 1:
        sys.exit(f'IndexNow: hace falta una clave en web/ (<32 cifras hexadecimales>.txt) y hay {len(halladas)}.')
    return halladas[0]


def bajar(url):
    pet = urllib.request.Request(url, headers={'User-Agent': AGENTE})
    with urllib.request.urlopen(pet, timeout=60) as r:
        return r.read()


def leer_sitemap(datos):
    """{dirección: lastmod} de un sitemap.xml ('' si la página no lleva fecha)."""
    raiz = ET.fromstring(datos)
    return {(u.findtext(f'{NS}loc') or '').strip(): (u.findtext(f'{NS}lastmod') or '').strip()
            for u in raiz.iter(f'{NS}url')}


def anterior(origen):
    """El sitemap con el que se compara (fichero o dirección), o None si no se puede leer."""
    try:
        if re.match(r'https?://', origen):
            return leer_sitemap(bajar(origen))
        with open(origen, 'rb') as f:
            return leer_sitemap(f.read())
    except (OSError, urllib.error.URLError, ET.ParseError) as ex:
        print(f'IndexNow: no se ha podido leer el sitemap anterior ({origen}): {ex}')
        return None


def enviar(base, k, urls):
    host = urlparse(base).hostname
    for i in range(0, len(urls), MAXIMO):
        tanda = urls[i:i + MAXIMO]
        cuerpo = json.dumps({'host': host, 'key': k, 'keyLocation': f'{base}{k}.txt', 'urlList': tanda}).encode('utf-8')
        pet = urllib.request.Request(API, data=cuerpo, method='POST',
                                     headers={'Content-Type': 'application/json; charset=utf-8', 'User-Agent': AGENTE})
        for intento in range(3):
            try:
                with urllib.request.urlopen(pet, timeout=60) as r:
                    # 200: recibidas; 202: recibidas, la clave está pendiente de comprobar.
                    print(f'IndexNow: {len(tanda)} direcciones enviadas (HTTP {r.status}).')
                break
            except urllib.error.HTTPError as ex:
                detalle = ex.read().decode('utf-8', 'replace')[:300]
                # 400 petición mal hecha; 403 la clave no vale; 422 direcciones de otro sitio; 429 demasiadas peticiones.
                if ex.code in (429, 500, 502, 503, 504) and intento < 2:
                    time.sleep(30 * (intento + 1))
                    continue
                sys.exit(f'IndexNow: HTTP {ex.code} {ex.reason}. {detalle}')
            except urllib.error.URLError as ex:
                if intento < 2:
                    time.sleep(30 * (intento + 1))
                    continue
                sys.exit(f'IndexNow: no se ha podido conectar con {API}: {ex.reason}')


def main():
    ap = argparse.ArgumentParser(description='Avisa a IndexNow de las páginas nuevas o cambiadas de la web.')
    ap.add_argument('carpeta', nargs='?', default=os.path.join(RAIZ, '_web'), help='la web generada por tools/web.py')
    ap.add_argument('--sin-enviar', action='store_true', help='solo dice cuántas direcciones mandaría')
    ap.add_argument('--todas', action='store_true', help='todas las direcciones del sitemap, no solo las nuevas')
    ap.add_argument('--anterior', help='sitemap con el que comparar (fichero o dirección); por defecto, el publicado')
    ap.add_argument('--preparar', metavar='FICHERO',
                    help='antes de desplegar: copia la clave a la carpeta y guarda en FICHERO el sitemap publicado')
    args = ap.parse_args()
    base, k = base_web(), clave()

    if args.preparar:
        shutil.copy(os.path.join(WEB, f'{k}.txt'), os.path.join(args.carpeta, f'{k}.txt'))
        try:
            datos = bajar(base + 'sitemap.xml')
            leer_sitemap(datos)
        except (OSError, urllib.error.URLError, ET.ParseError) as ex:
            sys.exit(f'IndexNow: clave copiada, pero no se ha podido guardar el sitemap publicado: {ex}')
        with open(args.preparar, 'wb') as f:
            f.write(datos)
        print(f'IndexNow: clave copiada a {args.carpeta} y sitemap publicado guardado en {args.preparar}.')
        return

    with open(os.path.join(args.carpeta, 'sitemap.xml'), 'rb') as f:
        nuevo = leer_sitemap(f.read())
    nuevo = {u: d for u, d in nuevo.items() if u.startswith(base)}
    viejo = None if args.todas else anterior(args.anterior or base + 'sitemap.xml')
    fijas = [base + r for r in FIJAS]
    if args.todas:
        cambiadas = [u for u in nuevo if u not in fijas]
    elif viejo is None:
        # Sin el sitemap anterior no se sabe qué ha cambiado: solo las fijas (para mandarlas todas, --todas).
        cambiadas = []
    else:
        cambiadas = [u for u, d in nuevo.items() if u not in fijas and viejo.get(u) != d]
    urls = fijas + cambiadas
    nuevas = sum(1 for u in cambiadas if viejo is not None and u not in viejo)
    resumen = (f'{len(urls)} direcciones: {len(fijas)} fijas y {len(cambiadas)} '
               + ('del sitemap' if args.todas else f'nuevas o cambiadas ({nuevas} nuevas)')
               + f', de {len(nuevo)} en el sitemap')
    if args.sin_enviar:
        print(f'IndexNow (sin enviar): mandaría {resumen}.')
        for u in cambiadas[:10]:
            print('  ' + u)
        if len(cambiadas) > 10:
            print(f'  … y {len(cambiadas) - 10} más')
        return
    print(f'IndexNow: {resumen}.')
    enviar(base, k, urls)


if __name__ == '__main__':
    main()

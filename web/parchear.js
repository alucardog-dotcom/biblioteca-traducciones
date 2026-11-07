'use strict';
// Parchear en el navegador: eliges tu ROM (o un zip con ella) o toda una carpeta de juegos; la página
// reconoce cada juego por su CRC32 en el catálogo (catalogo/parches.json) o, si no, por su nombre; baja el
// parche de la web de sus autores, lo aplica con motor.js y te da el juego traducido para guardarlo. Todo pasa
// en el navegador: los juegos no salen de tu equipo.

const M = window.Motor;
const $ = id => document.getElementById(id);
const esc = s => String(s ?? '').replace(/[&<>"']/g, c => ({'&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;'}[c]));
const webSegura = u => /^https?:\/\/\S+$/i.test(u || '') ? u : '';
const nombreSis = s => (window.SYS || {})[s] || s;
const num = n => n.toLocaleString('es-ES');
const PC = new Set(['dos', 'windows', 'scummvm', 'easyrpg', 'pc']);
const MODERNAS = new Set(['switch', 'ps3', 'ps4', 'ps5', 'wiiu', 'xbox360', 'xboxone']);
const A_MANO = new Set([...PC, ...MODERNAS]);

let indice = null, porCrc = new Map(), porToken = new Map();
let juego = null;   // {nombre, datos, salto, crc, crcSin}

function log(texto, tipo = '') {
  const r = $('registro');
  const hora = new Date().toLocaleTimeString('es', {hour: '2-digit', minute: '2-digit', second: '2-digit'});
  r.textContent += `[${hora}] ${texto}\n`;
  r.scrollTop = r.scrollHeight;
  if (tipo === 'bad') console.warn(texto);
}
function cuenta(evento) {
  try { if (window.goatcounter && window.goatcounter.count) window.goatcounter.count({path: 'parchear/' + evento, title: evento, event: true}); } catch (e) { }
}

// ---------------------------------------------------------------- nombres (como el programa)
const STOP = new Set(['the', 'a', 'an', 'el', 'la', 'los', 'las', 'de', 'del', 'of', 'and', 'y', 'en', 'to', 'traduccion', 'traducido', 'traducida', 'translation', 'translated', 'spanish', 'espanol', 'castellano', 'es', 'esp', 'spa', 'parche', 'patch', 'fix', 'final', 'rev', 'beta', 'completa', 'completo', 'ips', 'bps', 'ups', 'ppf', 'xdelta', 'aps', 'rom', 'iso', 'track', 'disc', 'usa', 'japan', 'europe', 'jap', 'jp', 'us', 'eu']);
const ROMAN = {ii: '2', iii: '3', iv: '4', vi: '6', vii: '7', viii: '8', ix: '9'};
function tokens(t) {
  if (!t) return [];
  const s = t.replace(/\[[^\]]*\]/g, ' ').replace(/\([^)]*\)/g, ' ').normalize('NFD').replace(/[̀-ͯ]/g, '')
    .toLowerCase().replace(/['’]/g, '').replace(/[^a-z0-9]+/g, ' ');
  const out = [];
  for (let w of s.split(' ')) {
    if (!w || /^v\d+$/.test(w) || (/^\d+[a-z]?$/.test(w) && w.length > 4) || STOP.has(w)) continue;
    w = ROMAN[w] || w;
    if (!out.includes(w)) out.push(w);
  }
  return out;
}
function simil(a, b) {
  if (!a.length || !b.length) return 0;
  const na = a.filter(x => /^\d+$/.test(x)).sort().join(), nb = b.filter(x => /^\d+$/.test(x)).sort().join();
  if (na !== nb) return 0;
  const inter = a.filter(x => b.includes(x)).length;
  if (!inter) return 0;
  const union = new Set([...a, ...b]).size, min = Math.min(a.length, b.length);
  return min >= 2 ? Math.max(inter / union, 0.9 * inter / min) : inter / union;
}

async function cargarIndice() {
  const r = await fetch('../catalogo/parches.json');
  if (!r.ok) throw new Error('No se ha podido cargar el catálogo (' + r.status + ').');
  indice = await r.json();
  porCrc = new Map();
  porToken = new Map();
  for (const t of indice) {
    for (const c of t.c) {
      if (!porCrc.has(c)) porCrc.set(c, []);
      porCrc.get(c).push(t);
    }
    t.titulos = [t.j, ...(t.r || '').split('|').map(x => x.trim().replace(/\.[a-z0-9]{1,4}$/i, ''))].map(tokens).filter(x => x.length);
    for (const w of new Set(t.titulos.flat())) {
      if (!porToken.has(w)) porToken.set(w, []);
      porToken.get(w).push(t);
    }
  }
  log(`Catálogo cargado: ${num(indice.length)} traducciones con CRC32.`);
}

// ---------------------------------------------------------------- un juego
async function leerFichero(f) {
  if (f.size > M.MAX_EN_MEMORIA) throw new Error('El fichero es demasiado grande para parchearlo en el navegador (más de 1,8 GB).');
  return new Uint8Array(await f.arrayBuffer());
}

// Dentro de un zip, el juego es el fichero más grande que no es un parche ni un léame.
async function sacarDelZip(datos, nombreZip) {
  const entradas = M.zipEntradas(datos).filter(e => !/\.(txt|nfo|diz|md|ips|bps|ups|ppf|aps|xdelta|html?)$/i.test(e.nombre));
  if (!entradas.length) throw new Error('El zip no contiene ningún juego.');
  const e = entradas.sort((a, b) => b.tam - a.tam)[0];
  log(`Zip ${nombreZip}: se usa ${e.nombre} (${(e.tam / 1048576).toFixed(1)} MB).`);
  return {nombre: e.nombre.split('/').pop(), datos: await M.zipExtraer(datos, e)};
}

// Lee el fichero, lo saca del zip si hace falta y calcula su CRC32 (con y sin cabecera). Deja `juego` listo.
async function prepararJuego(f) {
  $('juego').innerHTML = '';
  $('trads').innerHTML = '';
  $('resultado').innerHTML = '';
  juego = null;
  log(`Leyendo ${f.name} (${(f.size / 1048576).toFixed(1)} MB)…`);
  let datos = await leerFichero(f), nombre = f.name;
  if (/\.(7z|rar|chd)$/i.test(nombre)) throw new Error('Los 7z, rar y CHD no se pueden abrir en el navegador: usa el programa o descomprímelo antes.');
  if (M.esZip(datos)) ({nombre, datos} = await sacarDelZip(datos, nombre));
  const t0 = performance.now();
  const crc = M.hex(M.crc32(datos));
  const salto = M.cabecera(datos);
  const crcSin = salto ? M.hex(M.crc32(datos, salto)) : null;
  log(`CRC32 ${crc}${crcSin ? ` (sin la cabecera de ${salto} bytes: ${crcSin})` : ''} calculado en ${Math.round(performance.now() - t0)} ms.`);
  juego = {nombre, datos, crc, crcSin, salto};
  return juego;
}

async function elegirJuego(f) {
  try {
    await prepararJuego(f);
    identificar();
  } catch (e) {
    log('Error: ' + e.message, 'bad');
    $('juego').innerHTML = `<p class="aviso">${esc(e.message)}</p>`;
  }
}

// Con qué salto se aplica una traducción sobre el juego leído: el de la cabecera si el CRC casó sin ella.
const saltoPara = t => (t && t.c.includes(juego.crc)) ? 0 : juego.salto;

function identificar() {
  const vistos = new Set(), hallazgos = [];
  for (const [c, salto] of [[juego.crc, 0], [juego.crcSin, juego.salto]]) {
    if (!c) continue;
    for (const t of porCrc.get(c) || []) {
      if (vistos.has(t.k)) continue;
      vistos.add(t.k);
      hallazgos.push({t, salto});
    }
  }
  const base = juego.nombre.replace(/\.[^.]+$/, '');
  if (!hallazgos.length) {
    $('juego').innerHTML = `<h2>${esc(juego.nombre)}</h2>
      <p>No hay ninguna traducción en el catálogo para esta versión exacta del juego (CRC32 ${juego.crc}). Puede que exista para otra revisión:
      <a href="../catalogo/?q=${encodeURIComponent(base)}">busca el juego por su nombre en el catálogo</a>. Si ya tienes un parche, puedes aplicarlo igualmente:</p>
      ${formularioParche(null)}`;
    enlazarParcheManual(null);
    cuenta('no-reconocido');
    return;
  }
  cabeceraJuego(hallazgos[0].t, hallazgos[0].salto);
  $('trads').innerHTML = `<h3>${hallazgos.length === 1 ? 'Traducción encontrada' : hallazgos.length + ' traducciones encontradas'}</h3>` + hallazgos.map(({t}, i) => `
    <article class="trad" data-i="${i}">
      <div class="trad-t"><b>${esc(t.g || 'Grupo desconocido')}</b>${t.v ? ' · versión ' + esc(t.v) : ''}${t.e ? ` · <span class="chip ${/complet/i.test(t.e) ? 'ok' : 'aviso'}">${esc(t.e)}</span>` : ''}${t.ia ? ' <span class="chip ia">IA</span>' : ''}${t.f ? ' · ' + esc(t.f) : ''}</div>
      ${t.n ? `<p class="muted">${esc(t.n)}</p>` : ''}
      <div class="trad-a">
        ${t.d.length ? `<button class="btn primario" data-traducir="${i}">Traducir</button>` : ''}
        ${webSegura(t.u) ? `<a class="btn" href="${esc(t.u)}" target="_blank" rel="noopener">Página de la traducción</a>` : ''}
        ${t.p ? `<a class="btn" href="../${esc(t.p)}">Ficha en la web</a>` : ''}
      </div>
      ${t.d.length ? '' : `<p class="muted">Esta traducción no tiene descarga directa: bájala de su página y aplícala aquí.</p>${formularioParche(i)}`}
    </article>`).join('');
  $('trads').querySelectorAll('[data-traducir]').forEach(b => b.onclick = () => traducir(hallazgos[+b.dataset.traducir]));
  hallazgos.forEach((h, i) => enlazarParcheManual(i, h));
  cuenta('reconocido');
}

function cabeceraJuego(t, salto, nota = '') {
  $('juego').innerHTML = `<h2>${esc(t.j)} <span class="sis">${esc(nombreSis(t.s))}</span></h2>
    <p class="muted">Tu fichero: ${esc(juego.nombre)} · CRC32 ${juego.crc}${salto ? ` · con cabecera de ${salto} bytes, que se quita al parchear` : ''}${nota}</p>`;
}

const formularioParche = i => `<label class="manual">Tengo el parche: <input type="file" data-parche="${i === null ? '' : i}" accept=".ips,.bps,.ups,.ppf,.aps,.xdelta,.zip"></label>`;
function enlazarParcheManual(i, h) {
  const inp = document.querySelector(`input[data-parche="${i === null ? '' : i}"]`);
  if (!inp) return;
  inp.onchange = async () => {
    const f = inp.files[0];
    if (!f) return;
    try {
      const datos = await leerFichero(f);
      await aplicarParche(datos, f.name, h ? h.t : null, h ? h.salto : juego.salto);
    } catch (e) { log('Error: ' + e.message, 'bad'); mostrarError(e.message); }
  };
}

// ---------------------------------------------------------------- descargar
async function bajar(url) {
  log('Bajando ' + url);
  const r = await fetch(url);
  if (!r.ok) throw new Error(`${r.status} al bajar ${url}`);
  const datos = new Uint8Array(await r.arrayBuffer());
  log(`Bajado: ${(datos.length / 1024).toFixed(0)} KB.`);
  return {datos, nombre: decodeURIComponent(url.split('/').pop().split('?')[0])};
}

async function descargar(d) {
  if (d.t === 'github') {
    // El parche está en los ficheros de la última versión publicada (o de la etiqueta indicada), como en descarga.rs.
    const api = d.tag ? `https://api.github.com/repos/${d.r}/releases/tags/${d.tag}` : `https://api.github.com/repos/${d.r}/releases/latest`;
    const r = await fetch(api, {headers: {Accept: 'application/vnd.github+json'}});
    if (!r.ok) throw new Error(`GitHub no tiene una versión publicada con ficheros para ${d.r}.`);
    const rel = await r.json();
    const ORDEN = ['bps', 'xdelta', 'ups', 'ips', 'ppf', 'zip', '7z', 'rar', 'aps', 'vcdiff'];
    const assets = (rel.assets || []).map(a => {
      const n = (a.name || '').toLowerCase(), ext = n.slice(n.lastIndexOf('.') + 1), pos = ORDEN.indexOf(ext);
      if (pos < 0 || n.includes('source') || n.includes('src') || n.includes('herramienta') || /tools?\b/.test(n)) return null;
      return {pos, u: a.browser_download_url};
    }).filter(Boolean).sort((a, b) => a.pos - b.pos);
    if (!assets.length) throw new Error(`La versión publicada de ${d.r} no trae ningún parche.`);
    return bajar(assets[0].u);
  }
  let ultimo = null;
  for (const u of d.u || []) {
    try { return await bajar(u); } catch (e) { ultimo = e; log('No se pudo: ' + e.message, 'bad'); }
  }
  throw ultimo || new Error('Sin direcciones de descarga.');
}

async function traducir(h) {
  const {t, salto} = h;
  $('resultado').innerHTML = '<p class="muted">Bajando el parche…</p>';
  $('resultado').scrollIntoView({behavior: 'smooth', block: 'nearest'});
  try {
    let ultimo = null;
    for (const d of t.d) {
      try {
        const {datos, nombre} = await descargar(d);
        await aplicarParche(datos, nombre, t, salto, d);
        return;
      } catch (e) { ultimo = e; log('Error: ' + e.message, 'bad'); }
    }
    throw ultimo || new Error('No hay descarga.');
  } catch (e) {
    mostrarError(`${esc(e.message)} Puedes bajar el parche a mano${webSegura(t.u) ? ` de <a href="${esc(t.u)}" target="_blank" rel="noopener">su página</a>` : ''} y aplicarlo con «Tengo el parche».` + formularioParche('m'));
    const inp = document.querySelector('input[data-parche="m"]');
    if (inp) inp.onchange = async () => { try { await aplicarParche(await leerFichero(inp.files[0]), inp.files[0].name, t, salto); } catch (e2) { mostrarError(esc(e2.message)); } };
  }
}

function mostrarError(html) {
  $('resultado').innerHTML = `<p class="aviso">${html}</p>`;
}

// ---------------------------------------------------------------- aplicar
async function aplicarParche(datos, nombre, t, salto, d) {
  let parche = datos, nombreParche = nombre;
  if (M.esZip(datos)) {
    const entradas = M.zipEntradas(datos);
    let parches = M.parchesDe(entradas);
    // Un zip con varios parches: el que el catálogo dice para esta ficha, si lo sabe.
    if (parches.length > 1 && d && d.f && d.f.length) {
      const pref = parches.filter(e => d.f.some(f => e.nombre.endsWith(f)));
      if (pref.length) parches = pref;
    }
    if (!parches.length) throw new Error(`El zip ${nombre} no trae ningún parche reconocible (${entradas.map(e => e.nombre).join(', ')}).`);
    if (parches.length > 1) log(`El zip trae ${parches.length} parches; se usa ${parches[0].nombre}. Otros: ${parches.slice(1).map(e => e.nombre).join(', ')}.`);
    parche = await M.zipExtraer(datos, parches[0]);
    nombreParche = parches[0].nombre;
  }
  const formato = M.detectar(parche);
  log(`Parche ${nombreParche}: formato ${formato || 'desconocido'}.`);
  if (formato === 'xdelta') throw new Error('Este parche es xdelta y el navegador no puede aplicarlo todavía: usa el programa.');
  const origen = salto ? juego.datos.subarray(salto) : juego.datos;
  const t0 = performance.now();
  let {datos: out, mensaje} = M.aplicar(parche, origen, false);
  // La cabecera iNES vuelve a su sitio (los emuladores la necesitan); la de copiador de SNES sobra.
  if (salto === 16) {
    const con = new Uint8Array(16 + out.length);
    con.set(juego.datos.subarray(0, 16));
    con.set(out, 16);
    out = con;
  }
  log(`${mensaje} (${Math.round(performance.now() - t0)} ms)`);
  const ext = juego.nombre.includes('.') ? juego.nombre.slice(juego.nombre.lastIndexOf('.')) : '';
  const base = ext ? juego.nombre.slice(0, -ext.length) : juego.nombre;
  const salida = `${base} [T-Es]${ext}`;
  const blob = new Blob([out], {type: 'application/octet-stream'});
  const url = URL.createObjectURL(blob);
  $('resultado').innerHTML = `<div class="hecho"><h3>Listo: ${esc(salida)}</h3>
    <p>${esc(mensaje)} ${(out.length / 1048576).toFixed(1)} MB.${t ? ` Traducción de ${esc(t.g || 'grupo desconocido')}${t.v ? ' ' + esc(t.v) : ''}.` : ''}</p>
    <div class="trad-a"><a class="btn primario grande" href="${url}" download="${esc(salida)}">Descargar el juego traducido</a>${window.showSaveFilePicker ? '<button class="btn grande" id="guardar-en">Guardar en…</button>' : ''}</div>
    <p class="muted">El original no se ha tocado. Es una copia nueva con el parche aplicado.</p></div>`;
  if ($('guardar-en')) $('guardar-en').onclick = async () => {
    try {
      const h = await window.showSaveFilePicker({suggestedName: salida});
      const w = await h.createWritable();
      await w.write(blob);
      await w.close();
      log('Guardado en ' + h.name);
    } catch (e) { if (e.name !== 'AbortError') log('No se pudo guardar: ' + e.message, 'bad'); }
  };
  cuenta('traducido');
}

// ---------------------------------------------------------------- mis juegos: una carpeta entera
// Lo mismo que hace el programa (escaneo.rs): el sistema por la extensión o por el nombre de la carpeta,
// CRC32 de las ROMs (con y sin cabecera), de los ficheros de los zips (de su índice, sin descomprimir) y de
// las pistas de los .cue y .gdi; los demás formatos, por el nombre.
const POR_EXTENSION = {nes: 'nes', unf: 'nes', fds: 'fds', sfc: 'snes', smc: 'snes', swc: 'snes', fig: 'snes', bs: 'satellaview', gb: 'gb', gbc: 'gbc', gba: 'gba',
  n64: 'n64', z64: 'n64', v64: 'n64', nds: 'nds', dsi: 'nds', '3ds': '3ds', cia: '3ds', vb: 'virtualboy', sms: 'mastersystem', gg: 'gamegear', sg: 'sg1000',
  md: 'megadrive', gen: 'megadrive', smd: 'megadrive', '32x': 'sega32x', pce: 'pcengine', sgx: 'supergrafx', ngp: 'ngp', ngc: 'ngpc', ws: 'wswan', wsc: 'wswanc',
  col: 'colecovision', lnx: 'lynx', mx1: 'msx1', mx2: 'msx2', dsk: '', rom: '', bin: '', nsp: 'switch', xci: 'switch', nsz: 'switch', xcz: 'switch',
  wua: 'wiiu', wux: 'wiiu', wud: 'wiiu', xex: 'xbox360'};
const SIN_CRC = new Set(['nsp', 'xci', 'nsz', 'xcz', 'wua', 'wux', 'wud', 'xex', 'pkg']);
const DISCOS = new Set(['cue', 'gdi', 'chd', 'iso', 'cso', 'pbp', 'ccd', 'cdi', 'rvz', 'gcm', 'wbfs', 'ciso', 'wia', 'gcz', 'pkg']);
const ARCHIVOS = new Set(['zip', '7z', 'rar']);
const POR_CARPETA = {nes: 'nes', famicom: 'nes', nintendo: 'nes', fds: 'fds', 'famicom disk system': 'fds', snes: 'snes', sfc: 'snes', 'super nintendo': 'snes',
  'super famicom': 'snes', satellaview: 'satellaview', gb: 'gb', 'game boy': 'gb', gameboy: 'gb', gbc: 'gbc', 'game boy color': 'gbc', gba: 'gba',
  'game boy advance': 'gba', n64: 'n64', 'nintendo 64': 'n64', nds: 'nds', 'nintendo ds': 'nds', '3ds': '3ds', 'nintendo 3ds': '3ds', virtualboy: 'virtualboy',
  'virtual boy': 'virtualboy', gamecube: 'gamecube', gc: 'gamecube', wii: 'wii', mastersystem: 'mastersystem', 'master system': 'mastersystem', sms: 'mastersystem',
  gamegear: 'gamegear', 'game gear': 'gamegear', sg1000: 'sg1000', 'sg-1000': 'sg1000', megadrive: 'megadrive', 'mega drive': 'megadrive', genesis: 'megadrive',
  md: 'megadrive', sega32x: 'sega32x', '32x': 'sega32x', megacd: 'megacd', 'mega cd': 'megacd', 'mega-cd': 'megacd', segacd: 'megacd', 'sega cd': 'megacd',
  saturn: 'saturn', dreamcast: 'dreamcast', dc: 'dreamcast', psx: 'psx', ps1: 'psx', psone: 'psx', playstation: 'psx', ps2: 'ps2', 'playstation 2': 'ps2',
  psp: 'psp', psvita: 'psvita', pcengine: 'pcengine', 'pc engine': 'pcengine', turbografx: 'pcengine', 'turbografx-16': 'pcengine', pcenginecd: 'pcenginecd',
  'pc engine cd': 'pcenginecd', 'turbografx-cd': 'pcenginecd', supergrafx: 'supergrafx', pcfx: 'pcfx', 'pc-fx': 'pcfx', neogeocd: 'neogeocd', 'neo geo cd': 'neogeocd',
  ngp: 'ngp', ngpc: 'ngpc', 'neo geo pocket': 'ngp', 'neo geo pocket color': 'ngpc', wswan: 'wswan', wonderswan: 'wswan', wswanc: 'wswanc', 'wonderswan color': 'wswanc',
  msx: 'msx1', msx1: 'msx1', msx2: 'msx2', zxspectrum: 'zxspectrum', 'zx spectrum': 'zxspectrum', spectrum: 'zxspectrum', amstradcpc: 'amstradcpc',
  'amstrad cpc': 'amstradcpc', colecovision: 'colecovision', lynx: 'lynx', '3do': '3do', xbox: 'xbox', pc98: 'pc98', 'pc-98': 'pc98', switch: 'switch',
  'nintendo switch': 'switch', ps3: 'ps3', 'playstation 3': 'ps3', ps4: 'ps4', 'playstation 4': 'ps4', ps5: 'ps5', 'playstation 5': 'ps5', wiiu: 'wiiu',
  'wii u': 'wiiu', xbox360: 'xbox360', 'xbox 360': 'xbox360'};
const NO_JUEGO = new Set(['txt', 'nfo', 'diz', 'doc', 'docx', 'pdf', 'htm', 'html', 'url', 'lnk', 'xml', 'cfg', 'ini', 'json', 'png', 'jpg', 'jpeg', 'gif', 'bmp', 'webp',
  'mp4', 'mkv', 'avi', 'mp3', 'ogg', 'wav', 'm3u', 'srm', 'sav', 'state', 'ips', 'bps', 'ups', 'ppf', 'xdelta', 'vcdiff', 'aps', 'log', 'md5', 'sha1', 'sfv', 'exe', 'dll',
  'bat', 'ps1', 'torrent']);
const CARPETAS_FUERA = new Set(['images', 'videos', 'media', 'manuals', 'downloaded_images', 'downloaded_videos']);
const MAX_CRC_ESCANEO = 1 << 30;
const ext = n => n.includes('.') ? n.slice(n.lastIndexOf('.') + 1).toLowerCase() : '';

// El sistema según las carpetas de encima (la más cercana manda; la elegida también cuenta).
function porCarpeta(ruta) {
  const partes = ruta.split('/').slice(0, -1);
  for (let i = partes.length - 1; i >= 0; i--) {
    const s = POR_CARPETA[partes[i].toLowerCase()];
    if (s) return s;
  }
  return '';
}

// CRC32 de un fichero leído por trozos (con y sin cabecera, si la lleva), sin cargarlo entero.
async function crcsFichero(f, sis) {
  const lector = f.stream().getReader();
  let entero = M.CRC_INICIO, sin = M.CRC_INICIO, visto = 0, salto = -1;
  for (;;) {
    const {done, value} = await lector.read();
    if (done) break;
    if (salto < 0) {
      salto = value.length >= 4 && value[0] === 78 && value[1] === 69 && value[2] === 83 && value[3] === 26 ? 16
        : (sis === 'snes' || sis === 'satellaview') && f.size % 1024 === 512 ? 512 : 0;
    }
    entero = M.crcAvanza(entero, value);
    if (salto) {
      const desde = Math.max(0, salto - visto);
      if (desde < value.length) sin = M.crcAvanza(sin, value, desde);
    }
    visto += value.length;
  }
  const out = [M.hex(M.crcFin(entero))];
  if (salto > 0) out.push(M.hex(M.crcFin(sin)));
  return out;
}

// El índice de un zip leyendo solo el final del fichero: nombre, CRC32 y tamaño de cada entrada.
async function indiceZip(f) {
  const cola = new Uint8Array(await f.slice(Math.max(0, f.size - 65557)).arrayBuffer());
  let eocd = -1;
  for (let i = cola.length - 22; i >= 0; i--) if (cola[i] === 0x50 && cola[i + 1] === 0x4B && cola[i + 2] === 5 && cola[i + 3] === 6) { eocd = i; break; }
  if (eocd < 0) return [];
  const u32 = (b, p) => (b[p] | b[p + 1] << 8 | b[p + 2] << 16 | b[p + 3] << 24) >>> 0;
  const n = cola[eocd + 10] | cola[eocd + 11] << 8, tam = u32(cola, eocd + 12), off = u32(cola, eocd + 16);
  const cd = new Uint8Array(await f.slice(off, off + tam).arrayBuffer());
  const out = [], dec = new TextDecoder('utf-8');
  let p = 0;
  for (let k = 0; k < n && p + 46 <= cd.length; k++) {
    if (u32(cd, p) !== 0x02014B50) break;
    const metodo = cd[p + 10] | cd[p + 11] << 8, crc = M.hex(u32(cd, p + 16)), comp = u32(cd, p + 20), tamE = u32(cd, p + 24);
    const nlen = cd[p + 28] | cd[p + 29] << 8, elen = cd[p + 30] | cd[p + 31] << 8, clen = cd[p + 32] | cd[p + 33] << 8;
    const nombre = dec.decode(cd.subarray(p + 46, p + 46 + nlen));
    if (!nombre.endsWith('/')) out.push({nombre, metodo, crc, comp, tam: tamE, off: u32(cd, p + 42), cifrado: !!(cd[p + 8] & 1)});
    p += 46 + nlen + elen + clen;
  }
  return out;
}

// Las pistas que nombra un .cue o un .gdi (ya leído como texto).
function pistasDe(texto, gdi) {
  const out = [];
  for (let l of texto.split(/\r?\n/)) {
    l = l.trim();
    if (gdi) {
      const a = l.indexOf('"'), b = l.lastIndexOf('"');
      if (a >= 0 && b > a) out.push(l.slice(a + 1, b));
      else { const p = l.split(/\s+/); if (p.length > 4) out.push(p[4]); }
    } else if (l.toUpperCase().startsWith('FILE')) {
      const a = l.indexOf('"'), b = l.lastIndexOf('"');
      if (a >= 0 && b > a) out.push(l.slice(a + 1, b));
    }
  }
  return out;
}

const mismoSistema = (sis, t) => sis === 'pc' ? PC.has(t.s) : sis ? t.s === sis : !PC.has(t.s);

// Las traducciones de un juego: por CRC32 (exacto) o por el nombre (posible), como el programa.
function emparejar(g) {
  const vistos = new Set(), exactos = [], posibles = [];
  for (const c of g.c) for (const t of porCrc.get(c) || []) if (!vistos.has(t.k)) { vistos.add(t.k); exactos.push(t); }
  const gt = tokens(g.n), cands = new Set();
  for (const w of gt) for (const t of porToken.get(w) || []) if (mismoSistema(g.s, t)) cands.add(t);
  for (const t of cands) {
    if (vistos.has(t.k)) continue;
    if (Math.max(...t.titulos.map(x => simil(gt, x))) < 0.8) continue;
    vistos.add(t.k);
    posibles.push(t);
  }
  return {exactos, posibles};
}

// items: [{file, ruta}] con la ruta relativa a la carpeta elegida.
async function escanear(items) {
  const mios = $('mios');
  mios.innerHTML = '<h2>Mis juegos</h2><p class="progreso" id="progreso">Mirando la carpeta…</p>';
  mios.scrollIntoView({behavior: 'smooth', block: 'start'});
  const t0 = performance.now();
  const porRuta = new Map(items.map(it => [it.ruta.toLowerCase(), it]));
  // Las pistas de un .cue o .gdi no son juegos por sí solas.
  const dePista = new Set();
  const hojas = items.filter(it => ['cue', 'gdi'].includes(ext(it.file.name)) && it.file.size < 1 << 20);
  for (const it of hojas) {
    const dir = it.ruta.includes('/') ? it.ruta.slice(0, it.ruta.lastIndexOf('/') + 1) : '';
    it.pistas = pistasDe(await it.file.text(), ext(it.file.name) === 'gdi').map(n => porRuta.get((dir + n).toLowerCase())).filter(Boolean);
    for (const p of it.pistas) dePista.add(p.ruta.toLowerCase());
  }
  const juegos = [], traducidos = [], saltados = {fuera: 0, pequenos: 0};
  let leidos = 0, bytes = 0;
  const candidatos = items.filter(it => {
    const partes = it.ruta.toLowerCase().split('/');
    if (partes.slice(0, -1).some(p => CARPETAS_FUERA.has(p))) return false;
    const e = ext(it.file.name);
    if (NO_JUEGO.has(e) || dePista.has(it.ruta.toLowerCase())) return false;
    const vale = DISCOS.has(e) || ARCHIVOS.has(e) || e in POR_EXTENSION;
    if (!vale) { saltados.fuera++; return false; }
    if (it.file.size < 1024 && e !== 'cue' && e !== 'gdi') { saltados.pequenos++; return false; }
    return true;
  });
  for (const it of candidatos) {
    const f = it.file, e = ext(f.name), nombre = f.name.replace(/\.[^.]+$/, '');
    const carpeta = porCarpeta(it.ruta);
    let sis = POR_EXTENSION[e] || carpeta;
    if (nombre.includes('[T-Es]')) { traducidos.push({it, sis}); continue; }
    let c = [], k = 'file', zipEntradas = null;
    try {
      if (e === 'zip') {
        k = 'archive';
        zipEntradas = (await indiceZip(f)).filter(z => !NO_JUEGO.has(ext(z.nombre)));
        c = zipEntradas.map(z => z.crc);
        bytes += Math.min(f.size, 1 << 16);
      } else if (e === '7z' || e === 'rar') {
        k = 'archive';
      } else if (e === 'cue' || e === 'gdi') {
        k = 'sheet';
        for (const p of it.pistas || []) if (p.file.size <= MAX_CRC_ESCANEO) { c.push(...await crcsFichero(p.file, '')); bytes += p.file.size; }
      } else if (e === 'chd' || DISCOS.has(e) || SIN_CRC.has(e) || f.size > MAX_CRC_ESCANEO) {
        k = e === 'chd' ? 'chd' : 'file';
        if (DISCOS.has(e) && e !== 'chd' && f.size <= MAX_CRC_ESCANEO) { c = await crcsFichero(f, ''); bytes += f.size; }
      } else {
        c = await crcsFichero(f, sis);
        bytes += f.size;
      }
    } catch (err) { log(`${it.ruta}: ${err.message}`, 'bad'); }
    if (ARCHIVOS.has(e) && !sis) sis = carpeta;
    const g = {it, s: sis, n: nombre, k, c, zipEntradas};
    // Un zip con una ROM con cabecera: el índice da el CRC con ella; hace falta sacarla para el CRC sin ella.
    let par = emparejar(g);
    if (!par.exactos.length && zipEntradas && zipEntradas.length === 1 && zipEntradas[0].tam <= 16 << 20 && !zipEntradas[0].cifrado
        && (['nes', 'snes', 'satellaview'].includes(sis) || ['nes', 'sfc', 'smc', 'swc', 'fig', 'bs'].includes(ext(zipEntradas[0].nombre)))) {
      try {
        const z = zipEntradas[0];
        const todo = new Uint8Array(await f.arrayBuffer());
        const datos = await M.zipExtraer(todo, M.zipEntradas(todo).find(x => x.nombre === z.nombre));
        const salto = M.cabecera(datos);
        if (salto) { g.c = [...g.c, M.hex(M.crc32(datos, salto))]; par = emparejar(g); }
      } catch (err) { log(`${it.ruta}: ${err.message}`, 'bad'); }
    }
    g.exactos = par.exactos;
    g.posibles = par.posibles;
    juegos.push(g);
    leidos++;
    if (leidos % 10 === 0 || leidos === candidatos.length) {
      $('progreso').textContent = `Leídos ${num(leidos)} de ${num(candidatos.length)} ficheros (${(bytes / 1048576).toFixed(0)} MB)…`;
      await new Promise(r => setTimeout(r, 0));
    }
  }
  const ms = performance.now() - t0;
  const seg = ms < 1000 ? `${Math.round(ms)} ms` : `${(ms / 1000).toFixed(1)} s`;
  log(`Carpeta leída: ${num(items.length)} ficheros, ${num(candidatos.length)} juegos posibles, ${(bytes / 1048576).toFixed(0)} MB en ${seg}.`);
  cuenta('escaneo');
  await pintarMios(juegos, traducidos, items.length, saltados, seg);
}

// Portadas como en el catálogo (catalogo.js): las direcciones en el orden en que se prueban.
let porRutaJuego = null;
async function cargarPortadas() {
  if (porRutaJuego) return;
  porRutaJuego = new Map();
  try {
    const d = await (await fetch('../catalogo/juegos.json')).json();
    for (const j of d) porRutaJuego.set(j[2], j);
  } catch (e) { log('Sin portadas: ' + e.message, 'bad'); }
}
function urlsPortada(spec) {
  if (!spec) return [];
  const tipo = spec[0], resto = spec.slice(2).split('|');
  if (tipo === 'm') return ['../' + resto[0]];
  if (tipo === 'u') return [resto[0]];
  if (tipo === 's') {
    const st = 'https://cdn.cloudflare.steamstatic.com/steam/apps/' + encodeURIComponent(resto[0]) + '/';
    return [st + 'library_600x900.jpg', resto[1], st + 'header.jpg'].filter(Boolean);
  }
  const [sis, t, nombre, pu] = resto;
  const alt = t === 'Named_Boxarts' ? 'Named_Titles' : 'Named_Snaps';
  const thumb = tt => 'https://thumbnails.libretro.com/' + encodeURIComponent(sis) + '/' + tt + '/' + encodeURIComponent(nombre.replace(/[&*/:`<>?\\|"]/g, '_')) + '.png';
  return [thumb(t), pu, thumb(alt)].filter(Boolean);
}

let tarjetasMios = [];
function tarjetaMio(g, t, tipo) {
  const n = tarjetasMios.push({g, t}) - 1;
  const j = porRutaJuego && porRutaJuego.get(t.p);
  const urls = urlsPortada(j && j[6]);
  const img = urls.length ? `<img loading="lazy" src="${esc(urls[0])}" data-alt="${esc(urls.slice(1).join(' '))}" alt="" referrerpolicy="no-referrer" onerror="sig(this)" onload="this.classList.add('ok')">` : '';
  const directa = t.d.length > 0 && !A_MANO.has(t.s) && g.k !== 'sheet' && g.k !== 'chd' && g.it.file.size <= M.MAX_EN_MEMORIA;
  const cuantas = (g.exactos.length + g.posibles.length);
  const meta = [esc(nombreSis(t.s)), tipo === 'exacto' ? '<span class="ok">Tu versión vale</span>' : '<span class="warn">Sin comprobar</span>', cuantas > 1 ? `${cuantas} traducciones` : esc(t.g || '')].filter(Boolean).join(' · ');
  return `<li class="tarjeta con-acc" data-n="${n}"><a href="../${esc(t.p)}" title="${esc(g.it.ruta)}"><span class="port${img ? '' : ' sin'}">${img}<span class="ph">${esc(t.j)}</span></span>
    <span class="cuerpo"><span class="t">${esc(t.j)}</span><span class="m">${meta}</span><span class="m">${esc(g.it.file.name)}</span></span></a>
    <div class="acc">${directa ? `<button class="btn primario" data-mio="${n}">Traducir</button>` : ''}<a class="btn" href="../${esc(t.p)}">Ficha</a></div></li>`;
}

async function pintarMios(juegos, traducidos, total, saltados, seg) {
  await cargarPortadas();
  tarjetasMios = [];
  const con = juegos.filter(g => g.exactos.length || g.posibles.length);
  const exactos = con.filter(g => g.exactos.length), posibles = con.filter(g => !g.exactos.length);
  const orden = (a, b) => a.n.localeCompare(b.n, 'es');
  const tarjetas = gs => `<ul class="tarjetas">${gs.sort(orden).map(g => tarjetaMio(g, (g.exactos[0] || g.posibles[0]), g.exactos.length ? 'exacto' : 'posible')).join('')}</ul>`;
  const sin = juegos.length - con.length;
  const resumen = `${num(con.length)} de tus ${num(juegos.length)} juegos tienen traducción al español`
    + (exactos.length && posibles.length ? ` (${num(exactos.length)} de tu versión exacta y ${num(posibles.length)} por el nombre)` : '')
    + `. ${num(sin)} sin traducción conocida${traducidos.length ? `, ${num(traducidos.length)} ya traducidos` : ''}. ${num(total)} ficheros leídos en ${seg}.`;
  $('mios').innerHTML = `<div class="mios-h"><h2>Mis juegos</h2><span class="muted">${resumen}</span></div>
    ${exactos.length ? `<h3>Tu versión vale: Traducir y listo</h3>${tarjetas(exactos)}` : ''}
    ${posibles.length ? `<h3>Por el nombre: puede que pidan otra versión</h3><p class="muted">El CRC32 no coincide con ninguna ficha; los parches BPS y UPS lo comprueban al aplicarse, los IPS no.</p>${tarjetas(posibles)}` : ''}
    ${!con.length ? '<p class="aviso">Ninguno de los juegos de esa carpeta tiene traducción en el catálogo. Mira que estén sueltos o en zip (los 7z, rar y CHD no se pueden leer aquí) y que la carpeta diga el sistema (snes, psx, megadrive…) si las extensiones no lo dicen.</p>' : ''}
    ${traducidos.length ? `<h3>Ya traducidos</h3><p class="muted">${traducidos.map(x => esc(x.it.file.name)).join(' · ')}</p>` : ''}`;
  $('mios').querySelectorAll('[data-mio]').forEach(b => b.onclick = async () => {
    const {g, t} = tarjetasMios[+b.dataset.mio];
    try {
      await prepararJuego(g.it.file);
      cabeceraJuego(t, saltoPara(t), g.exactos.length ? '' : ' · emparejado por el nombre, sin comprobar la versión');
      $('juego').scrollIntoView({behavior: 'smooth', block: 'start'});
      await traducir({t, salto: saltoPara(t)});
    } catch (e) { log('Error: ' + e.message, 'bad'); mostrarError(esc(e.message)); }
  });
}

async function elegirCarpeta(lista) {
  const items = [...lista].map(f => ({file: f, ruta: f.webkitRelativePath || f.name}));
  if (!items.length) return;
  try { await escanear(items); } catch (e) { log('Error: ' + e.message, 'bad'); $('mios').innerHTML = `<p class="aviso">${esc(e.message)}</p>`; }
}

// Con la API de acceso a carpetas (Chrome y Edge) el navegador pide permiso para «ver» la carpeta, no para
// «subirla», y se recorre entera; en los demás, el input con webkitdirectory.
async function recorrer(dir, ruta, items) {
  for await (const [nombre, h] of dir.entries()) {
    if (h.kind === 'file') items.push({file: await h.getFile(), ruta: ruta + nombre});
    else if (!CARPETAS_FUERA.has(nombre.toLowerCase())) await recorrer(h, ruta + nombre + '/', items);
  }
}
async function pedirCarpeta() {
  if (!window.showDirectoryPicker) return $('carpeta').click();
  let dir;
  try { dir = await window.showDirectoryPicker({mode: 'read'}); } catch (e) { if (e.name !== 'AbortError') log('No se pudo abrir la carpeta: ' + e.message, 'bad'); return; }
  $('mios').innerHTML = '<h2>Mis juegos</h2><p class="progreso">Mirando la carpeta…</p>';
  const items = [];
  try { await recorrer(dir, dir.name + '/', items); await escanear(items); }
  catch (e) { log('Error: ' + e.message, 'bad'); $('mios').innerHTML = `<p class="aviso">${esc(e.message)}</p>`; }
}

// ---------------------------------------------------------------- arranque
(async () => {
  if (!M || !window.DecompressionStream) {
    $('juego').innerHTML = '<p class="aviso">Tu navegador es demasiado antiguo para esta página (hace falta DecompressionStream). Prueba con Chrome, Edge o Firefox actuales.</p>';
    return;
  }
  const zona = $('zona');
  $('rom').onchange = () => { if ($('rom').files[0]) elegirJuego($('rom').files[0]); };
  for (const ev of ['dragenter', 'dragover']) zona.addEventListener(ev, e => { e.preventDefault(); zona.classList.add('sobre'); });
  for (const ev of ['dragleave', 'drop']) zona.addEventListener(ev, e => { e.preventDefault(); zona.classList.remove('sobre'); });
  zona.addEventListener('drop', e => { const f = e.dataTransfer.files[0]; if (f) elegirJuego(f); });
  if ($('carpeta')) $('carpeta').onchange = () => elegirCarpeta($('carpeta').files);
  if ($('elegir-carpeta')) $('elegir-carpeta').onclick = pedirCarpeta;
  try { await cargarIndice(); } catch (e) { log('Error: ' + e.message, 'bad'); $('juego').innerHTML = `<p class="aviso">${esc(e.message)}</p>`; }
})();

'use strict';
// Parchear en el navegador (prueba): eliges tu ROM (o un zip con ella), la página la reconoce por su CRC32 en
// el catálogo (catalogo/parches.json), baja el parche de la web de sus autores, lo aplica con motor.js y te
// da el juego traducido para guardarlo. Todo pasa en el navegador: el juego no sale de tu equipo.

const M = window.Motor;
const $ = id => document.getElementById(id);
const esc = s => String(s ?? '').replace(/[&<>"']/g, c => ({'&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;'}[c]));
const webSegura = u => /^https?:\/\/\S+$/i.test(u || '') ? u : '';
const nombreSis = s => (window.SYS || {})[s] || s;
const ORDEN_EXT = ['bps', 'xdelta', 'ups', 'ips', 'ppf', 'aps', 'vcdiff'];

let indice = null, porCrc = new Map();
let juego = null;   // {nombre, datos, salto, crc, crcSinCabecera}

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

async function cargarIndice() {
  const r = await fetch('../catalogo/parches.json');
  if (!r.ok) throw new Error('No se ha podido cargar el catálogo (' + r.status + ').');
  indice = await r.json();
  porCrc = new Map();
  for (const t of indice) for (const c of t.c) {
    if (!porCrc.has(c)) porCrc.set(c, []);
    porCrc.get(c).push(t);
  }
  log(`Catálogo cargado: ${indice.length.toLocaleString('es')} traducciones con CRC32.`);
}

// ---------------------------------------------------------------- el juego
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

async function elegirJuego(f) {
  $('juego').innerHTML = '';
  $('trads').innerHTML = '';
  $('resultado').innerHTML = '';
  juego = null;
  try {
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
    identificar();
  } catch (e) {
    log('Error: ' + e.message, 'bad');
    $('juego').innerHTML = `<p class="aviso">${esc(e.message)}</p>`;
  }
}

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
  const j = hallazgos[0].t;
  $('juego').innerHTML = `<h2>${esc(j.j)} <span class="sis">${esc(nombreSis(j.s))}</span></h2>
    <p class="muted">Tu fichero: ${esc(juego.nombre)} · CRC32 ${juego.crc}${hallazgos[0].salto ? ` · con cabecera de ${hallazgos[0].salto} bytes, que se quita al parchear` : ''}</p>`;
  $('trads').innerHTML = `<h3>${hallazgos.length === 1 ? 'Traducción encontrada' : hallazgos.length + ' traducciones encontradas'}</h3>` + hallazgos.map(({t, salto}, i) => `
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
    mostrarError(`${e.message} Puedes bajar el parche a mano${webSegura(t.u) ? ` de <a href="${esc(t.u)}" target="_blank" rel="noopener">su página</a>` : ''} y aplicarlo con «Tengo el parche».` + formularioParche('m'));
    const inp = document.querySelector('input[data-parche="m"]');
    if (inp) inp.onchange = async () => { try { await aplicarParche(await leerFichero(inp.files[0]), inp.files[0].name, t, salto); } catch (e2) { mostrarError(e2.message); } };
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
  try { await cargarIndice(); } catch (e) { log('Error: ' + e.message, 'bad'); $('juego').innerHTML = `<p class="aviso">${esc(e.message)}</p>`; }
})();

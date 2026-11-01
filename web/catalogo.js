'use strict';
// Buscador del catálogo: carga juegos.json ([título, sistema, ruta, traducciones, un clic, palabras]) y
// filtra mientras escribes, sin distinguir acentos ni mayúsculas. La búsqueda queda en el enlace (?q=).
(function () {
  const q = document.getElementById('q'), s = document.getElementById('s'), res = document.getElementById('res');
  const cuenta = document.getElementById('cuenta'), mas = document.getElementById('mas');
  const POR_VEZ = 60;
  let juegos = [], hallados = [], vistos = 0;
  const norm = t => t.normalize('NFD').replace(/[̀-ͯ]/g, '').toLowerCase();
  const esc = t => String(t).replace(/[&<>"']/g, c => ({'&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;'}[c]));
  const num = n => n.toLocaleString('es-ES');

  function pintar() {
    const trozo = hallados.slice(vistos, vistos + POR_VEZ);
    res.insertAdjacentHTML('beforeend', trozo.map(j =>
      `<li><a href="../${j[2]}"><span class="t">${esc(j[0])}</span><span class="m">${esc(SYS[j[1]] || j[1])}` +
      `${j[3] > 1 ? ` · ${j[3]} traducciones` : ''}${j[4] ? ' · <b>Un clic</b>' : ''}</span></a></li>`).join(''));
    vistos += trozo.length;
    mas.hidden = vistos >= hallados.length;
  }

  function filtrar() {
    const palabras = norm(q.value).split(/[^a-z0-9]+/).filter(Boolean);
    hallados = juegos.filter(j => (!s.value || j[1] === s.value) && palabras.every(p => j.t.includes(p)));
    // Primero los que tienen la búsqueda en el título (no solo en otro nombre o en el grupo) y los que más traducciones tienen.
    if (palabras.length) {
      const enTitulo = j => palabras.every(p => j.n.includes(p)) ? 1 : 0;
      hallados.sort((a, b) => enTitulo(b) - enTitulo(a) || b[3] - a[3]);
    }
    vistos = 0;
    res.innerHTML = '';
    pintar();
    const n = hallados.length;
    if (!q.value && !s.value) cuenta.textContent = `${num(n)} juegos. Escribe para buscar.`;
    else if (n) cuenta.textContent = `${num(n)} ${n === 1 ? 'juego' : 'juegos'}`;
    else cuenta.textContent = 'No hay ningún juego con ese nombre. Prueba con menos palabras o con su nombre en inglés.';
    const p = new URLSearchParams();
    if (q.value) p.set('q', q.value);
    if (s.value) p.set('s', s.value);
    try { history.replaceState(null, '', p.toString() ? '?' + p : location.pathname); } catch (e) { }
  }

  const p = new URLSearchParams(location.search);
  if (p.get('q')) q.value = p.get('q');
  if (p.get('s')) s.value = p.get('s');
  q.addEventListener('input', filtrar);
  s.addEventListener('change', filtrar);
  mas.addEventListener('click', pintar);
  cuenta.textContent = 'Cargando el catálogo…';
  fetch('juegos.json')
    .then(r => r.json())
    .then(d => {
      juegos = d.map(j => Object.assign(j, {n: norm(j[0]), t: norm(j[0]) + ' ' + j[5]}));
      filtrar();
    })
    .catch(() => { cuenta.textContent = 'No se pudo cargar el catálogo. Recarga la página.'; });
})();

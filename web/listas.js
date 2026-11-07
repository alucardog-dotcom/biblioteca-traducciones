'use strict';
// Favoritos y listas en la web, como en el programa: el corazón de cada tarjeta y de la ficha, «Guardar en una
// lista» en la ficha, la fila «Tus favoritos» de la portada y la página Mis listas. Se guardan en el
// navegador (localStorage), con copia en JSON para llevarlas a otro navegador. Va en todas las páginas.
(function () {
  const CLAVE = 'listas';
  const FAV = '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 20s-7.5-4.6-9.3-9.2C1.4 7.4 3.6 4 7 4c2 0 3.6 1.1 5 3 1.4-1.9 3-3 5-3 3.4 0 5.6 3.4 4.3 6.8C19.5 15.4 12 20 12 20z"/></svg>';
  const esc = s => String(s ?? '').replace(/[&<>"']/g, c => ({'&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;'}[c]));
  const $ = id => document.getElementById(id);
  // Cómo llegar a la raíz del sitio desde esta página: lo dice el enlace del icono.
  const icono = document.querySelector('link[rel="icon"]');
  const RAIZ = icono ? icono.getAttribute('href').replace(/icono\.svg$/, '') : '';
  const nuevoId = () => 'l' + Date.now().toString(36) + Math.random().toString(36).slice(2, 6);

  let datos = cargar();
  function cargar() {
    try {
      const d = JSON.parse(localStorage.getItem(CLAVE) || 'null');
      if (d && Array.isArray(d.listas)) return d;
    } catch (e) { }
    return {v: 1, listas: [{id: 'fav', nombre: 'Favoritos', juegos: []}]};
  }
  function guardar() {
    try { localStorage.setItem(CLAVE, JSON.stringify(datos)); }
    catch (e) { aviso('No se ha podido guardar en este navegador (¿ventana privada o sin espacio?).'); }
  }
  const lista = id => datos.listas.find(l => l.id === id);
  function fav() {
    let f = lista('fav');
    if (!f) { f = {id: 'fav', nombre: 'Favoritos', juegos: []}; datos.listas.unshift(f); }
    return f;
  }
  const enLista = (l, r) => l.juegos.some(j => j.r === r);

  function aviso(texto) {
    let t = $('aviso-listas');
    if (!t) { t = document.createElement('div'); t.id = 'aviso-listas'; t.className = 'aviso-flot'; t.setAttribute('role', 'status'); document.body.append(t); }
    t.textContent = texto;
    t.classList.add('ver');
    clearTimeout(t._t);
    t._t = setTimeout(() => t.classList.remove('ver'), 2600);
  }

  // Lo que se guarda de un juego: su página, el título, el sistema y hasta tres direcciones de portada
  // (absolutas, para que valgan desde cualquier página).
  function datosDe(el) {
    let p = el.dataset.port ? el.dataset.port.split(' ') : [];
    if (!p.length) {
      const img = el.querySelector('img');
      if (img) p = [img.getAttribute('src'), ...(img.dataset.alt || '').split(' ')];
    }
    p = p.filter(Boolean).map(u => { try { return new URL(u, location.href).href; } catch (e) { return ''; } }).filter(Boolean).slice(0, 3);
    return {r: el.dataset.ruta, t: el.dataset.titulo || '', s: el.dataset.sis || '', sn: el.dataset.sisn || el.dataset.sis || '', p};
  }

  function alternarFav(el) {
    const j = datosDe(el), f = fav();
    const i = f.juegos.findIndex(x => x.r === j.r);
    if (i >= 0) f.juegos.splice(i, 1); else f.juegos.unshift(j);
    guardar();
    marcar();
    aviso(i >= 0 ? `Quitado de Favoritos: ${j.t}` : `Guardado en Favoritos: ${j.t}`);
    if ($('listas')) pintarListas();
  }

  // Corazones al día (también los de las tarjetas que llegan después, por el observador de abajo).
  function marcar() {
    const f = fav();
    document.querySelectorAll('[data-ruta]').forEach(el => {
      const b = el.querySelector('.fav');
      if (!b) return;
      const on = enLista(f, el.dataset.ruta);
      b.setAttribute('aria-pressed', on);
      b.title = on ? 'Quitar de Favoritos' : 'Añadir a Favoritos';
      b.setAttribute('aria-label', b.title);
    });
    const nav = $('nav-listas');
    if (nav) nav.textContent = f.juegos.length ? `Mis listas (${f.juegos.length})` : 'Mis listas';
  }

  function tarjeta(j, quitarDe) {
    const img = j.p && j.p.length ? `<img loading="lazy" src="${esc(j.p[0])}" data-alt="${esc(j.p.slice(1).join(' '))}" alt="" referrerpolicy="no-referrer" onerror="sig(this)" onload="this.classList.add('ok')">` : '';
    return `<li class="tarjeta${quitarDe ? ' con-acc' : ''}" data-ruta="${esc(j.r)}" data-sis="${esc(j.s)}" data-sisn="${esc(j.sn)}" data-titulo="${esc(j.t)}" data-port="${esc((j.p || []).join(' '))}">
      <a href="${esc(RAIZ + j.r)}"><span class="port${img ? '' : ' sin'}">${img}<span class="ph">${esc(j.t)}</span></span><span class="cuerpo"><span class="t">${esc(j.t)}</span><span class="m">${esc(j.sn || j.s)}</span></span></a>
      ${quitarDe ? `<div class="acc"><button type="button" class="btn" data-quitar="${esc(quitarDe)}" data-r="${esc(j.r)}">Quitar</button></div>` : ''}
      <button type="button" class="fav" aria-pressed="false" aria-label="Añadir a Favoritos">${FAV}</button></li>`;
  }

  // ---------------------------------------------------------------- «Guardar en una lista» (ficha del juego)
  function menuListas(boton) {
    const el = boton.closest('[data-ruta]');
    let m = $('menu-listas');
    if (m) { m.remove(); return; }
    m = document.createElement('div');
    m.id = 'menu-listas';
    m.className = 'menu-listas';
    const r = el.dataset.ruta;
    const pinta = () => {
      m.innerHTML = `<p class="muted">Guardar en…</p>${datos.listas.map(l => `<label><input type="checkbox" data-lista="${esc(l.id)}" ${enLista(l, r) ? 'checked' : ''}> ${esc(l.nombre)}</label>`).join('')}
        <form class="nueva"><input type="text" placeholder="Nueva lista" aria-label="Nombre de la lista nueva" maxlength="40"><button class="btn" type="submit">Crear</button></form>`;
    };
    pinta();
    boton.after(m);
    m.addEventListener('change', e => {
      const l = lista(e.target.dataset.lista);
      if (!l) return;
      const i = l.juegos.findIndex(x => x.r === r);
      if (e.target.checked && i < 0) l.juegos.unshift(datosDe(el));
      if (!e.target.checked && i >= 0) l.juegos.splice(i, 1);
      guardar();
      marcar();
      aviso(e.target.checked ? `Guardado en ${l.nombre}` : `Quitado de ${l.nombre}`);
    });
    m.querySelector('form').addEventListener('submit', e => {
      e.preventDefault();
      const nombre = m.querySelector('input[type="text"]').value.trim();
      if (!nombre) return;
      const l = {id: nuevoId(), nombre, juegos: [datosDe(el)]};
      datos.listas.push(l);
      guardar();
      pinta();
      aviso(`Lista «${nombre}» creada con este juego.`);
    });
    setTimeout(() => document.addEventListener('click', function fuera(e) {
      if (!m.contains(e.target) && e.target !== boton) { m.remove(); document.removeEventListener('click', fuera); }
    }), 0);
  }

  // ---------------------------------------------------------------- página Mis listas
  function pintarListas() {
    const c = $('listas');
    if (!c) return;
    const total = datos.listas.reduce((n, l) => n + l.juegos.length, 0);
    c.innerHTML = datos.listas.map(l => `<section class="lista-b" data-lista="${esc(l.id)}">
      <div class="lista-h"><h2>${esc(l.nombre)}</h2><span class="muted">${l.juegos.length === 1 ? '1 juego' : l.juegos.length + ' juegos'}</span>
        ${l.id === 'fav' ? '' : `<button type="button" class="btn ghost" data-renombrar="${esc(l.id)}">Cambiar el nombre</button><button type="button" class="btn ghost" data-borrar="${esc(l.id)}">Borrar la lista</button>`}</div>
      ${l.juegos.length ? `<ul class="tarjetas">${l.juegos.map(j => tarjeta(j, l.id)).join('')}</ul>`
        : `<p class="muted">${l.id === 'fav' ? 'Todavía no tienes favoritos. Pulsa el corazón de cualquier juego para guardarlo aquí.' : 'Esta lista está vacía. En la ficha de un juego, pulsa «Guardar en una lista».'}</p>`}
    </section>`).join('') + `
    <section class="lista-b"><div class="lista-h"><h2>Nueva lista</h2></div>
      <form class="nueva" id="nueva-lista"><input type="text" placeholder="Nombre de la lista" aria-label="Nombre de la lista" maxlength="40"><button class="btn" type="submit">Crear</button></form></section>
    <section class="lista-b"><div class="lista-h"><h2>Copia de seguridad</h2></div>
      <p class="muted">Las listas viven en este navegador. Descarga una copia para llevarlas a otro o por si borras los datos del navegador.</p>
      <div class="acciones-l"><button type="button" class="btn" id="exportar" ${total ? '' : 'disabled'}>Descargar copia (JSON)</button><label class="btn">Restaurar copia <input type="file" id="importar" accept=".json,application/json" hidden></label></div></section>`;
    marcar();
    c.querySelectorAll('[data-quitar]').forEach(b => b.onclick = () => {
      const l = lista(b.dataset.quitar);
      if (!l) return;
      l.juegos = l.juegos.filter(j => j.r !== b.dataset.r);
      guardar(); pintarListas();
    });
    c.querySelectorAll('[data-renombrar]').forEach(b => b.onclick = () => {
      const l = lista(b.dataset.renombrar);
      const nombre = prompt('Nombre de la lista', l.nombre);
      if (nombre && nombre.trim()) { l.nombre = nombre.trim().slice(0, 40); guardar(); pintarListas(); }
    });
    c.querySelectorAll('[data-borrar]').forEach(b => b.onclick = () => {
      const l = lista(b.dataset.borrar);
      if (confirm(`¿Borrar la lista «${l.nombre}»${l.juegos.length ? ` con sus ${l.juegos.length} juegos` : ''}?`)) {
        datos.listas = datos.listas.filter(x => x.id !== l.id); guardar(); pintarListas();
      }
    });
    $('nueva-lista').onsubmit = e => {
      e.preventDefault();
      const nombre = e.target.querySelector('input').value.trim();
      if (!nombre) return;
      datos.listas.push({id: nuevoId(), nombre: nombre.slice(0, 40), juegos: []});
      guardar(); pintarListas();
    };
    $('exportar').onclick = () => {
      const a = document.createElement('a');
      a.href = URL.createObjectURL(new Blob([JSON.stringify(datos, null, 1)], {type: 'application/json'}));
      a.download = `biblioteca-listas-${new Date().toISOString().slice(0, 10)}.json`;
      a.click();
      setTimeout(() => URL.revokeObjectURL(a.href), 1000);
    };
    $('importar').onchange = async e => {
      const f = e.target.files[0];
      if (!f) return;
      try {
        const d = JSON.parse(await f.text());
        if (!d || !Array.isArray(d.listas)) throw new Error('no es una copia de las listas');
        // Se juntan con las que ya hay: la misma lista (por nombre) suma sus juegos sin repetir.
        for (const l of d.listas) {
          if (!l || !Array.isArray(l.juegos)) continue;
          let mia = l.id === 'fav' ? fav() : datos.listas.find(x => x.id !== 'fav' && x.nombre === l.nombre);
          if (!mia) { mia = {id: nuevoId(), nombre: String(l.nombre || 'Lista').slice(0, 40), juegos: []}; datos.listas.push(mia); }
          for (const j of l.juegos) if (j && typeof j.r === 'string' && !enLista(mia, j.r)) mia.juegos.push({r: j.r, t: String(j.t || ''), s: String(j.s || ''), sn: String(j.sn || ''), p: Array.isArray(j.p) ? j.p.filter(u => /^https?:\/\//.test(u)).slice(0, 3) : []});
        }
        guardar(); pintarListas(); aviso('Copia restaurada.');
      } catch (err) { aviso('No se pudo leer la copia: ' + err.message); }
      e.target.value = '';
    };
  }

  // ---------------------------------------------------------------- portada: Tus favoritos
  function favoritosInicio() {
    const c = $('favoritos-inicio');
    if (!c) return;
    const f = fav();
    if (!f.juegos.length) { c.innerHTML = ''; return; }
    c.innerHTML = `<section class="bloque"><div class="lista-h"><h2>Tus favoritos</h2><a class="suave" href="${esc(RAIZ)}mis-listas/">Mis listas</a></div>
      <ul class="tarjetas">${f.juegos.slice(0, 12).map(j => tarjeta(j)).join('')}</ul></section>`;
  }

  // ---------------------------------------------------------------- arranque
  document.addEventListener('click', e => {
    const b = e.target.closest('.fav');
    if (b) {
      e.preventDefault();
      const el = b.closest('[data-ruta]');
      if (el) alternarFav(el);
      return;
    }
    const g = e.target.closest('#guardar-lista');
    if (g) { e.preventDefault(); menuListas(g); }
  });
  const listo = () => {
    favoritosInicio();
    pintarListas();
    marcar();
    // Las tarjetas que pinta JavaScript más tarde (buscador, Mis juegos) también llevan corazón.
    let t = null;
    new MutationObserver(() => { clearTimeout(t); t = setTimeout(marcar, 50); }).observe(document.body, {childList: true, subtree: true});
  };
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', listo); else listo();
  window.Listas = {datos: () => datos};
})();

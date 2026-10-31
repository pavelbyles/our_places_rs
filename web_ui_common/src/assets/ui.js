// ui.js — the only client script of the redesigned portals (besides htmx).
// Everything is wired through data-* attributes so no inline script or
// handler is needed (strict CSP: script-src 'self', no eval).
(function () {
  'use strict';

  var THEMES = ['light', 'dim', 'dark'];
  var THEME_LABELS = {
    light: 'Switch to light theme',
    dim: 'Switch to dim theme',
    dark: 'Switch to dark theme'
  };
  var YEAR_SECONDS = 31536000;

  // Cookie access throws where cookies are blocked; that must never stop the
  // rest of ui.js (the theme still switches for this page view).
  function setCookie(name, value) {
    var secure = window.location.protocol === 'https:' ? '; Secure' : '';
    try {
      document.cookie = name + '=' + encodeURIComponent(value) +
        '; Path=/; Max-Age=' + YEAR_SECONDS + '; SameSite=Lax' + secure;
    } catch (e) { /* cookies unavailable */ }
  }

  function getCookie(name) {
    var raw = '';
    try { raw = document.cookie; } catch (e) { return null; }
    var parts = raw ? raw.split('; ') : [];
    for (var i = 0; i < parts.length; i++) {
      var eq = parts[i].indexOf('=');
      if (parts[i].slice(0, eq) === name) {
        return decodeURIComponent(parts[i].slice(eq + 1));
      }
    }
    return null;
  }

  // --- Theme: light -> dim -> dark, remembered in the op_theme cookie. ----
  function currentTheme() {
    var cls = document.documentElement.classList;
    if (cls.contains('dark')) return 'dark';
    if (cls.contains('dim')) return 'dim';
    return 'light';
  }

  function applyTheme(theme) {
    var cls = document.documentElement.classList;
    cls.remove('dim', 'dark');
    if (theme !== 'light') cls.add(theme);
    var next = THEMES[(THEMES.indexOf(theme) + 1) % THEMES.length];
    var toggles = document.querySelectorAll('[data-theme-cycle]');
    for (var i = 0; i < toggles.length; i++) {
      toggles[i].setAttribute('aria-label', THEME_LABELS[next]);
      toggles[i].setAttribute('data-theme-current', theme);
    }
  }

  function cycleTheme() {
    var next = THEMES[(THEMES.indexOf(currentTheme()) + 1) % THEMES.length];
    applyTheme(next);
    setCookie('op_theme', next);
  }

  // --- Visitor time zone, so the server can default to dark after 18:00. --
  function storeTimezoneOffset() {
    var offset = String(-new Date().getTimezoneOffset());
    if (getCookie('op_tz_offset') !== offset) setCookie('op_tz_offset', offset);
  }

  // --- Dialogs (native <dialog>: Escape and focus trapping are built in). --
  function openDialog(id) {
    var dialog = document.getElementById(id);
    if (dialog && typeof dialog.showModal === 'function' && !dialog.open) dialog.showModal();
  }

  // --- Toasts, from an HX-Trigger: {"toast": {"message": ..., "variant": ...}}
  function showToast(detail) {
    var region = document.getElementById('toast-region');
    if (!region || !detail) return;
    var message = typeof detail === 'string' ? detail : (detail.message || detail.value);
    if (!message) return;
    var toast = document.createElement('div');
    toast.className = 'toast';
    toast.setAttribute('role', 'status');
    toast.setAttribute('data-variant', detail.variant || 'info');
    toast.textContent = String(message);
    region.appendChild(toast);
    window.setTimeout(function () { toast.remove(); }, 5000);
  }

  // --- Photo viewer: a [data-lightbox-index] inside a [data-lightbox-group]
  // opens the group's viewer <dialog data-lightbox> (its id is the group's
  // attribute value) on that photo. [data-lightbox-step] buttons and the
  // left/right arrow keys move through the group, wrapping at the ends.
  function viewerItems(viewer) {
    var group = document.querySelector('[data-lightbox-group="' + viewer.id + '"]');
    return group ? group.querySelectorAll('[data-lightbox-index]') : [];
  }

  function showPhoto(viewer, index) {
    var items = viewerItems(viewer);
    if (!items.length) return;
    var i = ((index % items.length) + items.length) % items.length;
    var img = viewer.querySelector('[data-lightbox-img]');
    var counter = viewer.querySelector('[data-lightbox-counter]');
    viewer.setAttribute('data-index', String(i));
    if (img) {
      img.src = items[i].getAttribute('data-full-src') || '';
      img.alt = items[i].getAttribute('data-alt') || '';
    }
    if (counter) counter.textContent = (i + 1) + ' / ' + items.length;
  }

  function stepPhoto(viewer, delta) {
    showPhoto(viewer, Number(viewer.getAttribute('data-index') || 0) + delta);
  }

  document.addEventListener('click', function (event) {
    var target = event.target instanceof Element ? event.target : null;
    if (!target) return;

    var thumb = target.closest('[data-lightbox-index]');
    var group = thumb && thumb.closest('[data-lightbox-group]');
    var viewer = group && document.getElementById(group.getAttribute('data-lightbox-group'));
    if (viewer) {
      event.preventDefault();
      showPhoto(viewer, Number(thumb.getAttribute('data-lightbox-index')));
      if (typeof viewer.showModal === 'function' && !viewer.open) viewer.showModal();
      return;
    }
    var stepper = target.closest('[data-lightbox-step]');
    if (stepper) {
      var current = stepper.closest('dialog[data-lightbox]');
      if (current) stepPhoto(current, Number(stepper.getAttribute('data-lightbox-step')));
      return;
    }

    var opener = target.closest('[data-dialog-open]');
    if (opener) {
      event.preventDefault();
      openDialog(opener.getAttribute('data-dialog-open'));
      return;
    }
    var closer = target.closest('[data-dialog-close]');
    if (closer) {
      var dialog = closer.closest('dialog');
      if (dialog) dialog.close();
      return;
    }
    // A click on the <dialog> element itself landed on its backdrop.
    if (target.tagName === 'DIALOG' && target.open) {
      target.close();
      return;
    }
    if (target.closest('[data-theme-cycle]')) {
      cycleTheme();
      return;
    }
    if (target.closest('[data-print]')) {
      window.print();
      return;
    }
    // Close open dropdowns/sheets (<details data-dismiss-outside>) on an
    // outside click.
    var open = document.querySelectorAll('details[data-dismiss-outside][open]');
    for (var i = 0; i < open.length; i++) {
      if (!open[i].contains(target)) open[i].removeAttribute('open');
    }
  });

  document.addEventListener('keydown', function (event) {
    if (event.key !== 'Escape') return;
    var open = document.querySelectorAll('details[data-dismiss-outside][open]');
    for (var i = 0; i < open.length; i++) {
      open[i].removeAttribute('open');
      var summary = open[i].querySelector('summary');
      if (summary) summary.focus();
    }
  });

  document.addEventListener('toast', function (event) { showToast(event.detail); });

  // --- Listing photo uploads (admin): ask the portal for signed URLs, then
  // PUT each file straight to Cloud Storage. No image bytes go through the
  // portal itself.
  document.addEventListener('submit', function (event) {
    var form = event.target instanceof HTMLFormElement ? event.target : null;
    if (!form || !form.hasAttribute('data-upload')) return;
    event.preventDefault();
    var input = form.querySelector('input[type="file"]');
    var status = form.querySelector('[data-upload-status]');
    var say = function (text) { if (status) status.textContent = text; };
    var files = input && input.files ? Array.prototype.slice.call(input.files) : [];
    if (!files.length) { say('Choose at least one photo.'); return; }
    say('Uploading ' + files.length + ' photo(s)…');
    fetch(form.getAttribute('data-upload'), {
      method: 'POST',
      credentials: 'same-origin',
      headers: { 'Content-Type': 'application/json', Accept: 'application/json' },
      body: JSON.stringify({ files: files.map(function (f) { return { content_type: f.type, size_bytes: f.size }; }) })
    })
      .then(function (res) {
        if (!res.ok) throw new Error('rejected');
        return res.json();
      })
      .then(function (signed) {
        return Promise.all(signed.map(function (s) {
          var file = files[Number(s.client_file_id)];
          return fetch(s.upload_url, { method: 'PUT', headers: { 'Content-Type': file.type }, body: file })
            .then(function (r) { return r.ok; }, function () { return false; });
        }));
      })
      .then(function (results) {
        var ok = results.filter(Boolean).length;
        if (input) input.value = '';
        if (!ok) { say('Upload failed. Try again.'); return; }
        say((ok === results.length
          ? 'Uploaded ' + ok + ' photo(s).'
          : 'Uploaded ' + ok + ' of ' + results.length + ' photo(s). Try the others again.') + ' Processing…');
        waitForPhotos(ok, say);
      })
      .catch(function () { say('Upload failed. Check the files (JPEG, PNG or WebP, under 15 MB) and try again.'); });
  });

  // Photos appear once image_worker has resized them: re-read the page's
  // photo slot every few seconds until the new ones are there.
  function waitForPhotos(added, say) {
    var slot = document.getElementById('listing-photos-slot');
    if (!slot) return;
    var target = Number(slot.getAttribute('data-photo-count') || 0) + added;
    var tries = 0;
    var poll = function () {
      tries += 1;
      fetch(location.href, { credentials: 'same-origin', headers: { Accept: 'text/html' } })
        .then(function (res) { return res.ok ? res.text() : ''; })
        .then(function (html) {
          var fresh = new DOMParser().parseFromString(html, 'text/html').getElementById('listing-photos-slot');
          var count = fresh ? Number(fresh.getAttribute('data-photo-count') || 0) : 0;
          if (fresh && count >= target) {
            slot.replaceWith(fresh);
            say('Photos ready.');
          } else if (tries < 30) {
            setTimeout(poll, 3000);
          } else {
            if (fresh) slot.replaceWith(fresh);
            say('Still processing. Reload the page in a minute to see the rest.');
          }
        }, function () { if (tries < 30) setTimeout(poll, 3000); });
    };
    setTimeout(poll, 2000);
  }

  // Clear a form after htmx submits it successfully (e.g. the message box).
  document.addEventListener('htmx:afterRequest', function (event) {
    var form = event.target instanceof HTMLFormElement ? event.target : null;
    if (form && form.hasAttribute('data-reset-on-success') && event.detail && event.detail.successful) {
      form.reset();
    }
  });

  // --- Broken images: hide the <img> so the placeholder tile behind it
  // shows instead of the browser's broken-image icon and alt text. Error
  // events do not bubble, so listen in the capture phase; images that failed
  // before this script ran are caught by the scan below.
  function markBroken(img) { img.setAttribute('data-broken', ''); }
  document.addEventListener('error', function (event) {
    if (event.target instanceof HTMLImageElement) markBroken(event.target);
  }, true);
  var settled = document.images;
  for (var b = 0; b < settled.length; b++) {
    if (settled[b].complete && settled[b].naturalWidth === 0 && settled[b].getAttribute('src')) markBroken(settled[b]);
  }

  // --- Stay-dates calendar (server-rendered by date_range_calendar). -------
  // A booked night can still be the check-out day: the guest leaves that
  // morning. A range may not include any booked or past night.
  var DAY_MS = 86400000;

  function parseDay(iso) { return iso ? Date.parse(iso + 'T00:00:00Z') : NaN; }
  function isoDay(ms) { return new Date(ms).toISOString().slice(0, 10); }

  function calendarParts(root) {
    return {
      checkIn: root.querySelector('input[name="check_in"]'),
      checkOut: root.querySelector('input[name="check_out"]'),
      status: root.querySelector('[data-calendar-status]'),
      buttons: root.querySelectorAll('button[data-date]')
    };
  }

  function dayButton(root, iso) {
    return root.querySelector('button[data-date="' + iso + '"]');
  }

  // The second month is hidden when the calendar is narrow; its buttons
  // still carry availability but cannot take focus.
  function isShown(el) { return el.getClientRects().length > 0; }

  function rangeIsFree(root, startIso, endIso) {
    for (var t = parseDay(startIso); t < parseDay(endIso); t += DAY_MS) {
      var btn = dayButton(root, isoDay(t));
      // Nights outside the rendered months are re-checked by the server.
      if (btn && btn.getAttribute('data-state') !== 'available') return false;
    }
    return true;
  }

  function paintCalendar(root) {
    var p = calendarParts(root);
    var start = p.checkIn ? p.checkIn.value : '';
    var end = p.checkOut ? p.checkOut.value : '';
    for (var i = 0; i < p.buttons.length; i++) {
      var b = p.buttons[i];
      var iso = b.getAttribute('data-date');
      var t = parseDay(iso);
      var isEnd = iso === start || iso === end;
      b.toggleAttribute('data-selected', isEnd);
      b.toggleAttribute('data-in-range', !!(start && end && t > parseDay(start) && t < parseDay(end)));
      if (b.parentElement) b.parentElement.setAttribute('aria-selected', isEnd ? 'true' : 'false');
    }
  }

  function announce(root, text) {
    var status = calendarParts(root).status;
    if (status) status.textContent = text;
  }

  function setRoving(root, button) {
    var buttons = calendarParts(root).buttons;
    for (var i = 0; i < buttons.length; i++) buttons[i].setAttribute('tabindex', '-1');
    if (button) button.setAttribute('tabindex', '0');
  }

  function enhanceCalendar(root) {
    root.setAttribute('data-enhanced', '');
    var p = calendarParts(root);
    var focusIso = (p.checkIn && p.checkIn.value) || '';
    var shown = Array.prototype.filter.call(p.buttons, isShown);
    var picked = focusIso && dayButton(root, focusIso);
    var first = (picked && isShown(picked) ? picked : null) ||
      shown.filter(function (b) { return b.getAttribute('data-state') === 'available'; })[0] ||
      shown[0] || p.buttons[0];
    setRoving(root, first);
    paintCalendar(root);
  }

  function enhanceCalendars(scope) {
    var roots = (scope || document).querySelectorAll('[data-calendar]');
    for (var i = 0; i < roots.length; i++) enhanceCalendar(roots[i]);
  }

  function pickDay(root, button) {
    var p = calendarParts(root);
    if (!p.checkIn || !p.checkOut) return;
    var iso = button.getAttribute('data-date');
    var state = button.getAttribute('data-state');
    var start = p.checkIn.value;
    var choosingEnd = start && !p.checkOut.value && parseDay(iso) > parseDay(start);

    if (choosingEnd) {
      if (state === 'past' || !rangeIsFree(root, start, iso)) {
        announce(root, 'Those dates include booked nights. Choose an earlier check-out.');
        return;
      }
      p.checkOut.value = iso;
      announce(root, 'Check-out ' + iso + ' selected.');
    } else {
      if (state !== 'available') {
        announce(root, 'That night is not available.');
        return;
      }
      p.checkIn.value = iso;
      p.checkOut.value = '';
      announce(root, 'Check-in ' + iso + ' selected. Choose a check-out date.');
    }
    root.setAttribute('data-check-in', p.checkIn.value);
    root.setAttribute('data-check-out', p.checkOut.value);
    setRoving(root, button);
    paintCalendar(root);
    root.dispatchEvent(new Event('change', { bubbles: true }));
  }

  function moveFocus(root, fromIso, days) {
    var target = dayButton(root, isoDay(parseDay(fromIso) + days * DAY_MS));
    if (target && isShown(target)) {
      setRoving(root, target);
      target.focus();
      return true;
    }
    return false;
  }

  document.addEventListener('click', function (event) {
    var target = event.target instanceof Element ? event.target : null;
    var button = target && target.closest('[data-calendar] button[data-date]');
    if (!button) return;
    pickDay(button.closest('[data-calendar]'), button);
  });

  document.addEventListener('keydown', function (event) {
    var target = event.target instanceof Element ? event.target : null;
    var button = target && target.closest('[data-calendar] button[data-date]');
    if (!button) return;
    var root = button.closest('[data-calendar]');
    var iso = button.getAttribute('data-date');
    var t = new Date(parseDay(iso));
    var step = { ArrowLeft: -1, ArrowRight: 1, ArrowUp: -7, ArrowDown: 7 }[event.key];
    if (event.key === 'Home') step = -t.getUTCDay();
    if (event.key === 'End') step = 6 - t.getUTCDay();
    if (event.key === 'PageUp' || event.key === 'PageDown') {
      var forward = event.key === 'PageDown';
      var other = new Date(t.getTime());
      other.setUTCMonth(t.getUTCMonth() + (forward ? 1 : -1));
      step = Math.round((other.getTime() - t.getTime()) / DAY_MS);
    }
    if (step === undefined) return;
    event.preventDefault();
    if (!moveFocus(root, iso, step)) {
      // Outside the shown months: ask the server for the next/previous page.
      var nav = root.querySelector(step > 0 ? '[aria-label="Next month"]' : '[aria-label="Previous month"]');
      if (nav && !nav.disabled) nav.click();
    }
  });

  // Typed dates repaint the grid (the change event still bubbles to
  // #booking-dates for htmx).
  document.addEventListener('change', function (event) {
    var input = event.target instanceof Element ? event.target : null;
    var root = input && input.matches('[data-calendar] input[type="date"]') && input.closest('[data-calendar]');
    if (!root) return;
    var p = calendarParts(root);
    root.setAttribute('data-check-in', p.checkIn ? p.checkIn.value : '');
    root.setAttribute('data-check-out', p.checkOut ? p.checkOut.value : '');
    paintCalendar(root);
  });

  // Re-enhance calendars that htmx swaps in (prev/next month), and open
  // dialogs that arrive with data-auto-open (e.g. the cancel confirmation).
  document.addEventListener('htmx:load', function (event) {
    var el = event.target instanceof Element ? event.target : null;
    if (!el) return;
    if (el.matches('[data-calendar]')) enhanceCalendar(el);
    else enhanceCalendars(el);
    var dialogs = el.matches('dialog[data-auto-open]') ? [el] : el.querySelectorAll('dialog[data-auto-open]');
    for (var i = 0; i < dialogs.length; i++) {
      if (!dialogs[i].open && typeof dialogs[i].showModal === 'function') dialogs[i].showModal();
    }
  });

  document.addEventListener('keydown', function (event) {
    if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') return;
    var viewer = document.querySelector('dialog[data-lightbox][open]');
    if (!viewer) return;
    event.preventDefault();
    stepPhoto(viewer, event.key === 'ArrowLeft' ? -1 : 1);
  });

  // --- Character counters: <p data-char-count="<field id>"> shows
  // "used / maxlength" as the user types (the server renders the first one).
  document.addEventListener('input', function (event) {
    var field = event.target instanceof Element ? event.target : null;
    if (!field || !field.id || !field.hasAttribute('maxlength')) return;
    var counter = document.querySelector('[data-char-count="' + field.id + '"]');
    if (counter) counter.textContent = field.value.length + ' / ' + field.getAttribute('maxlength');
  });

  // --- Carousels: [data-carousel] holds a scrolling [data-carousel-track]
  // and [data-carousel-step="-1"|"1"] arrows (server-rendered hidden). The
  // arrows show only when the track overflows, scroll one card per click,
  // and are disabled at either end.
  var reduceMotion = window.matchMedia && window.matchMedia('(prefers-reduced-motion: reduce)');

  function carouselParts(root) {
    return {
      track: root.querySelector('[data-carousel-track]'),
      arrows: root.querySelectorAll('[data-carousel-step]')
    };
  }

  function updateCarousel(root) {
    var p = carouselParts(root);
    if (!p.track) return;
    var max = p.track.scrollWidth - p.track.clientWidth;
    var overflows = max > 1;
    for (var i = 0; i < p.arrows.length; i++) {
      var back = Number(p.arrows[i].getAttribute('data-carousel-step')) < 0;
      p.arrows[i].hidden = !overflows;
      p.arrows[i].disabled = back ? p.track.scrollLeft <= 1 : p.track.scrollLeft >= max - 1;
    }
  }

  function updateCarousels() {
    var roots = document.querySelectorAll('[data-carousel]');
    for (var i = 0; i < roots.length; i++) updateCarousel(roots[i]);
  }

  function stepCarousel(root, dir) {
    var track = carouselParts(root).track;
    if (!track) return;
    var card = track.firstElementChild;
    var gap = parseFloat(window.getComputedStyle(track).columnGap) || 0;
    var distance = card ? card.getBoundingClientRect().width + gap : track.clientWidth;
    track.scrollBy({ left: dir * distance, behavior: reduceMotion && reduceMotion.matches ? 'auto' : 'smooth' });
  }

  document.addEventListener('click', function (event) {
    var target = event.target instanceof Element ? event.target : null;
    var arrow = target && target.closest('[data-carousel-step]');
    var root = arrow && arrow.closest('[data-carousel]');
    if (root) stepCarousel(root, Number(arrow.getAttribute('data-carousel-step')));
  });

  // Scroll events do not bubble; catch track scrolls in the capture phase.
  // Measure at most once per frame per carousel.
  var pendingCarousels = [];
  document.addEventListener('scroll', function (event) {
    var track = event.target instanceof Element ? event.target : null;
    var root = track && track.matches('[data-carousel-track]') && track.closest('[data-carousel]');
    if (!root || pendingCarousels.indexOf(root) !== -1) return;
    pendingCarousels.push(root);
    if (pendingCarousels.length > 1) return;
    window.requestAnimationFrame(function () {
      var roots = pendingCarousels;
      pendingCarousels = [];
      for (var i = 0; i < roots.length; i++) updateCarousel(roots[i]);
    });
  }, true);
  window.addEventListener('resize', updateCarousels);
  // Lazy photos can change card widths after first paint.
  window.addEventListener('load', updateCarousels);

  function init() {
    applyTheme(currentTheme());
    enhanceCalendars(document);
    updateCarousels();
    storeTimezoneOffset();
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', init);
  } else {
    init();
  }

  window.OurPlacesUi = { showToast: showToast, cycleTheme: cycleTheme };
})();

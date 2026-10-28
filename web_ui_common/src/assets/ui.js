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

  function setCookie(name, value) {
    var secure = window.location.protocol === 'https:' ? '; Secure' : '';
    document.cookie = name + '=' + encodeURIComponent(value) +
      '; Path=/; Max-Age=' + YEAR_SECONDS + '; SameSite=Lax' + secure;
  }

  function getCookie(name) {
    var parts = document.cookie ? document.cookie.split('; ') : [];
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

  document.addEventListener('click', function (event) {
    var target = event.target instanceof Element ? event.target : null;
    if (!target) return;

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

  function init() {
    applyTheme(currentTheme());
    storeTimezoneOffset();
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', init);
  } else {
    init();
  }

  window.OurPlacesUi = { showToast: showToast, cycleTheme: cycleTheme };
})();

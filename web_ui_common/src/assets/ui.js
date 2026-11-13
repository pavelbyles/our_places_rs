// ui.js — the only client script of the redesigned portals (besides htmx).
// Everything is wired through data-* attributes so no inline script or
// handler is needed (strict CSP: script-src 'self', no eval).
(function () {
  "use strict";

  var THEMES = ["light", "dim", "dark"];
  var THEME_LABELS = {
    light: "Switch to light theme",
    dim: "Switch to dim theme",
    dark: "Switch to dark theme",
  };
  var YEAR_SECONDS = 31536000;

  // Cookie access throws where cookies are blocked; that must never stop the
  // rest of ui.js (the theme still switches for this page view).
  function setCookie(name, value) {
    var secure = window.location.protocol === "https:" ? "; Secure" : "";
    try {
      document.cookie =
        name +
        "=" +
        encodeURIComponent(value) +
        "; Path=/; Max-Age=" +
        YEAR_SECONDS +
        "; SameSite=Lax" +
        secure;
    } catch (e) {
      /* cookies unavailable */
    }
  }

  function getCookie(name) {
    var raw = "";
    try {
      raw = document.cookie;
    } catch (e) {
      return null;
    }
    var parts = raw ? raw.split("; ") : [];
    for (var i = 0; i < parts.length; i++) {
      var eq = parts[i].indexOf("=");
      if (parts[i].slice(0, eq) === name) {
        return decodeURIComponent(parts[i].slice(eq + 1));
      }
    }
    return null;
  }

  // --- Theme: light -> dim -> dark, remembered in the op_theme cookie. ----
  function currentTheme() {
    var cls = document.documentElement.classList;
    if (cls.contains("dark")) return "dark";
    if (cls.contains("dim")) return "dim";
    return "light";
  }

  function applyTheme(theme) {
    var cls = document.documentElement.classList;
    cls.remove("dim", "dark");
    if (theme !== "light") cls.add(theme);
    var next = THEMES[(THEMES.indexOf(theme) + 1) % THEMES.length];
    var toggles = document.querySelectorAll("[data-theme-cycle]");
    for (var i = 0; i < toggles.length; i++) {
      toggles[i].setAttribute("aria-label", THEME_LABELS[next]);
      toggles[i].setAttribute("data-theme-current", theme);
    }
  }

  function cycleTheme() {
    var next = THEMES[(THEMES.indexOf(currentTheme()) + 1) % THEMES.length];
    applyTheme(next);
    setCookie("op_theme", next);
  }

  // --- Visitor time zone, so the server can default to dark after 18:00. --
  function storeTimezoneOffset() {
    var offset = String(-new Date().getTimezoneOffset());
    if (getCookie("op_tz_offset") !== offset) setCookie("op_tz_offset", offset);
  }

  // --- Dialogs (native <dialog>: Escape and focus trapping are built in). --
  function openDialog(id) {
    var dialog = document.getElementById(id);
    if (dialog && typeof dialog.showModal === "function" && !dialog.open)
      dialog.showModal();
  }

  // --- Toasts, from an HX-Trigger: {"toast": {"message": ..., "variant": ...}}
  function showToast(detail) {
    var region = document.getElementById("toast-region");
    if (!region || !detail) return;
    var message =
      typeof detail === "string" ? detail : detail.message || detail.value;
    if (!message) return;
    var toast = document.createElement("div");
    toast.className = "toast";
    toast.setAttribute("role", "status");
    toast.setAttribute("data-variant", detail.variant || "info");
    toast.textContent = String(message);
    region.appendChild(toast);
    window.setTimeout(function () {
      toast.remove();
    }, 5000);
  }

  // Server-rendered toasts marked [data-toast-auto] (e.g. swapped into
  // #toast-region out of band) go away like the ones above.
  function autoDismissToasts(scope) {
    var toasts = scope.matches("[data-toast-auto]")
      ? [scope]
      : scope.querySelectorAll("[data-toast-auto]");
    Array.prototype.forEach.call(toasts, function (toast) {
      if (toast.hasAttribute("data-toast-timer")) return;
      toast.setAttribute("data-toast-timer", "");
      window.setTimeout(function () {
        toast.remove();
      }, 5000);
    });
  }

  // --- Photo viewer: a [data-lightbox-index] inside a [data-lightbox-group]
  // opens the group's viewer <dialog data-lightbox> (its id is the group's
  // attribute value) on that photo. [data-lightbox-step] buttons and the
  // left/right arrow keys move through the group, wrapping at the ends.
  function viewerItems(viewer) {
    var group = document.querySelector(
      '[data-lightbox-group="' + viewer.id + '"]',
    );
    return group ? group.querySelectorAll("[data-lightbox-index]") : [];
  }

  function showPhoto(viewer, index) {
    var items = viewerItems(viewer);
    if (!items.length) return;
    var i = ((index % items.length) + items.length) % items.length;
    var img = viewer.querySelector("[data-lightbox-img]");
    var counter = viewer.querySelector("[data-lightbox-counter]");
    var caption = viewer.querySelector("[data-lightbox-caption]");
    viewer.setAttribute("data-index", String(i));
    if (img) {
      // srcset first, so the browser picks a variant fitting the viewer
      // (its `sizes`) instead of fetching the full-size fallback.
      img.srcset = items[i].getAttribute("data-full-srcset") || "";
      img.src = items[i].getAttribute("data-full-src") || "";
      img.alt = items[i].getAttribute("data-alt") || "";
    }
    if (counter) counter.textContent = i + 1 + " / " + items.length;
    // The host's photo name, shown only here; empty for unnamed photos.
    if (caption) caption.textContent = items[i].getAttribute("data-caption") || "";
  }

  function stepPhoto(viewer, delta) {
    showPhoto(viewer, Number(viewer.getAttribute("data-index") || 0) + delta);
  }

  document.addEventListener("click", function (event) {
    var target = event.target instanceof Element ? event.target : null;
    if (!target) return;

    var thumb = target.closest("[data-lightbox-index]");
    var group = thumb && thumb.closest("[data-lightbox-group]");
    var viewer =
      group &&
      document.getElementById(group.getAttribute("data-lightbox-group"));
    if (viewer) {
      event.preventDefault();
      showPhoto(viewer, Number(thumb.getAttribute("data-lightbox-index")));
      if (typeof viewer.showModal === "function" && !viewer.open)
        viewer.showModal();
      return;
    }
    var stepper = target.closest("[data-lightbox-step]");
    if (stepper) {
      var current = stepper.closest("dialog[data-lightbox]");
      if (current)
        stepPhoto(current, Number(stepper.getAttribute("data-lightbox-step")));
      return;
    }

    var opener = target.closest("[data-dialog-open]");
    if (opener) {
      event.preventDefault();
      openDialog(opener.getAttribute("data-dialog-open"));
      return;
    }
    var closer = target.closest("[data-dialog-close]");
    if (closer) {
      var dialog = closer.closest("dialog");
      if (dialog) dialog.close();
      return;
    }
    // A click on the <dialog> element itself landed on its backdrop.
    if (target.tagName === "DIALOG" && target.open) {
      target.close();
      return;
    }
    if (target.closest("[data-theme-cycle]")) {
      cycleTheme();
      return;
    }
    if (target.closest("[data-print]")) {
      window.print();
      return;
    }
    // Close open dropdowns/sheets (<details data-dismiss-outside>) on an
    // outside click.
    var open = document.querySelectorAll("details[data-dismiss-outside][open]");
    for (var i = 0; i < open.length; i++) {
      if (!open[i].contains(target)) open[i].removeAttribute("open");
    }
  });

  document.addEventListener("keydown", function (event) {
    if (event.key !== "Escape") return;
    var open = document.querySelectorAll("details[data-dismiss-outside][open]");
    for (var i = 0; i < open.length; i++) {
      open[i].removeAttribute("open");
      var summary = open[i].querySelector("summary");
      if (summary) summary.focus();
    }
  });

  document.addEventListener("toast", function (event) {
    showToast(event.detail);
  });

  // --- Listing photo uploads (admin): ask the portal for signed URLs, then
  // PUT each file straight to Cloud Storage. No image bytes go through the
  // portal itself.
  document.addEventListener("submit", function (event) {
    var form = event.target instanceof HTMLFormElement ? event.target : null;
    if (!form || !form.hasAttribute("data-upload")) return;
    event.preventDefault();
    var input = form.querySelector('input[type="file"]');
    var status = form.querySelector("[data-upload-status]");
    var say = function (text) {
      if (status) status.textContent = text;
    };
    var files =
      input && input.files ? Array.prototype.slice.call(input.files) : [];
    if (!files.length) {
      say("Choose at least one photo.");
      return;
    }
    say("Uploading " + files.length + " photo(s)…");
    fetch(form.getAttribute("data-upload"), {
      method: "POST",
      credentials: "same-origin",
      headers: {
        "Content-Type": "application/json",
        Accept: "application/json",
      },
      body: JSON.stringify({
        files: files.map(function (f) {
          return { content_type: f.type, size_bytes: f.size };
        }),
      }),
    })
      .then(function (res) {
        if (!res.ok) throw new Error("rejected");
        return res.json();
      })
      .then(function (signed) {
        return Promise.all(
          signed.map(function (s) {
            var file = files[Number(s.client_file_id)];
            // upload_headers are part of the signature (trace context), so send them too.
            var headers = Object.assign(
              { "Content-Type": file.type },
              s.upload_headers || {},
            );
            return fetch(s.upload_url, {
              method: "PUT",
              headers: headers,
              body: file,
            }).then(
              function (r) {
                return r.ok;
              },
              function () {
                return false;
              },
            );
          }),
        );
      })
      .then(function (results) {
        var ok = results.filter(Boolean).length;
        if (input) input.value = "";
        if (!ok) {
          say("Upload failed. Try again.");
          return;
        }
        say(
          (ok === results.length
            ? "Uploaded " + ok + " photo(s)."
            : "Uploaded " +
              ok +
              " of " +
              results.length +
              " photo(s). Try the others again.") + " Processing…",
        );
        waitForPhotos(ok, say);
      })
      .catch(function () {
        say(
          "Upload failed. Check the files (JPEG, PNG or WebP, under 15 MB) and try again.",
        );
      });
  });

  // Photos appear once image_worker has resized them: re-read the page's
  // photo slot every few seconds until the new ones are there.
  function waitForPhotos(added, say) {
    var slot = document.getElementById("listing-photos-slot");
    if (!slot) return;
    var target = Number(slot.getAttribute("data-photo-count") || 0) + added;
    var tries = 0;
    var poll = function () {
      tries += 1;
      fetch(location.href, {
        credentials: "same-origin",
        headers: { Accept: "text/html" },
      })
        .then(function (res) {
          return res.ok ? res.text() : "";
        })
        .then(
          function (html) {
            var fresh = new DOMParser()
              .parseFromString(html, "text/html")
              .getElementById("listing-photos-slot");
            var count = fresh
              ? Number(fresh.getAttribute("data-photo-count") || 0)
              : 0;
            if (fresh && count >= target) {
              slot.replaceWith(fresh);
              say("Photos ready.");
            } else if (tries < 30) {
              setTimeout(poll, 3000);
            } else {
              if (fresh) slot.replaceWith(fresh);
              say(
                "Still processing. Reload the page in a minute to see the rest.",
              );
            }
          },
          function () {
            if (tries < 30) setTimeout(poll, 3000);
          },
        );
    };
    setTimeout(poll, 2000);
  }

  // Clear a form after htmx submits it successfully (e.g. the message box).
  document.addEventListener("htmx:afterRequest", function (event) {
    var form = event.target instanceof HTMLFormElement ? event.target : null;
    if (
      form &&
      form.hasAttribute("data-reset-on-success") &&
      event.detail &&
      event.detail.successful
    ) {
      form.reset();
    }
  });

  // --- Broken images: hide the <img> so the placeholder tile behind it
  // shows instead of the browser's broken-image icon and alt text. Error
  // events do not bubble, so listen in the capture phase; images that failed
  // before this script ran are caught by the scan below.
  function markBroken(img) {
    img.setAttribute("data-broken", "");
  }
  document.addEventListener(
    "error",
    function (event) {
      if (event.target instanceof HTMLImageElement) markBroken(event.target);
    },
    true,
  );
  var settled = document.images;
  for (var b = 0; b < settled.length; b++) {
    if (
      settled[b].complete &&
      settled[b].naturalWidth === 0 &&
      settled[b].getAttribute("src")
    )
      markBroken(settled[b]);
  }

  // --- Stay-dates calendar (server-rendered by date_range_calendar). -------
  // An unavailable night can still be the check-out day: the guest leaves that
  // morning. A range may not include any unavailable or past night.
  var DAY_MS = 86400000;

  function parseDay(iso) {
    return iso ? Date.parse(iso + "T00:00:00Z") : Number.NaN;
  }
  function isoDay(ms) {
    return new Date(ms).toISOString().slice(0, 10);
  }

  function calendarParts(root) {
    return {
      checkIn: root.querySelector('input[name="check_in"]'),
      checkOut: root.querySelector('input[name="check_out"]'),
      status: root.querySelector("[data-calendar-status]"),
      buttons: root.querySelectorAll("button[data-date]"),
    };
  }

  function dayButton(root, iso) {
    return root.querySelector('button[data-date="' + iso + '"]');
  }

  // The second month is hidden when the calendar is narrow; its buttons
  // still carry availability but cannot take focus.
  function isShown(el) {
    return el.getClientRects().length > 0;
  }

  function rangeIsFree(root, startIso, endIso) {
    for (var t = parseDay(startIso); t < parseDay(endIso); t += DAY_MS) {
      var btn = dayButton(root, isoDay(t));
      // Nights outside the rendered months are re-checked by the server.
      if (btn && btn.getAttribute("data-state") !== "available") return false;
    }
    return true;
  }

  function paintCalendar(root) {
    var p = calendarParts(root);
    var start = p.checkIn ? p.checkIn.value : "";
    var end = p.checkOut ? p.checkOut.value : "";
    for (var i = 0; i < p.buttons.length; i++) {
      var b = p.buttons[i];
      var iso = b.getAttribute("data-date");
      var t = parseDay(iso);
      var isEnd = iso === start || iso === end;
      b.toggleAttribute("data-selected", isEnd);
      b.toggleAttribute(
        "data-in-range",
        !!(start && end && t > parseDay(start) && t < parseDay(end)),
      );
      if (b.parentElement)
        b.parentElement.setAttribute("aria-selected", isEnd ? "true" : "false");
    }
  }

  function announce(root, text) {
    var status = calendarParts(root).status;
    if (status) status.textContent = text;
  }

  function setRoving(root, button) {
    var buttons = calendarParts(root).buttons;
    for (var i = 0; i < buttons.length; i++)
      buttons[i].setAttribute("tabindex", "-1");
    if (button) button.setAttribute("tabindex", "0");
  }

  function enhanceCalendar(root) {
    root.setAttribute("data-enhanced", "");
    var p = calendarParts(root);
    var focusIso = (p.checkIn && p.checkIn.value) || "";
    var shown = Array.prototype.filter.call(p.buttons, isShown);
    var picked = focusIso && dayButton(root, focusIso);
    var first =
      (picked && isShown(picked) ? picked : null) ||
      shown.filter(function (b) {
        return b.getAttribute("data-state") === "available";
      })[0] ||
      shown[0] ||
      p.buttons[0];
    setRoving(root, first);
    paintCalendar(root);
  }

  function enhanceCalendars(scope) {
    var roots = (scope || document).querySelectorAll("[data-calendar]");
    for (var i = 0; i < roots.length; i++) enhanceCalendar(roots[i]);
  }

  function pickDay(root, button) {
    var p = calendarParts(root);
    if (!p.checkIn || !p.checkOut) return;
    var iso = button.getAttribute("data-date");
    var state = button.getAttribute("data-state");
    var start = p.checkIn.value;
    var choosingEnd =
      start && !p.checkOut.value && parseDay(iso) > parseDay(start);

    if (choosingEnd) {
      if (state === "past" || !rangeIsFree(root, start, iso)) {
        announce(
          root,
          "Those dates include unavailable nights. Choose an earlier check-out.",
        );
        return;
      }
      p.checkOut.value = iso;
      announce(root, "Check-out " + iso + " selected.");
    } else {
      if (state !== "available") {
        announce(root, "That night is not available.");
        return;
      }
      p.checkIn.value = iso;
      p.checkOut.value = "";
      announce(root, "Check-in " + iso + " selected. Choose a check-out date.");
    }
    root.setAttribute("data-check-in", p.checkIn.value);
    root.setAttribute("data-check-out", p.checkOut.value);
    setRoving(root, button);
    paintCalendar(root);
    root.dispatchEvent(new Event("change", { bubbles: true }));
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

  document.addEventListener("click", function (event) {
    var target = event.target instanceof Element ? event.target : null;
    var button = target && target.closest("[data-calendar] button[data-date]");
    if (!button) return;
    pickDay(button.closest("[data-calendar]"), button);
  });

  document.addEventListener("keydown", function (event) {
    var target = event.target instanceof Element ? event.target : null;
    var button = target && target.closest("[data-calendar] button[data-date]");
    if (!button) return;
    var root = button.closest("[data-calendar]");
    var iso = button.getAttribute("data-date");
    var t = new Date(parseDay(iso));
    var step = { ArrowLeft: -1, ArrowRight: 1, ArrowUp: -7, ArrowDown: 7 }[
      event.key
    ];
    if (event.key === "Home") step = -t.getUTCDay();
    if (event.key === "End") step = 6 - t.getUTCDay();
    if (event.key === "PageUp" || event.key === "PageDown") {
      var forward = event.key === "PageDown";
      var other = new Date(t.getTime());
      other.setUTCMonth(t.getUTCMonth() + (forward ? 1 : -1));
      step = Math.round((other.getTime() - t.getTime()) / DAY_MS);
    }
    if (step === undefined) return;
    event.preventDefault();
    if (!moveFocus(root, iso, step)) {
      // Outside the shown months: ask the server for the next/previous page.
      var nav = root.querySelector(
        step > 0
          ? '[aria-label="Next month"]'
          : '[aria-label="Previous month"]',
      );
      if (nav && !nav.disabled) nav.click();
    }
  });

  // Typed dates repaint the grid (the change event still bubbles to
  // #booking-dates for htmx).
  document.addEventListener("change", function (event) {
    var input = event.target instanceof Element ? event.target : null;
    var root =
      input &&
      input.matches(
        '[data-calendar] input[name="check_in"], [data-calendar] input[name="check_out"]',
      ) &&
      input.closest("[data-calendar]");
    if (!root) return;
    var p = calendarParts(root);
    root.setAttribute("data-check-in", p.checkIn ? p.checkIn.value : "");
    root.setAttribute("data-check-out", p.checkOut ? p.checkOut.value : "");
    paintCalendar(root);
  });

  // Prev/next month URLs were built when the calendar was rendered, so carry
  // the dates picked since then; otherwise a check-in picked before paging is
  // lost and the next pick starts a new stay instead of ending this one.
  document.addEventListener("htmx:configRequest", function (event) {
    var elt = event.detail?.elt;
    if (
      !(elt instanceof Element) ||
      !elt.matches("[data-calendar] [data-calendar-nav]")
    )
      return;
    var p = calendarParts(elt.closest("[data-calendar]"));
    var url = new URL(event.detail.path, window.location.origin);
    [
      ["check_in", p.checkIn],
      ["check_out", p.checkOut],
    ].forEach(function (pair) {
      var value = pair[1] ? pair[1].value : "";
      if (value) url.searchParams.set(pair[0], value);
      else url.searchParams.delete(pair[0]);
    });
    event.detail.path = url.pathname + url.search;
  });

  // Re-enhance calendars that htmx swaps in (prev/next month), and open
  // dialogs that arrive with data-auto-open (e.g. the cancel confirmation).
  document.addEventListener("htmx:load", function (event) {
    var el = event.target instanceof Element ? event.target : null;
    if (!el) return;
    if (el.matches("[data-calendar]")) enhanceCalendar(el);
    else enhanceCalendars(el);
    convertDateInputs(el);
    var dialogs = el.matches("dialog[data-auto-open]")
      ? [el]
      : el.querySelectorAll("dialog[data-auto-open]");
    for (var i = 0; i < dialogs.length; i++) {
      if (!dialogs[i].open && typeof dialogs[i].showModal === "function")
        dialogs[i].showModal();
    }
    autoDismissToasts(el);
  });

  document.addEventListener("keydown", function (event) {
    if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
    var viewer = document.querySelector("dialog[data-lightbox][open]");
    if (!viewer) return;
    event.preventDefault();
    stepPhoto(viewer, event.key === "ArrowLeft" ? -1 : 1);
  });

  // --- Character counters: <p data-char-count="<field id>"> shows
  // "used / maxlength" as the user types (the server renders the first one).
  document.addEventListener("input", function (event) {
    var field = event.target instanceof Element ? event.target : null;
    if (!field || !field.id || !field.hasAttribute("maxlength")) return;
    var counter = document.querySelector(
      '[data-char-count="' + field.id + '"]',
    );
    if (counter)
      counter.textContent =
        field.value.length + " / " + field.getAttribute("maxlength");
  });

  // --- Carousels: [data-carousel] holds a scrolling [data-carousel-track]
  // and [data-carousel-step="-1"|"1"] arrows (server-rendered hidden). The
  // arrows show only when the track overflows, scroll one card per click,
  // and are disabled at either end.
  var reduceMotion =
    window.matchMedia && window.matchMedia("(prefers-reduced-motion: reduce)");

  function carouselParts(root) {
    return {
      track: root.querySelector("[data-carousel-track]"),
      arrows: root.querySelectorAll("[data-carousel-step]"),
    };
  }

  function updateCarousel(root) {
    var p = carouselParts(root);
    if (!p.track) return;
    var max = p.track.scrollWidth - p.track.clientWidth;
    var overflows = max > 1;
    for (var i = 0; i < p.arrows.length; i++) {
      var back = Number(p.arrows[i].getAttribute("data-carousel-step")) < 0;
      p.arrows[i].hidden = !overflows;
      p.arrows[i].disabled = back
        ? p.track.scrollLeft <= 1
        : p.track.scrollLeft >= max - 1;
    }
  }

  function updateCarousels() {
    var roots = document.querySelectorAll("[data-carousel]");
    for (var i = 0; i < roots.length; i++) updateCarousel(roots[i]);
  }

  function stepCarousel(root, dir) {
    var track = carouselParts(root).track;
    if (!track) return;
    var card = track.firstElementChild;
    var gap = Number.parseFloat(window.getComputedStyle(track).columnGap) || 0;
    var distance = card
      ? card.getBoundingClientRect().width + gap
      : track.clientWidth;
    track.scrollBy({
      left: dir * distance,
      behavior: reduceMotion && reduceMotion.matches ? "auto" : "smooth",
    });
  }

  document.addEventListener("click", function (event) {
    var target = event.target instanceof Element ? event.target : null;
    var arrow = target && target.closest("[data-carousel-step]");
    var root = arrow && arrow.closest("[data-carousel]");
    if (root)
      stepCarousel(root, Number(arrow.getAttribute("data-carousel-step")));
  });

  // Scroll events do not bubble; catch track scrolls in the capture phase.
  // Measure at most once per frame per carousel.
  var pendingCarousels = [];
  document.addEventListener(
    "scroll",
    function (event) {
      var track = event.target instanceof Element ? event.target : null;
      var root =
        track &&
        track.matches("[data-carousel-track]") &&
        track.closest("[data-carousel]");
      if (!root || pendingCarousels.indexOf(root) !== -1) return;
      pendingCarousels.push(root);
      if (pendingCarousels.length > 1) return;
      window.requestAnimationFrame(function () {
        var roots = pendingCarousels;
        pendingCarousels = [];
        for (var i = 0; i < roots.length; i++) updateCarousel(roots[i]);
      });
    },
    true,
  );
  window.addEventListener("resize", updateCarousels);
  // Lazy photos can change card widths after first paint.
  window.addEventListener("load", updateCarousels);

  // Copy buttons: <button data-copy-target="#input-id">. Strict CSP, so no
  // inline handlers; the button text confirms the copy.
  document.addEventListener("click", function (event) {
    var target = event.target instanceof Element ? event.target : null;
    var button = target && target.closest("[data-copy-target]");
    if (!button) return;
    var source = document.querySelector(
      button.getAttribute("data-copy-target"),
    );
    if (!source || !navigator.clipboard) return;
    var text = "value" in source ? source.value : source.textContent;
    navigator.clipboard.writeText(text).then(function () {
      var label = button.getAttribute("data-copied-label") || "Copied";
      var original = button.textContent;
      button.textContent = label;
      window.setTimeout(function () {
        button.textContent = original;
      }, 2000);
    });
  });

  // --- Calendar date picker popup for [data-date-picker] inputs. -----------
  var dpPopup = null;
  var dpCurrentInput = null;
  var dpViewYear = 0;
  var dpViewMonth = 0; // 0-11

  var MONTH_NAMES = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
  ];
  var DAY_NAMES = ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"];

  function padZero(n) {
    return (n < 10 ? "0" : "") + n;
  }

  function formatIso(year, monthIndex, day) {
    return year + "-" + padZero(monthIndex + 1) + "-" + padZero(day);
  }

  function parseIsoDate(val) {
    if (!val || typeof val !== "string") return null;
    var m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(val.trim());
    if (!m) return null;
    var y = Number(m[1]),
      mo = Number(m[2]) - 1,
      d = Number(m[3]);
    var dt = new Date(y, mo, d);
    if (dt.getFullYear() === y && dt.getMonth() === mo && dt.getDate() === d) {
      return { year: y, month: mo, day: d };
    }
    return null;
  }

  function createDatePickerPopup() {
    if (dpPopup) return dpPopup;
    dpPopup = document.createElement("div");
    dpPopup.className = "op-datepicker-popup";
    dpPopup.setAttribute("role", "dialog");
    dpPopup.setAttribute("aria-label", "Date picker");
    document.body.appendChild(dpPopup);
    return dpPopup;
  }

  function positionDatePicker(input) {
    var rect = input.getBoundingClientRect();
    var popupHeight = 280;
    var top = rect.bottom + window.scrollY + 4;
    if (
      rect.bottom + popupHeight > window.innerHeight &&
      rect.top - popupHeight > 0
    ) {
      top = rect.top + window.scrollY - popupHeight - 4;
    }
    var left = rect.left + window.scrollX;
    var width = 280;
    if (left + width > window.innerWidth - 12) {
      left = Math.max(12, window.innerWidth - width - 12);
    }
    dpPopup.style.top = top + "px";
    dpPopup.style.left = left + "px";
  }

  function renderDatePicker() {
    if (!dpPopup || !dpCurrentInput) return;
    dpPopup.innerHTML = "";

    var header = document.createElement("div");
    header.className = "op-dp-header";

    var prevBtn = document.createElement("button");
    prevBtn.type = "button";
    prevBtn.className = "op-dp-nav";
    prevBtn.setAttribute("aria-label", "Previous month");
    prevBtn.textContent = "‹";
    prevBtn.addEventListener("click", function (e) {
      e.preventDefault();
      e.stopPropagation();
      dpViewMonth -= 1;
      if (dpViewMonth < 0) {
        dpViewMonth = 11;
        dpViewYear -= 1;
      }
      renderDatePicker();
    });

    var title = document.createElement("span");
    title.className = "op-dp-title";
    title.textContent = MONTH_NAMES[dpViewMonth] + " " + dpViewYear;

    var nextBtn = document.createElement("button");
    nextBtn.type = "button";
    nextBtn.className = "op-dp-nav";
    nextBtn.setAttribute("aria-label", "Next month");
    nextBtn.textContent = "›";
    nextBtn.addEventListener("click", function (e) {
      e.preventDefault();
      e.stopPropagation();
      dpViewMonth += 1;
      if (dpViewMonth > 11) {
        dpViewMonth = 0;
        dpViewYear += 1;
      }
      renderDatePicker();
    });

    header.appendChild(prevBtn);
    header.appendChild(title);
    header.appendChild(nextBtn);
    dpPopup.appendChild(header);

    var grid = document.createElement("div");
    grid.className = "op-dp-grid";

    for (var i = 0; i < DAY_NAMES.length; i++) {
      var dh = document.createElement("div");
      dh.className = "op-dp-dayhead";
      dh.textContent = DAY_NAMES[i];
      grid.appendChild(dh);
    }

    var firstDayOfWeek = new Date(dpViewYear, dpViewMonth, 1).getDay();
    var daysInMonth = new Date(dpViewYear, dpViewMonth + 1, 0).getDate();

    var minVal = dpCurrentInput.getAttribute("min") || "";
    var maxVal = dpCurrentInput.getAttribute("max") || "";
    var currVal = parseIsoDate(dpCurrentInput.value);
    var now = new Date();
    var todayIso = formatIso(now.getFullYear(), now.getMonth(), now.getDate());

    for (var b = 0; b < firstDayOfWeek; b++) {
      var blank = document.createElement("div");
      grid.appendChild(blank);
    }

    for (var d = 1; d <= daysInMonth; d++) {
      var dayBtn = document.createElement("button");
      dayBtn.type = "button";
      dayBtn.className = "op-dp-day";
      dayBtn.textContent = String(d);
      var iso = formatIso(dpViewYear, dpViewMonth, d);

      if (
        currVal &&
        currVal.year === dpViewYear &&
        currVal.month === dpViewMonth &&
        currVal.day === d
      ) {
        dayBtn.classList.add("op-dp-selected");
      }
      if (iso === todayIso) {
        dayBtn.classList.add("op-dp-today");
      }
      if ((minVal && iso < minVal) || (maxVal && iso > maxVal)) {
        dayBtn.disabled = true;
      } else {
        (function (dateString) {
          dayBtn.addEventListener("click", function (e) {
            e.preventDefault();
            e.stopPropagation();
            if (dpCurrentInput) {
              dpCurrentInput.value = dateString;
              dpCurrentInput.dispatchEvent(
                new Event("input", { bubbles: true }),
              );
              dpCurrentInput.dispatchEvent(
                new Event("change", { bubbles: true }),
              );
            }
            closeDatePicker();
          });
        })(iso);
      }
      grid.appendChild(dayBtn);
    }

    dpPopup.appendChild(grid);
  }

  function openDatePicker(input) {
    if (input.closest("[data-calendar]")) return;
    createDatePickerPopup();
    dpCurrentInput = input;
    var parsed = parseIsoDate(input.value);
    var now = new Date();
    if (parsed) {
      dpViewYear = parsed.year;
      dpViewMonth = parsed.month;
    } else {
      var minParsed = parseIsoDate(input.getAttribute("min"));
      if (
        minParsed &&
        (now.getFullYear() < minParsed.year ||
          (now.getFullYear() === minParsed.year &&
            now.getMonth() < minParsed.month))
      ) {
        dpViewYear = minParsed.year;
        dpViewMonth = minParsed.month;
      } else {
        dpViewYear = now.getFullYear();
        dpViewMonth = now.getMonth();
      }
    }
    positionDatePicker(input);
    renderDatePicker();
    dpPopup.classList.add("op-open");
  }

  function closeDatePicker() {
    if (dpPopup) {
      dpPopup.classList.remove("op-open");
    }
    dpCurrentInput = null;
  }

  // Convert any native date inputs to text with data-date-picker for consistent YYYY-MM-DD display
  function convertDateInputs(scope) {
    var inputs = (scope || document).querySelectorAll('input[type="date"]');
    for (var i = 0; i < inputs.length; i++) {
      var inp = inputs[i];
      if (inp.closest("[data-calendar]")) continue;
      inp.type = "text";
      inp.setAttribute("data-date-picker", "");
      inp.setAttribute("placeholder", "yyyy-mm-dd");
      inp.setAttribute("pattern", "\\d{4}-\\d{2}-\\d{2}");
      inp.setAttribute("autocomplete", "off");
    }
  }

  document.addEventListener("click", function (event) {
    var target = event.target instanceof Element ? event.target : null;
    if (!target) return;
    if (target.matches("input[data-date-picker]")) {
      openDatePicker(target);
      return;
    }
    if (dpPopup && dpPopup.classList.contains("op-open")) {
      if (!dpPopup.contains(target) && target !== dpCurrentInput) {
        closeDatePicker();
      }
    }
  });

  document.addEventListener("keydown", function (event) {
    if (
      event.key === "Escape" &&
      dpPopup &&
      dpPopup.classList.contains("op-open")
    ) {
      closeDatePicker();
    }
  });

  window.addEventListener("resize", function () {
    if (dpPopup && dpPopup.classList.contains("op-open") && dpCurrentInput) {
      positionDatePicker(dpCurrentInput);
    }
  });
  document.addEventListener(
    "scroll",
    function () {
      if (dpPopup && dpPopup.classList.contains("op-open") && dpCurrentInput) {
        positionDatePicker(dpCurrentInput);
      }
    },
    true,
  );

  // ----- Listing page: mark the section in view in the sticky section nav.
  function watchSectionNav() {
    var nav = document.querySelector("[data-section-nav]");
    if (!nav || !("IntersectionObserver" in window)) return;
    var links = Array.prototype.slice.call(nav.querySelectorAll("[data-section-link]"));
    var sections = links
      .map(function (a) { return document.getElementById(a.getAttribute("data-section-link")); })
      .filter(Boolean);
    if (!sections.length) return;
    var visible = {};
    function mark() {
      var current = null;
      sections.forEach(function (s) { if (!current && visible[s.id]) current = s.id; });
      links.forEach(function (a) {
        if (current && a.getAttribute("data-section-link") === current) {
          a.setAttribute("aria-current", "true");
        } else {
          a.removeAttribute("aria-current");
        }
      });
    }
    var observer = new IntersectionObserver(
      function (entries) {
        entries.forEach(function (e) { visible[e.target.id] = e.isIntersecting; });
        mark();
      },
      { rootMargin: "-80px 0px -55% 0px" },
    );
    sections.forEach(function (s) { observer.observe(s); });
  }

  // ----- Listing studio: reorder fixed form rows (ribbon items, amenity
  // groups). Field names carry the row number, so moving a row swaps the
  // values of two rows; without JS the host orders rows by slot.
  function rowFields(row) {
    return Array.prototype.slice.call(row.querySelectorAll("input, select, textarea"));
  }

  function swapRows(a, b) {
    var fa = rowFields(a);
    var fb = rowFields(b);
    fa.forEach(function (field, i) {
      var other = fb[i];
      if (!other) return;
      var v = field.value;
      field.value = other.value;
      other.value = v;
    });
  }

  function addReorderButtons(editor, rowSelector) {
    var rows = Array.prototype.slice.call(editor.querySelectorAll(rowSelector));
    rows.forEach(function (row, i) {
      if (row.querySelector("[data-row-move]")) return;
      var bar = document.createElement("div");
      bar.className = "flex justify-end gap-1";
      [["up", -1, "\u2191"], ["down", 1, "\u2193"]].forEach(function (spec) {
        var target = i + spec[1];
        var button = document.createElement("button");
        button.type = "button";
        button.setAttribute("data-row-move", spec[0]);
        button.setAttribute("aria-label", "Move row " + (i + 1) + " " + spec[0]);
        button.className = "size-8 rounded-md border border-border text-sm disabled:opacity-40";
        button.textContent = spec[2];
        button.disabled = target < 0 || target >= rows.length;
        button.addEventListener("click", function () {
          swapRows(row, rows[target]);
          var moved = rowFields(rows[target])[0];
          if (moved) moved.focus();
          editor.dispatchEvent(new Event("input", { bubbles: true }));
        });
        bar.appendChild(button);
      });
      row.appendChild(bar);
    });
  }

  // "Prefill from listing facts": bedrooms, bathrooms and maximum guests go
  // first in the ribbon; the host's other items keep their order after them.
  function prefillRibbon(editor) {
    var form = editor.closest("form");
    if (!form) return;
    function num(name) {
      var field = form.querySelector('[name="' + name + '"]');
      var n = field ? parseInt(field.value, 10) : NaN;
      return isNaN(n) ? 0 : Math.max(0, n);
    }
    var baths = num("full_bathrooms") + num("half_bathrooms") / 2;
    var facts = [
      ["bed", String(num("bedrooms")), "Bedrooms"],
      ["bath", String(baths), "Bathrooms"],
      ["guests", String(num("max_guests")), "Max guests"],
    ];
    var rows = Array.prototype.slice.call(editor.querySelectorAll("[data-ribbon-row]"));
    var others = rows
      .map(function (row) { return rowFields(row).map(function (f) { return f.value; }); })
      .filter(function (v) {
        var filled = (v[1] || "").trim() || (v[2] || "").trim();
        return filled && ["bed", "bath", "guests"].indexOf(v[0]) === -1;
      });
    var values = facts.concat(others).slice(0, rows.length);
    rows.forEach(function (row, i) {
      var fields = rowFields(row);
      var v = values[i] || [fields[0] ? fields[0].options[0].value : "", "", ""];
      fields.forEach(function (field, j) { field.value = v[j] || ""; });
    });
  }

  function enhanceListingStudio() {
    var ribbon = document.querySelector("[data-ribbon-editor]");
    if (ribbon) {
      addReorderButtons(ribbon, "[data-ribbon-row]");
      if (!ribbon.querySelector("[data-ribbon-prefill]")) {
        var prefill = document.createElement("button");
        prefill.type = "button";
        prefill.setAttribute("data-ribbon-prefill", "");
        prefill.className = "h-9 w-fit rounded-lg border border-border px-3 text-sm";
        prefill.textContent = "Prefill from listing facts";
        prefill.addEventListener("click", function () { prefillRibbon(ribbon); });
        ribbon.appendChild(prefill);
      }
    }
    var amenities = document.querySelector("[data-amenity-editor]");
    if (amenities) addReorderButtons(amenities, "[data-amenity-slot]");
  }

  function init() {
    applyTheme(currentTheme());
    enhanceCalendars(document);
    convertDateInputs(document);
    updateCarousels();
    storeTimezoneOffset();
    watchSectionNav();
    enhanceListingStudio();
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", init);
  } else {
    init();
  }

  window.OurPlacesUi = { showToast: showToast, cycleTheme: cycleTheme };
})();

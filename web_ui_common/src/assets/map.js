// map.js — sets up the maps in [data-map] containers (spec 103 §1.11).
// Loaded by ui.js, after pmtiles.js, only once a container is about to be
// shown. MapLibre is an ES module and comes in with import(); everything is
// configured from the container's data-* attributes. No HTML is built from
// strings: the only markup MapLibre renders is the fixed attribution below.
(function () {
  "use strict";

  // [[west, south], [east, north]]: common::location::JAMAICA_BOUNDS.
  var JAMAICA_BOUNDS = [
    [-78.6, 17.55],
    [-75.95, 18.7],
  ];
  // common::location::APPROX_MAX_M and EARTH_RADIUS_M.
  var APPROX_MAX_M = 500;
  var METRES_PER_DEGREE = (6371000 * Math.PI) / 180;
  var CIRCLE_POINTS = 64;
  var LOAD_TIMEOUT_MS = 10000;
  var MAX_ZOOM = 17;
  var ZOOM = { approximate: 13, exact: 15 };
  var SEARCH = { padding: 48, maxZoom: 13, singleZoom: 12 };
  var NOT_MAPPED = "These listings aren't on the map yet.";
  var AREA_COLOR = "#0d9488";
  var ATTRIBUTION = [
    '<a href="https://www.openstreetmap.org/copyright" target="_blank" rel="noopener">© OpenStreetMap contributors</a>',
    '<a href="https://protomaps.com" target="_blank" rel="noopener">Protomaps</a>',
  ];
  var SUPPORTED_KINDS = { listing: true, booking: true, search: true };

  var library = null; // Promise of the MapLibre module, shared by all maps.
  var live = []; // Containers with a map, for theme changes.

  function loadLibrary(el) {
    if (!library) {
      library = import(el.getAttribute("data-map-js")).then(function (maplibregl) {
        maplibregl.setWorkerUrl(
          new URL(el.getAttribute("data-map-worker"), window.location.href).href
        );
        var protocol = new window.pmtiles.Protocol();
        maplibregl.addProtocol("pmtiles", protocol.tile);
        return maplibregl;
      });
      // A failed import must not stick: the next container may retry.
      library.catch(function () {
        library = null;
      });
    }
    return library;
  }

  function hasWebGL() {
    try {
      var canvas = document.createElement("canvas");
      return !!(canvas.getContext("webgl2") || canvas.getContext("webgl"));
    } catch (e) {
      return false;
    }
  }

  function isDark() {
    var cls = document.documentElement.classList;
    return cls.contains("dark") || cls.contains("dim");
  }

  function styleUrl(el) {
    return (
      el.getAttribute("data-tiles-base") +
      (isDark() ? "/style-dark.json" : "/style-light.json")
    );
  }

  function point(el) {
    var lat = parseFloat(el.getAttribute("data-lat"));
    var lng = parseFloat(el.getAttribute("data-lng"));
    if (!isFinite(lat) || !isFinite(lng)) return null;
    return [lng, lat];
  }

  function precision(el) {
    if (el.getAttribute("data-map") === "booking") return "exact";
    return el.getAttribute("data-precision") === "exact" ? "exact" : "approximate";
  }

  // A 64-point polygon of radius APPROX_MAX_M around [lng, lat].
  function circle(center) {
    var dLat = APPROX_MAX_M / METRES_PER_DEGREE;
    var dLng = dLat / Math.cos((center[1] * Math.PI) / 180);
    var ring = [];
    for (var i = 0; i <= CIRCLE_POINTS; i++) {
      var a = (2 * Math.PI * i) / CIRCLE_POINTS;
      ring.push([center[0] + dLng * Math.cos(a), center[1] + dLat * Math.sin(a)]);
    }
    return {
      type: "Feature",
      properties: {},
      geometry: { type: "Polygon", coordinates: [ring] },
    };
  }

  // Sources and layers go away with setStyle, so this runs on every
  // style.load. DOM markers survive setStyle and are added once.
  function addAreas(map, centers) {
    if (!centers.length || map.getSource("op-area")) return;
    map.addSource("op-area", {
      type: "geojson",
      data: { type: "FeatureCollection", features: centers.map(circle) },
    });
    map.addLayer({
      id: "op-area-fill",
      type: "fill",
      source: "op-area",
      paint: { "fill-color": AREA_COLOR, "fill-opacity": 0.18 },
    });
    map.addLayer({
      id: "op-area-line",
      type: "line",
      source: "op-area",
      paint: { "line-color": AREA_COLOR, "line-width": 2 },
    });
  }

  // --- The search map (spec 103 §1.6): one marker per result card, read
  // from the cards on the page, so the map shows exactly what the list does.
  function cardsFor(el) {
    var scope = el.closest("[data-results]") || document;
    var out = [];
    var cards = scope.querySelectorAll("[data-map-lat]");
    for (var i = 0; i < cards.length; i++) {
      var card = cards[i];
      var lat = parseFloat(card.getAttribute("data-map-lat"));
      var lng = parseFloat(card.getAttribute("data-map-lng"));
      if (!isFinite(lat) || !isFinite(lng)) continue;
      out.push({
        center: [lng, lat],
        exact: card.getAttribute("data-map-precision") === "exact",
        price: card.getAttribute("data-map-price") || "",
        name: card.getAttribute("data-map-name") || "",
        template: card.querySelector("template[data-map-card]"),
      });
    }
    return out;
  }

  // A price badge: a real <button>, at least 44x44, reachable with Tab.
  // Styles go through the CSSOM, which the CSP allows (no style attribute
  // in markup).
  function badge(card) {
    var b = document.createElement("button");
    b.type = "button";
    b.className = "op-map-badge";
    b.textContent = card.price;
    b.setAttribute("aria-label", card.name + ", " + card.price);
    b.setAttribute("aria-haspopup", "dialog");
    var st = b.style;
    st.minWidth = "44px";
    st.minHeight = "44px";
    st.padding = "0 12px";
    st.borderRadius = "9999px";
    st.border = "1px solid rgba(0,0,0,0.15)";
    st.background = "#ffffff";
    st.color = "#111827";
    st.font = "600 13px/1 system-ui, sans-serif";
    st.boxShadow = "0 1px 4px rgba(0,0,0,0.25)";
    st.cursor = "pointer";
    st.whiteSpace = "nowrap";
    return b;
  }

  // The marker element around a badge. MapLibre gives a marker's element
  // role="img", a label and a tabindex, so they go on this wrapper (and are
  // cleared) rather than on the button. An exact point gets a tail: the
  // badge on a pin.
  function markerElement(button, exact) {
    var wrap = document.createElement("div");
    wrap.style.display = "flex";
    wrap.style.flexDirection = "column";
    wrap.style.alignItems = "center";
    wrap.appendChild(button);
    if (exact) {
      var tail = document.createElement("span");
      tail.setAttribute("aria-hidden", "true");
      tail.style.width = "2px";
      tail.style.height = "12px";
      tail.style.background = AREA_COLOR;
      wrap.appendChild(tail);
    }
    return wrap;
  }

  function overlay(el, text) {
    var p = document.createElement("p");
    p.setAttribute("data-map-overlay", "");
    p.textContent = text;
    var st = p.style;
    st.position = "absolute";
    st.left = "50%";
    st.top = "12px";
    st.transform = "translateX(-50%)";
    st.margin = "0";
    st.padding = "8px 14px";
    st.borderRadius = "9999px";
    st.background = "rgba(255,255,255,0.92)";
    st.color = "#111827";
    st.font = "13px/1.3 system-ui, sans-serif";
    st.zIndex = "2";
    el.appendChild(p);
  }

  // Badges, popups and the view. Markers are DOM and survive setStyle, so
  // they are added once; only the circles come back on style.load.
  function addSearchMarkers(maplibregl, map, el, cards) {
    var open = null;
    function close(restore) {
      if (!open) return;
      var o = open;
      open = null;
      o.popup.remove();
      if (restore) o.button.focus();
    }
    cards.forEach(function (card) {
      var button = badge(card);
      var marker = new maplibregl.Marker({
        element: markerElement(button, card.exact),
        anchor: card.exact ? "bottom" : "center",
      })
        .setLngLat(card.center)
        .addTo(map);
      var wrap = marker.getElement();
      wrap.removeAttribute("tabindex");
      wrap.removeAttribute("role");
      wrap.removeAttribute("aria-label");
      wrap.setAttribute("data-map-badge", "");
      button.addEventListener("click", function (e) {
        e.stopPropagation();
        if (open && open.button === button) return close(false);
        close(false);
        if (!card.template) return;
        var popup = new maplibregl.Popup({ offset: card.exact ? 40 : 24, maxWidth: "300px" })
          .setLngLat(card.center)
          .setDOMContent(card.template.content.cloneNode(true))
          .addTo(map);
        var entry = { popup: popup, button: button };
        open = entry;
        popup.on("close", function () {
          if (open === entry) open = null;
        });
        popup.getElement().addEventListener("keydown", function (e) {
          if (e.key === "Escape") {
            e.stopPropagation();
            close(true);
          }
        });
      });
      button.addEventListener("keydown", function (e) {
        if (e.key === "Escape" && open && open.button === button) {
          e.stopPropagation();
          close(true);
        }
      });
    });
  }

  function searchView(cards) {
    if (cards.length === 0) return { bounds: JAMAICA_BOUNDS };
    if (cards.length === 1) return { center: cards[0].center, zoom: SEARCH.singleZoom };
    var w = Infinity, s = Infinity, e = -Infinity, n = -Infinity;
    cards.forEach(function (c) {
      w = Math.min(w, c.center[0]);
      e = Math.max(e, c.center[0]);
      s = Math.min(s, c.center[1]);
      n = Math.max(n, c.center[1]);
    });
    return {
      bounds: [[w, s], [e, n]],
      fitBoundsOptions: { padding: SEARCH.padding, maxZoom: SEARCH.maxZoom },
    };
  }

  function setState(el, state) {
    el.setAttribute("data-map-state", state);
  }

  function showFallback(el) {
    var canvas = el.querySelector("[data-map-canvas]");
    var fallback = el.querySelector("[data-map-fallback]");
    if (canvas) canvas.hidden = true;
    if (fallback) fallback.hidden = false;
  }

  function teardown(el) {
    var map = el.__opMap;
    el.__opMap = null;
    if (el.__opTimer) {
      clearTimeout(el.__opTimer);
      el.__opTimer = null;
    }
    var i = live.indexOf(el);
    if (i >= 0) live.splice(i, 1);
    var note = el.querySelector("[data-map-overlay]");
    if (note) note.remove();
    if (map) {
      try {
        map.remove();
      } catch (e) {
        /* already gone */
      }
    }
  }

  // "Map unavailable": the page around the map keeps working. One warning,
  // nothing sent to the server.
  function fail(el, reason) {
    if (el.getAttribute("data-map-state") === "failed") return;
    el.__opGen = (el.__opGen || 0) + 1;
    teardown(el);
    setState(el, "failed");
    showFallback(el);
    console.warn("Map unavailable:", reason);
  }

  function init(el) {
    var state = el.getAttribute("data-map-state");
    if (el.__opMap || state === "loading" || state === "failed") return;
    var kind = el.getAttribute("data-map");
    if (!SUPPORTED_KINDS[kind]) return fail(el, "unsupported map kind " + kind);
    var search = kind === "search";
    var center = search ? null : point(el);
    if (!search && !center) return fail(el, "no coordinates");
    if (!hasWebGL()) return fail(el, "WebGL is unavailable");

    // The generation token: remove() during a pending init bumps it, and the
    // init that resolves late builds nothing.
    var gen = (el.__opGen = (el.__opGen || 0) + 1);
    setState(el, "loading");
    el.__opTimer = setTimeout(function () {
      if (el.__opGen === gen) fail(el, "the map didn't load within 10 seconds");
    }, LOAD_TIMEOUT_MS);

    loadLibrary(el)
      .then(function (maplibregl) {
        if (el.__opGen !== gen) return;
        var how = search ? null : precision(el);
        var cards = search ? cardsFor(el) : [];
        var view = search ? searchView(cards) : { center: center, zoom: ZOOM[how] };
        var options = {
          container: el.querySelector("[data-map-canvas]") || el,
          style: styleUrl(el),
          maxBounds: JAMAICA_BOUNDS,
          maxZoom: MAX_ZOOM,
          attributionControl: false,
          cooperativeGestures: true,
          dragRotate: false,
          pitchWithRotate: false,
          touchPitch: false,
        };
        Object.keys(view).forEach(function (k) {
          options[k] = view[k];
        });
        var map = new maplibregl.Map(options);
        el.__opMap = map;
        live.push(el);
        map.addControl(
          new maplibregl.AttributionControl({ compact: true, customAttribution: ATTRIBUTION })
        );
        map.addControl(new maplibregl.NavigationControl({ showCompass: false }));
        var loaded = false;
        map.on("error", function (e) {
          if (!loaded) fail(el, (e && e.error && e.error.message) || "map error");
        });
        map.once("load", function () {
          if (el.__opGen !== gen) return;
          loaded = true;
          clearTimeout(el.__opTimer);
          el.__opTimer = null;
          setState(el, "ready");
        });
        var areas;
        if (search) {
          addSearchMarkers(maplibregl, map, el, cards);
          if (!cards.length) overlay(el, NOT_MAPPED);
          areas = cards
            .filter(function (c) {
              return !c.exact;
            })
            .map(function (c) {
              return c.center;
            });
        } else if (how === "exact") {
          new maplibregl.Marker({ color: AREA_COLOR }).setLngLat(center).addTo(map);
          areas = [];
        } else {
          areas = [center];
        }
        if (areas.length) {
          map.on("style.load", function () {
            addAreas(map, areas);
          });
        }
      })
      .catch(function (e) {
        if (el.__opGen === gen) fail(el, (e && e.message) || "MapLibre failed to load");
      });
  }

  // Ends a map (it scrolled far away): frees its WebGL context. init() can
  // build it again later.
  function remove(el) {
    if (el.getAttribute("data-map-state") === "failed") return;
    el.__opGen = (el.__opGen || 0) + 1;
    teardown(el);
    setState(el, "idle");
  }

  // A map initialized while hidden has no size until it's shown.
  function resize(el) {
    if (el.__opMap) el.__opMap.resize();
  }

  // Theme toggle: follow light <-> dim/dark.
  var wasDark = isDark();
  new MutationObserver(function () {
    var dark = isDark();
    if (dark === wasDark) return;
    wasDark = dark;
    live.forEach(function (el) {
      if (el.__opMap) el.__opMap.setStyle(styleUrl(el));
    });
  }).observe(document.documentElement, { attributes: true, attributeFilter: ["class"] });

  window.OurPlacesMap = { init: init, remove: remove, resize: resize };
})();

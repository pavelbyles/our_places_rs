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
  var AREA_COLOR = "#0d9488";
  var ATTRIBUTION = [
    '<a href="https://www.openstreetmap.org/copyright" target="_blank" rel="noopener">© OpenStreetMap contributors</a>',
    '<a href="https://protomaps.com" target="_blank" rel="noopener">Protomaps</a>',
  ];
  var SUPPORTED_KINDS = { listing: true, booking: true };

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
  function addArea(map, center) {
    if (map.getSource("op-area")) return;
    map.addSource("op-area", { type: "geojson", data: circle(center) });
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
    var center = point(el);
    if (!center) return fail(el, "no coordinates");
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
        var how = precision(el);
        var map = new maplibregl.Map({
          container: el.querySelector("[data-map-canvas]") || el,
          style: styleUrl(el),
          center: center,
          zoom: ZOOM[how],
          maxBounds: JAMAICA_BOUNDS,
          maxZoom: MAX_ZOOM,
          attributionControl: false,
          cooperativeGestures: true,
          dragRotate: false,
          pitchWithRotate: false,
          touchPitch: false,
        });
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
        if (how === "exact") {
          new maplibregl.Marker({ color: AREA_COLOR }).setLngLat(center).addTo(map);
        } else {
          map.on("style.load", function () {
            addArea(map, center);
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

# Map fixture (spec 103, plan T12)

A small copy of one tile version, for the `ui-js` Playwright project. Tests
serve it with `page.route` at the base URL the styles name:

    https://storage.googleapis.com/our-places-gcs-map-tiles-test/v20261113

- `jamaica.pmtiles`: Protomaps build 20261113, Jamaica bbox, `--maxzoom=10`
- `style-light.json`, `style-dark.json`: `scripts/map-tiles/make_styles.mjs`
  with the base URL above
- `fonts/*/0-255.pbf`: the first glyph range of each stack (other ranges 404)
- `sprites/v4/{light,dark}{,@2x}.{json,png}`

Data © OpenStreetMap contributors (ODbL), Protomaps. Fonts: Noto Sans (OFL,
see `fonts/OFL.txt`). Regenerate with `pmtiles extract … --maxzoom=10` on a
z15 build and the commands above; keep the folder under 2MB.

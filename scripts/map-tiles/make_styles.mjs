// Writes style-light.json and style-dark.json for one tile version
// (spec 103 §1.10). Run by scripts/build_map_tiles.sh; never in a browser.
//
// usage: node make_styles.mjs <base_url> <out_dir> <fontstack>...
//
// The sources carry no attribution: map.js sets `customAttribution` itself,
// so no HTML from an upstream package reaches the page.
import { writeFileSync } from "node:fs";
import { join } from "node:path";
import { layers, namedFlavor } from "@protomaps/basemaps";

const [baseUrl, outDir, ...fontstacks] = process.argv.slice(2);
if (!baseUrl || !outDir || fontstacks.length === 0) {
  console.error("usage: node make_styles.mjs <base_url> <out_dir> <fontstack>...");
  process.exit(2);
}

const available = new Set(fontstacks);

const isStack = (v) => Array.isArray(v) && v.length > 0 && v.every((f) => typeof f === "string");

/** The font stacks inside an expression: its `["literal", [...]]` lists. */
function stacksIn(expr, out) {
  if (!Array.isArray(expr)) return;
  if (expr[0] === "literal" && isStack(expr[1])) {
    out.push(expr[1].join(","));
    return;
  }
  for (const arg of expr.slice(1)) stacksIn(arg, out);
}

/** Every font stack a style's layers can ask for. */
function fontsUsed(styleLayers) {
  const used = new Set();
  for (const layer of styleLayers) {
    const font = layer.layout?.["text-font"];
    if (font === undefined) continue;
    const stacks = [];
    if (isStack(font)) stacks.push(font.join(","));
    else stacksIn(font, stacks);
    // An expression we can't read would hide which stacks are fetched.
    if (stacks.length === 0) throw new Error(`layer ${layer.id}: no font stack found in text-font`);
    for (const s of stacks) used.add(s);
  }
  return used;
}

for (const flavor of ["light", "dark"]) {
  const styleLayers = layers("protomaps", namedFlavor(flavor), { lang: "en" });
  const missing = [...fontsUsed(styleLayers)].filter((f) => !available.has(f));
  if (missing.length > 0) {
    console.error(`style-${flavor}: font stacks not shipped: ${missing.join("; ")}`);
    process.exit(1);
  }
  const style = {
    version: 8,
    glyphs: `${baseUrl}/fonts/{fontstack}/{range}.pbf`,
    sprite: `${baseUrl}/sprites/v4/${flavor}`,
    sources: {
      protomaps: { type: "vector", url: `pmtiles://${baseUrl}/jamaica.pmtiles` },
    },
    layers: styleLayers,
  };
  writeFileSync(join(outDir, `style-${flavor}.json`), JSON.stringify(style));
  console.log(`wrote style-${flavor}.json (${styleLayers.length} layers)`);
}

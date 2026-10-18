import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { existsSync, lstatSync, readFileSync, readdirSync } from "node:fs";

import manifest from "../package.json" with { type: "json" };
assert.deepEqual(manifest.dependencies ?? {}, {});

const lock = JSON.parse(
  readFileSync(new URL("../package-lock.json", import.meta.url), "utf8"),
);
// Production consumes the audited vendor snapshot, never an npm runtime.
const nodeVersion = readFileSync(new URL("../../.node-version", import.meta.url), "utf8");
assert.match(nodeVersion, /^26\.7\.0\n?$/);
const vendorRoot = new URL("../vendor/xcss/", import.meta.url);
const sourceBytes = readFileSync(new URL("SOURCE.json", vendorRoot));
// Pin the committed receipt as well as its identity: changing both a payload
// and its recorded digest must not turn an edited snapshot into an accepted one.
assert.equal(createHash("sha256").update(sourceBytes).digest("hex"),
  "3bccf5dad68ce08dff8f3b0b318b3d81a3aee294eb9575c62de42e06692ccfb6",
  "The committed xcss source receipt has changed");
const source = JSON.parse(sourceBytes.toString("utf8"));
assert.equal(source.format, 1);
assert.equal(source.package, "@xcss/web");
assert.equal(source.version, "2.0.0");
assert.equal(source.origin, "https://github.com/isarmg/xcss");
assert.equal(source.base_revision, "a055fdf5ce4bdf6cb943284813e42ba864c74b9d");
assert.equal(source.source_directory, "web");
assert.equal(source.npm, undefined);
// Unlisted files, including retired JavaScript, must not survive snapshot sync.
const rootStat = lstatSync(vendorRoot);
assert.ok(rootStat.isDirectory() && !rootStat.isSymbolicLink(), "Vendor root must be a physical directory");
function vendorFiles(directory, prefix = "") {
  return readdirSync(directory, { withFileTypes: true }).flatMap(entry => {
    const relative = `${prefix}${entry.name}`;
    assert.equal(entry.isSymbolicLink(), false, `Vendor symlink: ${relative}`);
    if (entry.isDirectory()) {
      return vendorFiles(new URL(`${encodeURIComponent(entry.name)}/`, directory), `${relative}/`);
    }
    assert.ok(entry.isFile(), `Unsupported vendor entry: ${relative}`);
    return [relative];
  });
}
assert.deepEqual(vendorFiles(vendorRoot).sort(), ["SOURCE.json", ...Object.keys(source.files)].sort(),
  "Vendor files must exactly match the committed receipt");
for (const [path, expected] of Object.entries(source.files)) {
  assert.match(expected, /^[a-f0-9]{64}$/);
  assert.ok(!path.split("/").some(part => part === "." || part === "..") && !path.startsWith("/") && !path.includes("\\"));
  const bytes = readFileSync(new URL(`../vendor/xcss/${path}`, import.meta.url));
  assert.equal(createHash("sha256").update(bytes).digest("hex"), expected, `Vendor drift: ${path}`);
}
for (const required of ["admin-ui/styles.css", "admin-ui/content-blocks.css", "web-fonts/fonts.css", "web-fonts/boot.css", "web-fonts/OFL.txt", "web-fonts/CJK-LICENSE.txt", "web-fonts/NORMAL-LICENSE.txt", "web-fonts/provenance.json", "LICENSE"]) {
  assert.ok(source.files[required], `Missing vendor contract: ${required}`);
}
assert.equal(source.files.LICENSE, "cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30");
assert.match(readFileSync(new URL("LICENSE", vendorRoot), "utf8"), /Apache License\s+Version 2\.0/);
assert.match(readFileSync(new URL("web-fonts/OFL.txt", vendorRoot), "utf8"), /SIL OPEN FONT LICENSE Version 1\.1/);
const adminStyles = readFileSync(new URL("../vendor/xcss/admin-ui/styles.css", import.meta.url), "utf8");
assert.match(adminStyles, /@import ["']\.\/content-blocks\.css["']/);
const aliases = readFileSync(new URL("../rust-web.toml", import.meta.url), "utf8");
assert.match(aliases, /"@xcss\/web\/"\s*=\s*"vendor\/xcss\/"/);
for (const name of ["@xcss/web", "react", "react-dom", "vite", "@vitejs/plugin-react", "typescript"]) {
  assert.equal(manifest.dependencies?.[name] ?? manifest.devDependencies?.[name], undefined);
  assert.equal(lock.packages?.[`node_modules/${name}`], undefined);
}
for (const retired of ["vite.config.ts", "xcss-web-vite.mjs", "xcss-web-assets.mjs", "font-licenses.mjs"]) {
  assert.equal(existsSync(new URL(`../${retired}`, import.meta.url)), false);
}

for (const snapshot of ["content-blocks.css", "provenance.json", "verify.mjs"]) {
  assert.equal(existsSync(new URL(`../appearance/${snapshot}`, import.meta.url)), false, "xcss CSS must not be copied into the product");
}

const main = readFileSync(new URL("../src/main-xcss-web.js", import.meta.url), "utf8");
const styles = readFileSync(new URL("../src/styles.css", import.meta.url), "utf8");
assert.match(main, /pkg\/xszs_web\.js/);
assert.match(main, /xszs_web_bg\.wasm\?url/);
assert.match(main, /xcss-web\/vendor\/xcss-web\/startup\.js/);
assert.doesNotMatch(main, /server-corners|server-corner-geometry|startup\.json/);
const rust = readFileSync(new URL("../xcss-web/src/lib.rs", import.meta.url), "utf8");
assert.match(rust, /xcss_web::mount_server\("root",/);
// Ignore generated lint attributes and formatting while retaining the shared
// shell renderer, its matching props builder, and the exact product binding.
const shellStructure = rust.replace(/#\[[^\]]*\]/g, "").replace(/\s+/g, "");
assert.match(shellStructure, /component_view\(&AdminShell,\{letmutprops=component_props_builder\(&AdminShell\)\.product\(\{"xszs"\},?\)/);

assert.doesNotMatch(main, /AdministratorsPanel|平台管理员|Platform administrators/);
assert.doesNotMatch(main, /useAdministratorSession|function Login|localStorage|sessionStorage|error\.message/);
assert.doesNotMatch(styles, /@font-face|font-family|--xcss-[a-z-]+\s*:|\.toast|\.sidebar/);

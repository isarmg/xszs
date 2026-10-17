import { createXcssWebViteConfig } from "./xcss-web-vite.mjs";
import { isAbsolute, join } from "node:path";
import { pathToFileURL } from "node:url";
import { xcssFontLicenses } from "./font-licenses.mjs";
import { mergeConfig, type Plugin } from "vite";
import { readFileSync } from "node:fs";

async function runtimeInventoryPlugins() {
  const tools = process.env.XSSI_BUILD_TOOLS_DIR;
  const output = process.env.XSSI_FRONTEND_METADATA_FILE;
  if (!tools && !output) return [];
  if (!tools || !output || !isAbsolute(tools) || !isAbsolute(output)) {
    throw new Error("Explicit runtime inventory requires absolute tools and metadata paths");
  }
  const { runtimeInventory } = await import(pathToFileURL(join(tools, "scripts/frontend_build.mjs")).href);
  return [runtimeInventory({ output, root: process.cwd() })];
}

export default async function configuration() {
  const inventory = await runtimeInventoryPlugins();
  return mergeConfig(createXcssWebViteConfig({ base: "/admin/" }), {
  plugins: [...inventory, xcssFontLicenses(), {
    name: "media-font-license",
    generateBundle() {
      this.emitFile({ type: "asset", fileName: "assets/MapleMono-OFL.txt", source: readFileSync(new URL(import.meta.resolve("@xcss/web/web-fonts/OFL.txt"))) });
      this.emitFile({ type: "asset", fileName: "assets/CJK-LICENSE.txt", source: readFileSync(new URL(import.meta.resolve("@xcss/web/web-fonts/CJK-LICENSE.txt"))) });
    },
  } satisfies Plugin],
  build: {
    rollupOptions: {
      output: {
        entryFileNames: "assets/admin.js",
        chunkFileNames: "assets/[name].js",
        assetFileNames: (assetInfo: { names: string[] }) =>
          assetInfo.names.some((name) => name.endsWith(".css"))
            ? "assets/admin.css"
            : "assets/[name][extname]",
      },
    },
  },
  server: {
    port: 5174,
    proxy: { "/api/v1": "http://127.0.0.1:8080" },
  },
});
}

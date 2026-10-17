// Bootstrap only. Application state, networking and views live in web/leptos.
import "@xcss/web/web-fonts/boot.css";
import "@xcss/web/design-tokens/tokens.css";
import "@xcss/web/design-tokens/tokens.dark.css";
import "@xcss/web/design-tokens/reset.css";
import "@xcss/web/design-tokens/accessibility.css";
import "@xcss/web/web-fonts/fonts.css";
import "@xcss/web/admin-ui/styles.css";
import "@xcss/web/admin-ui/date-range.css";
import "./styles.css";
import { startServerCorners } from "@xcss/web/admin-ui/server-corners";
startServerCorners(document);
import init from "../pkg/xszs_web.js";
import wasmUrl from "../pkg/xszs_web_bg.wasm?url";
await init({ module_or_path: wasmUrl });

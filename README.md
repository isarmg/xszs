# XSZS

XSZS pairs devices, receives checked multipart uploads and provides the media library and administrator control plane.

The server runs on x86_64 GNU/Linux. Its executable has one runtime entry point:

```sh
xszs --root /opt/isarmg/xszs
```

Configuration is `ROOT/config/config.json`. State defaults to `ROOT/state`; an explicit `data_dir` may select external instance data. Logs and transient runtime files always use `ROOT/logs` and `ROOT/run`. Configuration is controlled by the administrator and is read-only to the service account.

Installation, upgrades and offline account maintenance are supplied by the separately maintained [XSSI](https://github.com/isarmg/xssi) scripts and SHA-256-verified `libexec/setup` helper. The server executable only runs the service. Upgrade scripts verify the downloaded package before stopping every writer, then use XSSC for locked backups, current-contract validation, replacement and readiness checks. Uninstallation is managed by the operator.

The server manages application authentication, verified instance and file quotas, and bounded resource use. It does not identify client IP addresses or configure any reverse proxy.

See the [documentation](docs/README.md).

前端使用 xcss-web CSR + Rust/WASM，后端继续使用 Axum。构建、验收和迁移边界见 [xcss-web 界面维护说明](docs/xcss-web.md)。


生产前端构建使用仓库内 Rust 工具，无需安装 npm 依赖。准备 Rust 1.99.0、wasm32-unknown-unknown target 和 wasm-bindgen-cli 0.2.129 后，在产品根目录执行：

```sh
cargo +1.99.0 run --quiet --locked --manifest-path tools/web-build/Cargo.toml -- build --product-root .
```

开发监听、Axum 集成和测试边界见 [Rust 原生 Web 构建与测试](docs/rust-native-web.md)。Node/Playwright/Vite 仅用于浏览器验收与历史 TypeScript/React 对照测试。

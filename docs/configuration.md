# 配置

默认文件 `/etc/isarmg/xszs/config.json`，归 `isarmg-xszs:isarmg-xszs` 所有，文件 `0600`、父目录 `0700`。默认文件缺失可使用环境变量和默认值；显式 `--config` 文件缺失、不安全或配置错误则失败。

优先级：CLI > `XSZS_` 环境变量 > JSON > 默认值。未知、重复和错误字段会被拒绝，无旧变量别名。

| JSON | 环境变量 | 默认值 |
|---|---|---|
| bind | XSZS_BIND | 127.0.0.1:8080 |
| data_dir | XSZS_DATA_DIR | /var/lib/isarmg/xszs |
| development | XSZS_DEVELOPMENT | false |
| log_level | XSZS_LOG_LEVEL | info（error/warn/info/debug/trace） |
| trusted_proxies | XSZS_TRUSTED_PROXIES | []（环境值为 JSON 数组，元素是 IP/CIDR） |

`--config`、`--data-dir`、`--bind`、`--development true|false` 为全局参数，可放在子命令前后。development=true 仅允许 loopback 监听。

所有内部状态都在 data_dir：`xszs.sqlite3`、`secrets.json`、`logs/` 和锁文件。无单独 database_url。init 在空实例中自动生成业务密钥并以 0600 原子写入 secrets.json；run/check 只读，缺失或损坏时拒绝，不自动补建。升级保留整个状态目录。管理员密码只在 init 或 admin reset-password 的 stdin 中传入。

日志使用公共 xcss UTC 结构化日志与 logs/ 轮转。生产 HTTPS 由入口负责，管理员认证继续检查 Host/Origin 并设置 Secure Cookie。trusted_proxies 仅控制客户端 IP：默认忽略转发头，只有可信直连代理才按链解析；不会改变 TLS 或认证语义。

`config validate` 只检查配置，无需初始化；`check` 离线检查状态；`status` 查询在线 readiness。

`storage_dir` / `XSZS_STORAGE_DIR` 可单独指定媒体目录，默认 data_dir/media；只放业务媒体，SQLite、密钥及日志仍在 data_dir。setup 为选定的空媒体目录建立服务账户权限，并在 systemd drop-in 中授予 data_dir 和 storage_dir 写权限。后续调整路径时先停服、准备已有媒体/状态及权限，再执行 upgrade 更新写入许可。

上传参数保留 max_part_bytes（默认67108864）、upload_global_concurrency（16）、upload_per_account_concurrency（4）；可用对应 XSZS_ 大写变量覆盖。metrics_token 为可选独立监控访问令牌。development_web_dir / XSZS_DEVELOPMENT_WEB_DIR 仅允许开发构建使用。

# 命令

- `xszs init --username admin`：密码从 stdin 读取，创建空实例、管理员和持久业务密钥；已有状态拒绝覆盖
- `xszs run`：只启动已初始化实例，不创建管理员、密钥或数据库
- `xszs config validate --json`：纯配置验证，初始化前可执行
- `xszs check --json`：离线检查数据库、密钥和目录；不是迁移或修复
- `xszs status --json`：查询在线服务，离线不能报告 ready
- `xszs admin reset-password --username admin`：服务停止后执行，密码从 stdin 读取，使用 data_dir 中固定数据库

全局参数：`--config /absolute/config.json`、`--data-dir /absolute/state`、`--bind IP:PORT`、`--development true|false`。JSON > 默认值之前依次由产品前缀环境变量、CLI 覆盖；配置错误不会被覆盖隐藏。

领域检查保留 `doctor`（媒体内容完整性）和 `reconcile scan`（上传恢复）。`release-verify PATH` 是显式离线发行包检查，与 run 无关。

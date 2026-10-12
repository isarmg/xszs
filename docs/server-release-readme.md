# 安装与运行

发行包根目录包含 `install.sh`、`bin/xszs`、`config/xszs.json.example` 与 `systemd/xszs.service`。Linux x86_64 二进制内嵌管理 Web；普通运行不依赖发行目录、版本目录或 current 链接。解包后始终在该发行包中执行安装脚本。

## 安装

先把示例 JSON 复制到私有文件并填写所需参数。共同参数为 bind、data_dir、development、log_level、trusted_proxies；默认监听 `127.0.0.1:8080`，development=false。不要填写管理员密码、数据库 URL 或业务加密密钥。

```bash
umask 077
cp config/xszs.json.example "$HOME/xszs-config.json"
# 编辑 "$HOME/xszs-config.json"，尤其核对 data_dir 和 bind
sudo -v
read -r -s -p 'Initial administrator password: ' password; printf '\n'
printf '%s\n' "$password" | sudo ./install.sh setup "$HOME/xszs-config.json" admin
unset password
```

唯一安装接口为 `install.sh setup CONFIG_JSON [USERNAME]`。省略 USERNAME 时使用 admin；密码直接传给 init 的 stdin，不写入 JSON、环境变量、unit 或日志。setup 创建专用非 root 服务账户 `isarmg-xszs`，安装所选配置、二进制和 unit，依次运行 config validate、init 和 check。已有配置或非空数据目录会拒绝；setup 不覆盖实例，也不自动启动服务。

- `/opt/isarmg/xszs/bin/xszs`：root 所有的可执行文件
- `/etc/isarmg/xszs/config.json`：`isarmg-xszs` 所有，文件 0600、父目录0700
- data_dir：所选 JSON 指定的私有状态目录，默认 `/var/lib/isarmg/xszs`，服务账户所有、0700
- `/run/isarmg/xszs`：仅由 systemd 在服务运行期间创建；init 和离线检查不依赖它

init 在 data_dir 中创建 `xszs.sqlite3`、`secrets.json`、日志和必要目录；业务密钥只在首次初始化时自动生成。run/check 不会补建丢失的密钥或覆盖数据库。升级时保留完整实例状态。

setup 成功后按输出启动，再检查在线状态：

```sh
sudo systemctl enable --now xszs.service
sudo -u isarmg-xszs -- /opt/isarmg/xszs/bin/xszs status --json
curl --fail http://127.0.0.1:8080/readyz
```

如果调整了 bind，检查对应的实际地址。公网 HTTPS、证书与防火墙由外部入口负责；trusted_proxies 仅用于可信代理提供的业务客户端 IP，不改变 TLS 或认证语义。

## 升级

在新发行包的解压目录中，只执行：

```sh
sudo ./install.sh upgrade
```

upgrade 不接受额外参数，读取已安装的 `/etc/isarmg/xszs/config.json`。先验证配置；如果服务正在运行，先停止并等待退出。随后替换二进制/unit、更新所选状态目录的 systemd 写入许可，执行 daemon-reload 和离线 check；检查成功且此前处于运行状态才重新启动。原来停止的服务仍保持停止；检查失败时保持停止并报告错误。

升级不执行 init，不修改配置、业务密钥或数据库内容，不备份，也不递归改变数据目录所有者。已有状态检查失败时保留现场、读取具体错误，不通过重新初始化或替换密钥绕过检查。

## 配置和运行检查

```sh
sudo -u isarmg-xszs -- /opt/isarmg/xszs/bin/xszs config validate --json
sudo systemctl stop xszs.service
sudo -u isarmg-xszs -- /opt/isarmg/xszs/bin/xszs check --json
sudo systemctl start xszs.service
sudo journalctl --utc -u xszs.service -n 100 --no-pager
```

config validate 是纯配置检查，初始化之前即可执行。check 离线检查实例，需先停服。status 检查在线 readiness。服务使用公共 UTC 结构化日志、data_dir/logs 轮转和120秒 systemd停止超时。

可选 storage_dir 仅存放业务媒体，默认 data_dir/media；SQLite、密钥及日志始终位于 data_dir。首次 setup 要求独立媒体目录为空，并设置服务账户权限。安装脚本按所选 data_dir 和 storage_dir 写入 systemd drop-in 的 ReadWritePaths；不依靠固定的默认数据目录。更改 data_dir 后需安排维护、准备对应完整实例状态和所有者权限，再从当前发行包执行 upgrade 以更新写入许可；不能仅改路径后期望 run 初始化新目录。

[配置](https://github.com/isarmg/xszs/blob/main/docs/configuration.md) · [命令](https://github.com/isarmg/xszs/blob/main/docs/cli.md)

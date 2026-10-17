# xszs 运行与管理 API 参考

日常实例操作见[使用指南](usage.md)，安装参数见[配置参考](configuration.md)。

## 管理员、实例与日志接口

浏览器认证合同只有三条：`POST /api/v1/auth/login`、`GET /api/v1/auth/session`、
`POST /api/v1/auth/logout`。登录 body 精确为 `{username,password}`；登录和 session 成功体精确为
`{authenticated:true,user_id,username,role:"admin",csrf_token}`。备份账户的 `accounts.username` 只用于管理员识别存储租户，不能作为客户端登录凭据；它与 `_common_administrators.username` 是不同身份域。
用户管理等业务位于 `/api/v1/admin/*`，移动端仍只使用 `/v1/*`。管理员 username 规范化、严格
当前 Argon2id、登录准入、Session/CSRF 生命周期、Cookie 和安全审计均由 xcss 的
Admin Core、SQLite Store、Axum Adapter 拥有。空闲 30 分钟、绝对 12 小时、每管理员 32 个/全局 1024 个
Session 是固定平台策略，不提供产品级 TTL 配置。管理员登录按实际数据库每 60 秒最多 60 次尝试；所有已接收尝试计入，成功不清空计数，达到上限返回 429 且不会延长窗口。请求体和密码计算仍受全局资源预算约束，不按 IP、任意账号或会话创建计数桶。

管理实例列表的 `GET /api/v1/admin/overview` 每页最多 50 个账户，响应额外包含
`previous_cursor`、`next_cursor` 和全局 `online_users`。其余全局 total/unlimited/used/pending/quota
由 SQL 聚合全部数据，不按当前页推断。按名称 NOCASE、原名、稳定 UUID 排序，使用原样返回的 cursor
正反翻页；名称重复或锚点行被删除仍可继续，刷新、创建和删除后返回第一页以核对最新列表。
`GET /api/v1/admin/users/{account_id}` 直接读取单个详情，`PUT` 保存后也不扫描全表。
当前 DDL 的唯一索引 `devices_account_unique_idx` 约束每个账户最多一个客户端实例，列表每页与详情嵌套
设备数量因此有界；不是在内存中截断其他设备。列表与详情响应预算为 1 MiB、时限 10 秒，错误/空页
与真正零实例分别表达，超出 JavaScript 精确整数范围的统计明确失败，不截断或重写存量数据。

管理 Web 的日志页使用 UTC 日历日期。`GET /api/v1/admin/logs` 返回
`{date,instance_id,logs,previous_cursor,next_cursor}`；多日范围还包含 `end_date`，省略日期时默认 UTC 当天。
单日使用 `?date=YYYY-MM-DD`，范围使用 `?start_date=YYYY-MM-DD&end_date=YYYY-MM-DD`，
包含起止当天，不能与 `date` 混用；`&instance_id=UUID` 按实例筛选。每页最多 50 条，按审计序号倒序，
使用返回的 `cursor` 继续翻页；翻页必须携带相同的显式日期或范围，游标与范围/实例不一致会被拒绝。
实例筛选覆盖设备 actor、属于设备的 API Key actor 及管理员 device entity，不按共同 account 猜测。
从实例详细信息进入日志会保留该实例上下文，“全部实例”恢复全局日志；刷新返回第一页。
日期边界为 UTC 午夜，每天固定 24 小时；展示时间使用带 `Z` 标识的 UTC 时间。
浏览器单页响应预算 1 MiB、时限 10 秒，不积累历史页。账户级 `GET /v1/audit-events` 保持独立
账户授权及分页合同。结构化请求跨度和提交后实例事件提供稳定 instance_id，可独立分析。

上传分片并发默认全局 16、每个已认证实例 4；等候实例配额的请求不占用其他实例可用的全局名额。
预览解码最多并发 2 个任务，取消页面请求后，已开始的解码仍占用名额直到实际结束。
缓存命中的预览也先经过同一并发入口。上述限制是运行预算，不代表实测吞吐或内存使用量。

配对码先匹配真实 pending 实例，再应用每实例 8 次突发、每 15 秒补充一次的业务频率限制；无效配对码不创建额度键。匿名配对处理另有全局 16 个并发资源名额。

Android/iOS 只向 `/v1/auth/bootstrap` 提交服务器地址、实例授权码和设备信息。授权码在管理员直接新建备份实例时生成，服务端保存密文和独立查找摘要；更换授权码会清除旧设备 Token 并要求重新配对。客户端以实例授权码配对；Server 启动时校验当前 Schema。

## 6. 当前数据库合同

服务端软件版本是 `1.0.1`，数据库合同使用 `v1`：`product_metadata` 必须精确为
`application=xszs`、`application_version=v1`、
`schema_revision=1`，Schema SHA-256 为
`0e37f8a3992b1904215d5f4c9ac428448752718506611f818482d890322d300d`。移动队列对应
`xszc` 1.0.0、schema revision 1 与 SHA-256
`87eb55ba9366cd06d5a2e0b69b5fd4c7a6eef59c381e9fd4ea340e9e04ef6dfb`。移动数据库身份由 Client
仓库定义，不属于 Server 启动时验证的数据库。

数据库只由 XSSI 的独立 setup 在全新安装时创建；普通服务启动不创建数据库。启动持锁后在 ROOT/run 内捕获一次私有快照，同一 generation 验证当前结构、平台元数据、完整性、外键、管理员和实例授权。已存在空文件、非当前元数据或结构漂移会在业务写入前拒绝，不能现场手改指纹“修复”。数据库健康检查要求当前结构和平台元数据有效，连接可以执行 SELECT 1 并不代表服务已经就绪。

## 实例创建和配额

管理端只呈现“备份实例”：点击新建后 `POST /api/v1/admin/instances` 使用默认名称，并在同一数据库事务中创建自动分配的
内部存储归属、100 GiB 默认配额、客户端实例和长期授权码；路径和配额可在详情页调整，不要求管理员先创建业务用户。
内部 `accounts` 仅作为上传数据的隔离与配额边界，不是登录身份，也不会在产品页面暴露账号或密码。当前管理员只能
从右上角人物图标进入 xcss 账户设置。实例创建后永久启用；写请求失败不会自动重放，界面仅显示安全错误和
Request ID。

管理 API 的单实例配额范围为 0～9007199254740991 bytes（0 表示不限）。概览容量合计也必须处于
JavaScript 安全整数范围；超出范围时返回结构化错误，不返回截断、环绕或不精确的计数。
管理页面接受小数 GiB，并四舍五入到最近的整数字节；大于零但不足半字节的输入会被拒绝，避免误设为“不限”。

“更换密码”先显示确认窗口，说明现有客户端凭据立即失效且需要重新配对；取消不会发送写请求。
确认后的请求执行期间禁止重复提交。若请求结果无法确认，界面提示关闭窗口并刷新实例信息，
核对当前密码后再决定下一步操作。

## 存储协调与诊断预算

服务启动和 120 秒后台周期使用同一协调路径和运行锁，重试未完成 upload commit、无引用
blob rooted unlink/删行及 committed/orphan staging 清理。协调不周期性重新 Hash 全部已完成历史 blob。
资源更新后，旧上传回执按不可变 `commit_blob_id` 验证，不要求当前资源指针指向旧版本。
完成上传的实际工作最多全局 2 个、同一持久实例 1 个；HTTP 取消后已接受的工作保持名额至实际结束。
同一 upload 的文件提交锁同时防止重复提交并发。无效上传 ID 或无权限对象不会创建锁目录。

永久删除响应 202 表示用户可见 metadata 已删除但
物理 blob 尚待该协调路径收口，不能盲目重放 DELETE；204 才表示本次已完成物理回收。指标只暴露聚合
数量和字节数，使用独立 Bearer Token。

## 公共接口标识

当前 Server 使用 `.state-instance.lock`、数据库和存储的实际运行互斥锁；工具互斥由独立安装维护程序管理。服务身份头为 `x-service`，健康状态中的公共源码修订字段为 `common_revision`。管理会话采用 `__Host-admin-xszs-session`，显式开发模式采用 `admin-xszs-session`；生产 Cookie 的 Secure、HttpOnly、SameSite、Path 和 CSRF 约束继续生效。资源清单格式为 `web-assets-v1`，公共数据库内部表及索引采用 `_common_` 前缀。

### 上传创建重试标识

`POST /v1/uploads` 必须携带 `X-Upload-Operation: <non-nil UUID>`。
移动端使用已持久化的准备 generation UUID；创建响应丢失、分块重试和进程重启后继续使用同一个 UUID。
服务端将此 UUID 用作上传/去重回执 ID，且绑定到已认证账户、实例和完整 manifest。
缺失或非法 UUID 返回 400；不同 manifest 重用同一 UUID 或其他账户/实例复用返回 409。
相同 operation 的重试不会再写入资源元数据或追加回执：接收阶段返回原缺块列表，提交后返回原资源 ID。
若同一资源已有后续成功提交，旧 operation 返回 `upload_superseded`，不会恢复旧内容。
真实的新版本（包括 A→B→A 或只更新 metadata）必须使用新的准备 generation UUID；相同内容的新 operation 不与未完成旧 operation 合并。
本契约不改变资源 manifest JSON 或数据库 Schema。

实例取消/撤销使用 `POST /api/v1/admin/instances/{id}/revoke`，重复调用保持终态且不删除账号；永久删除使用独立的 `DELETE /api/v1/admin/instances/{id}`，仅接受已取消/撤销且没有备份记录或待回收 blob 的实例。活动实例 DELETE 返回 409。详见 [Web 生命周期合同](xcss-web.md#实例取消与永久删除)。

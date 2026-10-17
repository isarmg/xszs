# Leptos 业务数据强类型化

本次实际接入 DTO 的路径：实例总览、用户详情、配额保存请求及返回值。原来在这些组件之间传递的 `Value` 已换为结构体，UI 直接访问字段。

统计/容量以 u64 保存；表单保留“未修改配额时提交原整数”的流程，避免 GiB 浮点显示造成回写舍入。仅格式化容量时转 f64；缺失实例用 Option，继续显示原来的占位文本。日志等剩余动态响应继续使用既有严格校验。

所有 typed loader/mutation 都先调用原来的 `contracts::validate` / `valid_mutation_response`，再构造 DTO。保留路径与查询参数绑定、revision、页数/条数限制、nullable、未知键策略及整数边界。`models.rs` 不是新增服务端协议。原认证、取消和轮询生命周期沿用原实现；响应不合法继续映射 `platform.invalid_response`。

结构化请求经过 `encode_json_body`，错误显式显示且不发送。分页支持 typed page 的 `PageCursors`（适用时），不再为分页把整页 DTO 转回 JSON；cursor 状态读取借用页面，只复制 cursor 字符串，避免克隆整页列表。

原生校验命令：

```sh
CARGO_BUILD_JOBS=2 cargo test --manifest-path web/leptos/Cargo.toml --lib --offline
```

新增单测覆盖实际 DTO 与既有严格守卫组合，而不是仅测试 derive 能否反序列化。浏览器回归与发布构建由本轮总体验证记录统一列出；本文件不将原生测试等同于浏览器验收，也不声称存在未测量的性能加速。

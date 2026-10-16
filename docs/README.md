# xszs 文档

xszs 接收 xszc 手机客户端的照片和视频，并提供浏览器管理页面。按下面的顺序完成安装、配对和一次测试上传。

## 开始使用

1. [安装与首次运行](server-release-readme.md)：检查归档、通过 XSSI 安装并手动启动。
2. [管理备份实例](usage.md)：创建实例、连接手机、调整配额与查看日志。
3. [配置参考](configuration.md)：环境变量、存储路径、权限与凭据。
4. [日常运维](operations.md)：服务状态、容量、离线诊断与故障定位。

## 开发与参考

- [开发与构建](development.md)：工具链、源码构建、测试和归档验证。
- [服务命令](cli.md)、[运行与管理 API 参考](runtime-reference.md)。
- [图库 API](gallery-api.md)、[接口消费者](interface-consumers.md)。
- [架构](architecture.md)、[功能设计参考](feature-inventory-and-tradeoffs.md)、[仓库职责](repository-boundary.md)。
- [账号设置](account-settings.md)、[公共支撑](common-support.md)、[安全审查](unsafe-audit.md)。
- [当前 1.0.1 发行说明](releases/1.0.1.md)

服务端软件版本为 `1.0.1`，当前数据库身份为 `v1`，数据库结构修订为 1；它们分别管理。Android/iOS 应用与本地队列属于独立 [xszc 项目](https://github.com/isarmg/xszc)。

- [Leptos 界面维护说明](leptos-web.md)：Rust/WASM 构建、原界面对照、实际浏览器验收及 Axum 静态资源。

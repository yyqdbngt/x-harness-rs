# Windows 同学分发与更新

此通道仅属于 `yyqdbngt/x-harness-rs`，与上游正式/演练通道独立。
复用同一个 Host、模型设置、系统凭据存储和 Tauri 更新器，不运行 PowerShell 代理。

## 给同学安装

1. 从本 fork 的 [最新 Release](https://github.com/yyqdbngt/x-harness-rs/releases/latest)
   下载当前版本的 `XHarness_<版本>_x64-setup.exe` 安装。
2. 每位同学自己在「设置 → 模型」添加服务和密钥。软件不携带开发者的 API Key。
3. 启动后以及页面可见时每 6 小时检查更新；也可从左下角更新入口手动检查。
4. 点击下载，验签通过后再确认重启安装。先保存任务：安装会停止 Agent 和后台命令。

更新包有独立签名；没有购买 Windows 发布者证书，首次安装可能出现未知发布者或
SmartScreen 提示。核对本仓库来源和 SHA256SUMS，不要关闭安全防护。更新验签不等同于
微软发布者信誉，不保证消除提示。

首轮 Release 额外带一个低一版基础包用于验证升级。两版来自同一源码提交，版本差用于
验证发现/下载/安装流程，不代表目标版新增了额外功能。常规安装请选择当前版本。

## 维护者发布新版

维护分支为 `btlqql/friends-updates`；包含共享模型配置实现。上游 PR 未合并之前，
不要从不包含这些更改的旧 master 直接发包。先合入希望分发的代码、审阅差异，再将
`friends-v<major.minor.patch>` Tag 指向该提交并推到本 fork。

`.github/workflows/friends-release.yml` 在 Windows 原生 CI 执行测试、打包、签名和独立验签，
然后先上传 Draft，全部成功才公开并更新 latest。普通代码 push 不会发布更新。
Tag 必须递增；已存在的 Release（包括 Draft）不能覆盖。失败的 Draft 应检查原因，
保留证据并选择新版本重新发布，不能把未验签文件硬塞进已公开版本。

固定通道为 `https://github.com/yyqdbngt/x-harness-rs/releases/latest/download/latest.json`，
清单中的安装包地址使用不可变版本 Tag。不要把本仓库的其他无关 Release 标记为 latest。
本通道当前仅 Windows x64，macOS/Linux 不会被误指向 Windows 安装包。

## 签名密钥

专用 Actions Secrets：`XHARNESS_FRIENDS_PRIVATE_KEY`、`XHARNESS_FRIENDS_PASSWORD`、
`XHARNESS_FRIENDS_PUBLIC_KEY`。公钥同时写入 Rust 编译配置和 Tauri Bundler 配置。
绝不能把私钥、密码、用户凭据上传为 Artifact、放进 Release 或提交 Git。

首次配置的本地备份位于维护者 Windows 用户的
`%LOCALAPPDATA%\XHarness\signing\friends`：

- `updater.key`：有密码保护的私钥；`updater.key.pub`：可公开的公钥。
- `password.dpapi.clixml`：Windows DPAPI 保护的密码，只能由相应用户/机器解密。
- 目录 ACL 限制为当前用户。此目录不在项目或 OneDrive 文档目录中。

维护者仍须把私钥和密码另外备份到可靠的加密密码库；仅复制 DPAPI 文件到新电脑
不能保证恢复密码。不要把它们发到聊天中。不要随意重新生成/替换密钥：旧客户端只信任
原公钥，丢失原私钥后通常需要用户手动安装使用新公钥的基础包。

## 测试边界

CI 执行完整 Windows Host/桌面测试、签名反例测试（篡改内容、篡改注释、错误公钥）、
版本/渠道隔离测试和安装包验签。CI 成功不等于完成真实系统安装验收。
首轮交付须单独报告本地安装、更新发现、下载和重启升级各阶段的实际结果。
应用数据沿用当前用户的 `com.xlang.xharness` 目录；更新不迁移或打包这些私人数据。

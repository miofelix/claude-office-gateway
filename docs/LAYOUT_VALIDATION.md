# beta.3 布局修复与验证

日期：2026-10-27。

## 根因与修复

在未修复的生产页面中，将视口设为 `780×560`，`document.documentElement.scrollHeight` 为 `1184`，同时 `.page-scroll` 自己也滚动，复现双滚动条。

复制反馈的 `.sr-only` 元素采用绝对定位，滚动容器没有定位上下文，其偏移相对于外层文档计算。把 `.page-scroll` 临时设为 `position: relative`，外层文档高度立即恢复为 `560`；撤销后恢复为 `1184`。

修复为内容滚动区域建立定位上下文，并把 `html/body/#root` 与应用壳约束在视口内。隐藏的实时反馈保留，标题栏不随内容滚动。

检查还复现并修复了三个显示问题：

- 长发布说明同时触发说明区域和更新弹窗的滚动条；改为弹窗统一滚动，并校验底部按钮可到达。
- 无分隔长 URL 或错误文字撑出通知／更新弹窗；允许文字按可用宽度换行。
- 原生 WebKit 在 `360px` 视口展开高级设置时，双列表单的原生控件撑出卡片；小视口统一使用单列。这也覆盖高缩放下的布局。

## 可重复检查

安装项目依赖后，在 macOS 运行：

```bash
npm run test:layout
```

`scripts/check-layout.py` 启动随机本地端口的 Vite 服务，运行 Swift 原生 WKWebView 检查并自动清理。使用独立的临时 WebKit 存储和 `tests/layout` 的合成 IPC 数据，渲染生产 React 树与 CSS。测试 fixture 不属于生产前端入口，不访问真实凭据、证书或供应商。

`scripts/check.sh` 已包含此项，macOS CI 与正式发布构建必须通过。覆盖 176 个布局场景：两种平台的 CSS 分支、中英文、亮暗外观、`360/480/620/780/920/921/1060/1440px` 断点和 `560/800px` 高度、展开高级设置／证书说明、连接和复制成功、长错误，以及证书、轻量模式和更新弹窗。

断言包含外层文档和应用壳没有额外滚动、主内容无横向溢出、标题栏控件不重叠、页脚可到达、弹窗处于视口内、弹窗最多一个滚动区且底部操作可到达。修复前原生检查报告 228 个失败项，修复后 176 个场景全部通过。

另有 `ego-browser` / Chromium 交互检查，共 164 个场景。先运行 `npm run dev:renderer -- --host 127.0.0.1`，然后在代理拥有的同一个 TaskSpace 中执行：

```javascript
const page = task.page("p1");
const { testLayout } = await import("file:///absolute/repo/path/scripts/test-layout.mjs");
await testLayout(page);
```

Chromium 与 WKWebView 使用相同的几何和滚动断言。原生 WebKit 的 Windows CSS 分支检查不等同于实际 Windows WebView2 桌面验证；Windows 的原生编译、既有回归测试和安装器构建由 Windows Actions 执行。

## 发布平台

`0.2.0-beta.3` 起正式构建只接受 `aarch64-apple-darwin` 和 `x86_64-pc-windows-msvc`。发布矩阵、签名构建与资产收集的目标列表、清单完整性校验和中英文下载说明同步移除 macOS Intel。发布测试验证两种支持平台缺一不可、Windows EXE/MSI 变体必须齐全、退休的 Intel 产物不能混入新版本。

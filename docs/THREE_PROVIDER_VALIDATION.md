# 三供应商接口与 Office 功能验证

日期：2026-10-28。使用当前设备已保存的 DeepSeek、阿哲（sub2api）和 Kimi 配置。运行时 API 密钥仅由钥匙串读入内存，报告不包含密钥或文档正文。测试请求均为合成内容，不修改用户课程报告。

## 相同接口矩阵

`scripts/live-office-smoke.py` 串行运行，每组请求间隔三至四秒。当前矩阵包含：本地路由／认证／CORS、中文 JSON、中文 SSE、强制工具调用 JSON 及续轮、强制工具调用 SSE 及续轮、多工具错误结果和图片历史、嵌套中文工具参数、图片实际计数、token 计数接口、系统指令，以及任意工具强制调用 SSE 及续轮。每个返回的网关请求编号能与脱敏日志关联。三家最终 11 组接口矩阵均已通过。token 计数一项对 Responses 后端验证的是明确返回 501，不等于实现了精确计数。

| 供应商 | 首轮 10 组结果 | 已证实的问题 | 对照与修复状态 |
| --- | --- | --- | --- |
| DeepSeek / Anthropic Messages | 首轮 10/10；最终 11/11 通过 | 本轮未见接口失败 | 最终包含新增“任意工具”调用及结果续轮 |
| 阿哲 / OpenAI Responses | 首轮 9/10；修正测试指令后 11/11 通过 | 图片计数曾连续出错；一次系统指令测试存在措辞歧义 | 图片同样请求直连阿哲也回复 3；后两轮同图返回 2，体现上游识别不稳定；Gateway 图片字节与顺序断言通过。系统指令去除“包含连字符却要求无标点”的矛盾后，专项 2/2 与完整 11/11 通过 |
| Kimi `kimi-k3.1-pro` / OpenAI Responses | 修复前 7/10；最终构建 11/11 通过 | 三项指定工具调用原来直接 HTTP 400 | 直连上游错误为 `tool_choice 'specified' is incompatible with thinking enabled`；移除 reasoning 或发送 `thinking.disabled` 均无效，`required` 也被接口拒绝；使用受限 `auto` + 明确工具指令，指定、任意工具和结果续轮均实际通过 |

阿哲图片重复三次均回答 3，直连相同请求也回答 3。该样本在本机渲染验证为两个红色方块、一个蓝色、一个绿色。这个结论只说明该次错误发生在上游模型的实际视觉判断环节，不能推断所有图片任务的质量。

阿哲最终完整报告为 `azhe-corrected-final-matrix.json`。此前 `azhe-final-matrix.json` 为 10/11，其中系统测试提示同时要求保留带连字符的字符串、不得输出标点，存在歧义；已将提示改为逐字输出并完整重跑，不覆盖旧记录。

Kimi 修复仅对 `api.moonshot.cn` 且模型恰为 `kimi-k3.1-pro` 的 OpenAI Responses 工具强制选择生效：具名选择时只向上游提供被选工具；`any` 时保留可用工具；两者都改为 `auto` 并加入明确调用指令。如果上游仍只回答文本，Gateway 返回错误，而不会把未执行工具伪装为成功。由于 `auto` 由模型决定，不能保证百分之百满足强制选择；测试结果需单独记录。该兼容逻辑及失败检测已有本地回归；最终构建在钥匙串正常授权后，先通过 4/4 工具专项，再完整通过 11/11 组接口用例，报告为 `kimi-final-matrix.json`。

上述接口矩阵对应可执行文件 SHA-256：`00eea772fd7eded35decaf9566f78719db2d7f1e773db542c127f70392974d16`。该轮 Rust 39/39 测试、Clippy、格式检查、26 项前端测试、TypeScript、生产构建和 macOS 签名完整性检查均通过。后续错误诊断构建的验证范围见下文；此前矩阵不能算作后续构建已重跑。

## 传输与错误处理

当前改进包括：流式连接按连续无数据而非整个生成时长超时；下游十秒心跳；混合换行 SSE 正确拆分；并发许可保持到流结束；只对可辨认为请求发送前的临时连接失败最多重试一次；保持原始 JSON 截止时间；请求已发送后、TLS、代理、HTTP 错误、流中断均不自动重放。上游 429／503 保留 `Retry-After` 与上游请求编号；日志只记录错误类别、阶段、耗时与网关请求编号。

本地故障注入先复现了临时解析／建连失败导致 502，修复后仅两次连接尝试且上游只接收一次 POST。另有真实 TLS 模拟上游覆盖已提交 POST 断开、工具流中途断开、上游限流、证书拒绝、代理拒绝与总超时。

## PowerPoint 端到端范围

真实课程文稿在 PowerPoint 中经过长时间自动操作，已观察到读取、创建、形状编辑、图表插入、截图和页序修复；该文稿曾出现 `slide_id=undefined`、`insertSlidesFromBase64: InvalidArgument`、重复页和图表视觉问题，它们属于 Office 操作与生成包层面，不能用接口矩阵通过来宣称解决。

当前独立回归文稿为 `artifacts/validation/Gateway-三供应商功能回归.pptx`。此文稿保存为隔离测试基线；只有在官方加载项实际完成编辑并核对 PPTX 内容后，才记为 PowerPoint 实际读写通过。

因新文稿的加载项初始化白屏，改用已有可交互会话另存副本 `Gateway-实际Office功能回归.pptx`，原课程报告文件仍保留。阿哲实际操作完成：原有 19 页之后追加 3 页，连续两次改写并读回中文文本；创建原生 3×3 表格并读回 9 格；创建仅一张原生 XY 图，两个独立系列与指定 X/Y 数值一致；复制测试页后仅删除副本，最终页数 22。`scripts/audit-office-regression.py` 对保存后的 PPTX 独立校验全通过，前 19 页的规范化 slide XML、原图表数据和已有备注未改变，表格和图表均为真实对象。真实回归仍需对 DeepSeek 和 Kimi 重复同一操作序列，尚不能宣称三家 Office 工具功能全部通过。

DeepSeek 已对同一隔离副本执行相同实际操作，追加第 23–25 页，连续中文改写、3×3 原生表格、双系列 XY 图、复制后仅删除副本均完成并读回。保存后的 `deepseek-office-audit.json` 全部通过，前 19 页仍与原报告规范化 XML、图表、备注一致；阿哲测试页的数据也保持完整。Kimi 的实际 Office 操作仍待完成。

本轮备注代码返回 `undefined is not an object`，未写成功。早先四页验证稿的四个 notesSlide 均有真实非空备注（92、87、89、93 字符），故不能凭本轮错误断言整个平台不支持备注；具体调用方式仍需排查。

## 2026-10-28 流式错误诊断与暂停点

Kimi 在真实 Office 操作中于 `2026-10-28T00:24:42.945Z` 返回 `Upstream returned a streaming error`。旧代码丢弃了原始错误类别，当时日志无法追溯确认具体原因。恢复同一会话后，在 `2026-10-28T00:38:09.944Z` 又收到 HTTP 429，旧提示仍只表示通用限流，不能据此认定最初的流式失败也是同一原因。

本轮已修正上游 HTTP／SSE 错误分类：识别速率限制、token 速率限制、额度／余额不足、上下文超长、工具选择／参数失败和过载；响应只包含固定安全提示。HTTP 错误正文最多读取 16 KiB、等待两秒，异常时保持原状态和 `Retry-After`。日志只记录安全类别、单向指纹、请求编号和请求规模；请求编号还附在 `error.message` 中，供 Office 复制错误详情时保留。上游拒绝及已提交请求仍不自动重放。

新构建的最小 Kimi 连接探针被上游 HTTP 429 拒绝，安全类别为 `quota`（额度或余额不足）。日志时间 `2026-10-28T00:47:26Z`，请求编号 `og_05a7258a40fc4f02aa9be6e1a2800afc`，请求仅 147 字节、1 轮、0 个工具、输出上限 1 token。这确认当前连接受到上游账户额度状态阻挡；不能由此倒推最初流式错误的具体原因。

当前已安装诊断版 SHA-256：`e6d3d54665cdb460e925647def78ed673a3bab71a3fa26df79fd9f2bfcbd1511`，`transport_revision` 为 `2026-10-28.1`。43 项 Rust 测试、Clippy（禁止警告）、26 项前端测试、TypeScript、生产构建及 macOS 签名完整性检查通过。新增回归覆盖错误类别和隐私、Office 复制文本保留编号、SSE 失败不伪造成功／不重放、超大或停滞错误正文保持原 HTTP 拒绝。

隔离副本已保存为 28 页。独立校验 `kimi-office-before-stream-diagnostics.json` 确认原 19 页的 XML、图表和备注保持一致，阿哲与 DeepSeek 的文本／原生表格／XY 图数据仍通过；Kimi 第 26 页文本已写入，第 27–28 页表格和图表尚未完成。Kimi 备注路径也未完成验证。用户随后要求停止测试、先提交当前阶段改动，因此三家完整 Office 回归仍不能标记为全部通过。

## 复现命令

```sh
python3 scripts/live-office-smoke.py --timeout 180 --case-interval 3 \
  --output artifacts/validation/<provider>-full-matrix.json
cargo test --locked --manifest-path src-tauri/Cargo.toml
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

测试需先在 Gateway UI 选中对应供应商并连接。探针按当前活动配置路由，不能在供应商切换过程中并行运行。

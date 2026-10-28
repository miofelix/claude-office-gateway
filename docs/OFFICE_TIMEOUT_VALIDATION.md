# Office 长任务超时修复与真实回归

日期：2026-10-27。使用本机 PowerPoint 官方 Claude 加载项与已保存的 DeepSeek 供应商配置验证。用户报告的 Office 构建标识为 `472d4fadab377116a8627bcada574e6737af6d64`；本地 Gateway 是包含未发布修复的 `0.2.0-beta.4` 构建。

## 可重复的故障与修复

原实现使用 Reqwest 客户端的总请求超时。配置为 120 秒时，即使上游持续返回文本、工具参数或 SSE 注释，整个回复超过 120 秒也会被终止，生成用户看到的 `The provider timed out. Check the network or increase the provider timeout.`。

缩短为 1 秒后，以真实 localhost TLS 模拟上游，每 175 毫秒发送一条 SSE 数据，1.4 秒后结束。以下命令在修复前两次复现同文报错，约 1.05 秒失败：

```sh
cd src-tauri
cargo test --locked regression_active_stream_must_not_hit_total_provider_timeout -- --nocapture
```

修复后，流式响应按上游连续无数据的读取间隔超时；连接阶段最多等待配置值与 30 秒中的较小值。非流式 JSON、token 计数与连接探针仍受配置的总请求时限约束。下游每 10 秒收到标准 Anthropic `ping`，上游只有注释或推理事件时仍可保持 Office 连接；该心跳不延长上游的读取期限。传输错误日志只记录阶段、耗时与是否超时，不记录认证、URL 或正文。

同时保留此前发现的两个修复：混合 CRLF/LF 的 SSE 记录按最早边界切分；并发许可直到响应流消费完成或断开才释放。

参考：[Reqwest 总超时与读取超时](https://docs.rs/reqwest/0.12.28/reqwest/struct.ClientBuilder.html#method.read_timeout)、[Anthropic 流式 ping 事件](https://platform.claude.com/docs/en/build-with-claude/streaming#ping-events)。

## 本地回归

`cargo test --locked`：30/30 通过。新增超时测试覆盖：

- 上游持续活动超过配置时限，流仍以 `message_stop` 正常结束。
- 上游不返回响应头，按期限返回 HTTP 504。
- 上游停止发送数据，Office 在第 10 秒收到心跳后，仍于约第 11 秒收到一次超时错误，且没有伪造成功结束。
- 非流式响应持续发送零散数据，仍按总时限退出。
- 上游只发送 SSE 注释，下游及时收到合法 `ping`。

`cargo clippy --locked --all-targets -- -D warnings`、Rust 格式检查、前端格式检查、TypeScript 检查、26 项前端测试与生产构建均通过。macOS Release 应用构建与 `codesign --verify --deep --strict` 通过。

## DeepSeek 真实请求

供应商 `DeepSeek`，协议 `anthropic`，模型 `deepseek-flash`，超时配置保留为 120 秒。使用已安装的修复版执行：

```sh
python3 scripts/live-office-smoke.py --timeout 180 --workers 1 \
  --output artifacts/validation/deepseek-timeout-fix-smoke.json
```

6/6 组通过：路由、认证与 CORS；中文 JSON；中文 SSE；JSON 强制工具调用及结果续轮；SSE 强制工具调用及省略工具定义的结果续轮；并行工具历史、错误工具结果与图片输入。流式中文首段约 0.63 秒，工具流式首段约 0.89 秒。

首轮脚本曾误把原生 Anthropic 用量对象的嵌套字段判为非法。已改为验证实际 token 数值并允许嵌套用量元数据，随后完整重跑通过。脚本默认串行，并支持 `--case` 单项回归，避免默认并发给供应商增加压力。这些合成请求验证协议传输，不能替代真实 PPT 的内容与视觉检查，也不证明供应商对图片的视觉理解能力。

## 本机交付与文档操作观察

修复应用已安装到 `/Applications/Claude Office Gateway.app`，原应用保留于 `artifacts/validation/Gateway-before-timeout-fix.app`。安装后确认新进程监听 `127.0.0.1:15724`，界面显示 Office 连接已就绪，活动供应商为 DeepSeek。安装二进制 SHA-256：`e2a167b4c7e38513c649bd17017006c18e1e4b37c6a23c8cfd72e00eb4d44f0d`。

17 页报告草稿已保存为 `artifacts/validation/演示文稿1.pptx`，修复前副本为 `artifacts/validation/Gateway-课程报告-修复前草稿.pptx`。用户已继续运行加载项任务；实际对话可见多轮页面编辑、表格内容写入、原生图表插入、备注、截图及视觉检查。以下问题仍单独记录，不归入已解决的传输超时：

- `slide_id` 为 `undefined`：工具输入校验失败。成功编辑会返回 `slide_id_update`，调用方应更新缓存或重新读取页面 ID；现有记录不足以断言漏参的唯一原因。
- `Presentation.insertSlidesFromBase64` 返回 `InvalidArgument`：Office 拒绝导入该次生成包。仅凭错误位置或局部 XML 不能确定具体的 XML、关系或导入参数问题。
- 第 11 页图表存在坐标标题错位、旧图叠加、分支串联等视觉或内容问题，加载项正在修改并复检。图表插入返回成功不代表根轨迹数学或版式已通过。

Gateway 不执行 Office 文档工具，也没有在本次修复中改写模型生成的 Office.js、XML 或工具输入来掩盖这些错误。

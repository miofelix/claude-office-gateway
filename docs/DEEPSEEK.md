# DeepSeek Office 工具兼容

日期：2026-10-27。只匹配供应商 URL 的主机名 `api.deepseek.com`，支持 Anthropic Messages 和 OpenAI Chat Completions。

## 配置

推荐使用 Anthropic Messages，基础地址 `https://api.deepseek.com/anthropic`，认证 `x-api-key`，供应商模型填写实际 ID，例如本次验证使用的 `deepseek-flash`。Office 模型别名保持 `claude-office-custom`。

若使用 OpenAI Chat Completions，基础地址为 `https://api.deepseek.com`，认证 `Authorization: Bearer`，高级设置的 token 字段选择 `max_tokens`。

## 原因与处理

用户报告 Office 可以连接，但对话显示 `Claude is temporarily unavailable`，详细错误为上游 HTTP 422。在独立空白 PowerPoint 文稿中通过官方加载项复现：Office 给函数工具附加 `type: "custom"`，DeepSeek 原生接口不接受这个类型。只省略工具定义里的这个字段，同一真实密钥和合成工具请求由 422 变为 200。

还发现思考模式下的工具选择与历史要求存在兼容差异。使用用户已保存且授权测试的真实密钥，以合成内容直接请求官方接口得到以下对照结果：

| 场景 | 思考设置 | 结果 |
| --- | --- | --- |
| Office 函数工具带 `type: custom` | disabled | HTTP 422：工具类型 `custom` 无法反序列化 |
| 同一工具省略 `type` | disabled | HTTP 200，`tool_use` |
| 指定 `tool_choice: tool` | 默认／adaptive | HTTP 400：`Thinking mode does not support this tool_choice` |
| 同一指定工具请求 | disabled | HTTP 200，`tool_use` |
| 回传该工具结果 | adaptive + high | HTTP 400：要求回传 `content[].thinking` |
| 首次和后续工具请求 | disabled | HTTP 200，工具调用后正常返回文本 |

DeepSeek 默认开启思考模式。强制工具选择存在模式限制，思考模式下的工具历史还需要保留思考内容。Office 的工具请求与历史无法保证满足这些条件，因此网关对整个工具会话采用一致的非思考模式。

非空 `tools`，或历史中的 `tool_use` / `tool_result` 都会触发兼容处理；后续即使省略工具定义，也继续使用非思考模式。Anthropic 函数工具仅移除值为 `custom` 的 `type` 字段，保留名称、描述与输入 schema，不改写其他类型工具；同时设置 `thinking.type=disabled` 并移除 `output_config.effort`。Chat 请求经已有工具格式转换后设置非思考模式并移除 `reasoning_effort`。

token 计数接口同样会拒绝 `type: custom`，因此也规范化该工具字段，但不添加或修改思考设置。普通无工具请求、其他域名和 OpenAI Responses 保留原行为。第三方转发站不按模型名猜测供应商。界面在使用 DeepSeek 官方 Anthropic / Chat 接口时显示提示。

## 回归验证

真实 TLS 模拟上游的网关集成测试先后复现 HTTP 400 和 Office 实际格式触发的 HTTP 422，修复后通过。覆盖 Anthropic / Chat 的指定工具、任意工具、自动工具、默认参数、无工具文本、无工具定义的结果回传，以及普通供应商和相似域名不受影响。

真实密钥验证只使用合成的幻灯片计数工具，不读取 Office 文档。密钥由本机钥匙串读取至进程内存，测试记录不包含密钥、网关令牌或用户文档。实际发布构建验证结果见 [公测验证记录](RELEASE_VALIDATION.md)。

修复后的 macOS 签名构建通过本机 HTTPS 网关完成非流式与流式工具往返，均保留输入中的 adaptive/high 参数。工具调用、工具结果和最终答案均验证成功；流式续轮额外省略工具定义，确认兼容处理根据工具历史继续生效。

PowerPoint 官方加载项实际验证也通过：在本轮新建的空白文稿中，文本请求正常返回 `HELLO`，读取幻灯片数量返回 1，随后通过工具成功新增标题为 `Gateway validation` 的第 2 页。原有用户文稿未被修改。

## 官方参考

- [Anthropic API 兼容说明](https://api-docs.deepseek.com/guides/anthropic_api)
- [思考模式与工具历史](https://api-docs.deepseek.com/guides/thinking_mode)
- [Chat Completions 参数](https://api-docs.deepseek.com/api/create-chat-completion)

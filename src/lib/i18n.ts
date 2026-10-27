import { useEffect, useState } from "react";
export type Language = "en" | "zh";
function preferredLanguage(): Language {
  let saved: string | null = null;
  try {
    saved = window.localStorage?.getItem("office-gateway-language") ?? null;
  } catch {}
  return saved === "en"
    ? "en"
    : saved === "zh"
      ? "zh"
      : navigator.language.startsWith("zh")
        ? "zh"
        : "en";
}
let language: Language = preferredLanguage();
const zh: Record<string, string> = {
  'Office requires a model ID starting with "claude", such as "claude-office-custom". This gateway alias maps to your provider model.':
    "Office 要求模型 ID 以「claude」开头，例如「claude-office-custom」。此网关别名会映射到供应商模型。",
  'Office requires the model ID to start with "claude", such as "claude-office-custom". Use this gateway alias in Office; keep the actual model ID in Provider model.':
    "Office 要求模型 ID 以「claude」开头，例如「claude-office-custom」。在 Office 中使用这个网关别名；真实模型 ID 填在左侧「供应商模型」。",

  "Version and updates": "版本与更新",
  Close: "关闭",
  "Beta channel": "测试通道",
  "Stable channel": "稳定通道",
  "Checking for updates…": "正在检查更新…",
  "You are up to date.": "当前已是最新版本。",
  "A new version is available.": "发现新版本。",
  "Downloading and verifying update…": "正在下载并验证更新签名…",
  "Installing update…": "正在安装更新…",
  "Restarting…": "正在重启…",
  "Updates are verified with a signing key before installation.":
    "安装前会校验更新签名，确保更新包来源可信。",
  "Available version": "可用版本",
  "Last checked": "上次检查",
  "Check for updates on startup": "启动时检查更新",
  "Installing an update restarts this app and pauses the local gateway.":
    "安装更新会重启应用并短暂中断本地网关，请先完成当前 Office 任务。",
  "Release notes": "更新说明",
  "Download from GitHub": "前往 GitHub 下载",
  "Updating…": "正在更新…",
  "Checking…": "正在检查…",
  "Install and restart": "安装并重启",
  "Check for updates": "检查更新",
  "Could not check for updates. Check your network and try again.":
    "暂时无法检查更新，请检查网络后重试。",
  "Update endpoint configuration is invalid.": "更新源配置无效。",
  "Could not initialize signed updates.": "无法初始化签名更新。",
  "An update operation is already running.": "已有更新任务正在进行。",
  "Update details changed. Check for updates again before installing.":
    "更新信息已变化，请重新检查后再安装。",
  "Update download or signature verification failed. Nothing was installed.":
    "下载或签名校验失败，未安装任何更新。",
  "Could not install the update. The gateway has stopped; reconnect or install from GitHub.":
    "更新安装失败，网关已停止；请重新连接或从 GitHub 手动安装。",
  "Could not install the update. Move the app to Applications or install the new version from GitHub.":
    "更新安装失败；请将应用移到「应用程序」，或从 GitHub 安装新版。",

  Appearance: "外观",
  Light: "亮色",
  Dark: "暗色",
  "Follow system": "跟随系统",
  "macOS: user-level SSL trust for localhost in your login Keychain.":
    "macOS：在当前用户的登录钥匙串中，为此证书设置 SSL／localhost 信任。",
  "Windows: this certificate is added to Current User / Trusted Root Certification Authorities.":
    "Windows：将此证书加入「当前用户／受信任的根证书颁发机构」。",

  "Connect your model to Office": "把你的模型接入 Office",
  "Trust this device, connect a provider, then add the gateway in Office.":
    "安装本地证书 → 连接模型供应商 → 在 Office 中填入网关信息。",
  "Local HTTPS certificate": "本地 HTTPS 证书",
  "Lets the Office add-in reach your local gateway securely.":
    "让 Office 加载项安全访问本机网关。",
  "Not yet trusted": "尚未信任",
  "Not installed": "未安装",
  "Local gateway": "网关地址",
  "Current user": "当前用户",
  "What is trusted?": "证书信任范围",
  "Certificates identify hosts, not ports.": "证书校验主机名，不区分端口。",
  "The certificate matches localhost and loopback addresses. It cannot be restricted to one port. Trusting this certificate does not automatically trust unrelated localhost certificates.":
    "证书匹配 localhost 与回环地址，无法限定单个端口。信任本应用的证书，不会让其他无关的 localhost 证书自动受信任。",
  "The gateway separately checks the local port, allowed Office origin, and gateway token.":
    "网关服务另外校验本地端口、Office 来源和网关令牌。",
  "Check certificate": "检查证书状态",
  Check: "检查",
  "Uninstall certificate": "卸载证书",
  "Uninstall the local certificate?": "卸载本地证书？",
  "Use the API URL, key, and model from your provider.":
    "填写供应商提供的 API 地址、密钥和模型 ID。",
  "Saved key · leave blank to keep": "已保存密钥，留空即可沿用",
  "Provider verified": "供应商验证通过",
  "Connect saves your settings and checks the provider.":
    "连接前自动保存并检测供应商。",
  Save: "保存",
  "Save and connect": "保存并连接",
  "Changes use the Save button above. Stop the gateway before editing.":
    "修改后点击上方「保存」。运行时请先停止网关再编辑。",
  "Test session": "隔离测试",
  "Test session: keys are kept in memory and are not saved to your credential vault.":
    "隔离测试模式：密钥仅保存在当前会话，不写入系统凭据库。",
  "Keys stay in your system credential vault. Document contents are not saved.":
    "密钥保存在系统凭据库，文档内容不落盘。",
  "Set up the Office add-in": "连接 Office 加载项",
  "Choose an Office app, then enter the connection details.":
    "先安装对应加载项，再填写下方连接信息。",
  "Install the official Claude add-in": "安装 Claude 官方加载项",
  Install: "安装",
  "In the Claude add-in, choose Gateway": "打开 Claude 加载项，选择 Gateway",
  "Open the add-in in Office, choose the gateway sign-in option, and use these settings:":
    "在加载项的登录方式中选择 Gateway，然后按以下选项设置：",
  "API format": "API 格式",
  Authentication: "认证方式",
  "Paste these three values": "将这三个参数粘贴到加载项",
  Ready: "已就绪",
  Copy: "复制",
  "The gateway token is different from your provider key. Paste it without Bearer.":
    "网关令牌与供应商密钥不同。粘贴时无需添加 Bearer 前缀。",
  "Connect the provider first to enable copying. The gateway token is separate from your provider key.":
    "连接供应商后即可复制。网关令牌与供应商密钥不同，无需添加 Bearer 前缀。",
  "Keep this app running while using Office.":
    "使用 Office 期间保持本应用运行。",
  "Provider checks may incur a small charge.":
    "供应商连接检测可能产生少量费用。",
  "This installs this app’s one-year localhost certificate for your current user. Certificate checks match hostnames, not ports. Your system may ask you to confirm the trust change.":
    "将本应用有效期一年的 localhost 证书安装到当前用户。证书按主机名校验，不限制端口。系统可能要求你确认信任变更。",

  "Open GitHub repository": "打开 GitHub 仓库",
  "Current user · localhost only": "当前用户 · 仅限本机",
  "Model provider": "模型供应商",
  "Office connection": "Office 连接",
  "In Office, choose Gateway → Anthropic → x-api-key. Paste the gateway token without Bearer.":
    "在 Office 中选择 Gateway → Anthropic → x-api-key。粘贴网关令牌时无需 Bearer 前缀。",
  "Changes use the Save provider button above.": "修改后点击上方「保存配置」。",
  Copied: "已复制",

  "Office Gateway": "Office Gateway",
  "Private model bridge": "本地模型网关",
  "Close navigation": "关闭导航",
  "Main navigation": "主导航",
  Connection: "连接",
  "Setup guide": "使用指南",
  "Advanced settings": "高级设置",
  client: "客户端",
  "Closing this window keeps the gateway running. Quit from the tray or menu bar to stop it.":
    "关闭窗口后网关继续运行。从菜单栏或托盘退出应用即可停止。",
  "Open navigation": "打开导航",
  "Refresh state": "刷新状态",
  Dismiss: "关闭提示",
  "Remove the local certificate?": "移除本地证书？",
  "Trust localhost for Office?": "为 Office 信任本地 HTTPS？",
  "This stops the gateway, removes only this app’s certificate from your user certificate store, and deletes its local identity files. Your provider settings and keys are kept.":
    "将停止网关，从当前用户的证书存储中移除本应用的证书，并删除本地证书身份文件。供应商配置与密钥会保留。",
  Cancel: "取消",
  "Remove certificate": "移除证书",
  "Install certificate": "安装证书",
  Connected: "已连接",
  "Connecting…": "正在连接…",
  "Connect gateway": "连接网关",
  "Stored securely": "已安全保存",
  "Generated on save": "保存时生成",
  "Three steps to Office": "三步连接 Office",
  "Bring your model into PowerPoint, Excel, and Word.":
    "让自己的模型接入 Office。",
  "A local HTTPS gateway translates the Claude Office add-in protocol into the provider API you already use.":
    "通过本地 HTTPS 网关，将 PowerPoint、Excel 和 Word 中的 Claude 加载项连接到你选用的模型 API。",
  Live: "运行中",
  Local: "本地",
  "Localhost is trusted": "本地 HTTPS 已受信任",
  "Secure localhost connection required": "需要可信的本地 HTTPS 连接",
  "The gateway can accept Office requests over HTTPS.":
    "网关可通过 HTTPS 接收 Office 请求。",
  "Install the per-user certificate before connecting a provider.":
    "连接模型前，请先为当前用户安装本地证书。",
  Trusted: "已信任",
  "Installing…": "正在安装…",
  "Connect a provider": "连接模型供应商",
  "Enter the API endpoint and model that should answer requests from Office.":
    "填写处理 Office 请求的 API 地址、协议和模型。",
  "API base URL": "API 基础地址",
  "Provider protocol": "供应商协议",
  "Provider model": "供应商模型",
  "Exact model ID": "填写准确的模型 ID",
  "Provider API key": "供应商 API 密钥",
  "Saved securely · enter a new key to replace": "已安全保存；输入新密钥可替换",
  "Paste the provider key": "粘贴供应商密钥",
  "Stored in the system credential vault": "密钥保存在系统凭据库",
  "Saving…": "正在保存…",
  "Save provider": "保存配置",
  "Connect the Office add-in": "连接 Office 加载项",
  "Copy these values into the Gateway sign-in panel in your Claude Office add-in.":
    "将以下信息粘贴到 Claude Office 加载项的 Gateway 登录页面。",
  Use: "使用",
  "format and": "格式和",
  "authentication. The gateway token is separate from your provider key.":
    "认证。网关令牌与供应商密钥不同，请勿添加 Bearer 前缀。",
  "Read the setup guide": "查看使用指南",
  Recheck: "重新检测",
  Stop: "停止",
  "Cancel connection": "取消连接",
  "Advanced gateway settings": "网关高级选项",
  "Office model ID": "Office 模型 ID",
  "Display name": "显示名称",
  "Local port": "本地端口",
  "Timeout (seconds)": "超时时间（秒）",
  "Install the localhost certificate to enable connection.":
    "安装并信任本地证书后即可连接。",
  "Connect sends two short provider checks. Your provider may bill these requests.":
    "连接时会发送两次简短的供应商检测请求，可能产生少量费用。",
  "Requests go from Office to this device and then to":
    "请求由 Office 经本机转发至",
  "your provider": "你的模型供应商",
  ". Document contents are not stored by this client.":
    "。本客户端不会保存文档内容。",
  "Fine tune the bridge": "调整网关行为",
  "These values affect the local listener and how requests are authenticated upstream.":
    "设置本地连接和上游 API 的认证方式。运行期间需先停止网关才能编辑。",
  "Upstream authentication": "上游认证方式",
  "Choose which header the provider expects.": "选择供应商要求的认证请求头。",
  "OpenAI token field": "OpenAI token 限制字段",
  "Some OpenAI-compatible providers still require the legacy field.":
    "部分兼容 OpenAI 的供应商仍要求旧版字段。",
  "Allowed Office origin": "允许的 Office 来源",
  "Only this origin is accepted by the local CORS policy.":
    "本地 CORS 策略只接受此 HTTPS 来源。",
  "Certificate identity": "证书身份",
  "Remove certificate…": "移除证书…",
  "Save advanced settings": "保存高级设置",
  "Setup once, then keep it running": "完成配置后，使用期间保持运行",
  "Office connection guide": "Office 连接指南",
  "The client keeps a small trusted gateway on localhost while your Office add-in is in use.":
    "使用 Office 加载项时，本客户端在 localhost 上运行可信的 HTTPS 网关。",
  "Install the local certificate": "安装本地证书",
  "Install the certificate for your current user and localhost. macOS may show a Keychain confirmation; Windows uses the current user's certificate store.":
    "为当前用户安装 localhost 证书。macOS 可能显示钥匙串确认窗口；Windows 使用当前用户的证书存储。",
  "Connect your model provider": "连接模型供应商",
  "Enter an HTTPS API base URL, select the provider protocol, paste your key, and use the exact model ID from your provider. The client sends two small checks when you connect.":
    "填写 HTTPS API 基础地址，选择协议，粘贴密钥和准确的模型 ID。连接时客户端会发送两次简短检测请求。",
  "Paste the gateway details into Office": "将网关信息粘贴至 Office",
  "Open the Claude add-in in PowerPoint, Excel, or Word, choose Gateway, and paste the URL, gateway token, and Office model ID shown on the Connection page.":
    "在 PowerPoint、Excel 或 Word 中打开 Claude 加载项，选择 Gateway，粘贴连接页显示的地址、网关令牌和 Office 模型 ID。选择 Anthropic 格式和 x-api-key 认证，令牌无需 Bearer 前缀。",
  "Official add-ins": "官方加载项",
  "Get the Office panel": "安装 Office 加载项",
  "Install the compatible Claude add-in from Microsoft Marketplace.":
    "从 Microsoft Marketplace 安装对应应用的 Claude 加载项。",
  "Private by default": "隐私保护",
  "Provider keys live in the system credential vault. This app has no telemetry and does not log document contents.":
    "供应商密钥保存在系统凭据库中。本应用没有遥测，也不会记录文档内容。",
  "Back to connection": "返回连接页",
  Complete: "已完成",
  "Connect first": "请先连接",
  "Gateway URL": "网关地址",
  "Gateway token": "网关令牌",
  "Not configured": "尚未配置",
  "Not connected": "未连接",
  "Starting local gateway": "正在启动本地网关",
  "Checking provider": "正在检测供应商",
  "Certificate needs attention": "请检查本地证书",
  "Ready for Office": "Office 连接已就绪",
  "Needs attention": "需要处理",
  "Provider settings saved securely.": "供应商配置已安全保存。",
  "The gateway is ready for Office.": "网关已就绪，可以连接 Office。",
  "The local gateway stopped.": "本地网关已停止。",
  "Provider check completed.": "供应商检测已完成。",
  "The localhost certificate is trusted.": "本地证书已受信任。",
  "The local certificate was removed.": "本地证书已移除。",
  "Enter an HTTPS API base URL.": "请输入 HTTPS API 基础地址。",
  "Enter an HTTPS API base URL without credentials or query parameters.":
    "请输入不包含凭据或查询参数的 HTTPS API 基础地址。",
  "Enter the exact provider model ID.": "请输入准确的供应商模型 ID。",
  "Enter a provider API key.": "请输入供应商 API 密钥。",
  "Enter a valid Office model ID.": "请输入有效的 Office 模型 ID。",
  "Choose a local port between 1024 and 65535.":
    "请选择 1024 到 65535 之间的本地端口。",
  "Provider timeout must be between 1 and 600 seconds.":
    "供应商超时时间需在 1 到 600 秒之间。",
};
export function t(value: string): string {
  if (language === "en") return value;
  const key = value.trim();
  const translation = zh[key];
  if (!translation) return value;
  return (
    value.slice(0, value.length - value.trimStart().length) +
    translation +
    value.slice(value.trimEnd().length)
  );
}
export function setLanguage(next: Language) {
  language = next;
  try {
    window.localStorage?.setItem("office-gateway-language", next);
  } catch {}
  document.documentElement.lang = next === "zh" ? "zh-CN" : "en";
  window.dispatchEvent(new Event("office-gateway-language"));
}
export function useLanguage() {
  const [value, setValue] = useState(language);
  useEffect(() => {
    document.documentElement.lang = language === "zh" ? "zh-CN" : "en";
    const update = () => setValue(language);
    window.addEventListener("office-gateway-language", update);
    return () => window.removeEventListener("office-gateway-language", update);
  }, []);
  return [value, setLanguage] as const;
}

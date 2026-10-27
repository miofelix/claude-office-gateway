# localhost TLS 证书与信任范围

调研日期：2026-10-27。依据官方标准、平台文档及只读代码检查；未修改代码、证书、信任或用户设置，未执行 UI／原生验证。

**结论：普通 HTTPS 证书及本项目当前的系统信任设置，不能把信任精确限定为 `https://localhost:15724`。** SAN 校验主机身份；端口属于连接目标及 HTTP origin。RFC 9110 §4.3.3 明确说明 HTTPS 授权判定不依赖连接端口，并要求服务器区分不同端口的目标 URI。这不是仅从“API 没有 port 参数”作出的猜测。[R1][R2]

**SAN、URI-ID 与 nameConstraints**

| 机制 | 实际边界 |
| --- | --- |
| SAN `dNSName=localhost` | DNS 名称匹配，不带端口；`localhost:15724` 不能当作该 DNS 名称的合法端口约束。[R1][R3] |
| SAN `iPAddress=127.0.0.1`／`::1` | IP 地址匹配，不带端口；它们与 DNS-ID `localhost` 是不同身份项。[R1][R3] |
| URI-ID `https://localhost:15724` | RFC 9525 §7.2 明确只用 scheme 与 host，其他组件被忽略，因此端口不参与匹配。该 RFC §4.2 也说明 HTTP 并未规定用 URI-ID 表示服务器身份；添加它不会让普通 HTTPS 客户端获得端口约束。[R1] |
| `nameConstraints` | RFC 5280 §4.2.1.10 的 CA 路径约束；DNS/IP 限定名字／地址范围，URI 约束明确只作用于 host，不包含 scheme 或 port。不是完整 URL 白名单，也不能靠放进叶证书限制端口。[R3] |

**“信任一个身份”不等于“信任所有 localhost 证书”，也不等于严格的叶证书固定**

无关密钥产生的另一个自签 localhost 证书，不会仅因同名就从本次安装取得信任；服务器还须证明掌握受认可身份对应的私钥。**标准推论：**若持有本证书私钥，在其他 localhost 端口使用同一证书，且客户端接受其信任链及其他条件，证书主机名检查本身不会因端口改变而失败。这不表示其他本地进程自动获得了该私钥。[R1][R2]

应用内按证书／公钥固定，只接受特定对端身份，与将自签证书登记为系统信任锚，是不同的验证策略。Windows 文档也将 `TrustedPeople` 配合 `CERT_CHAIN_ENABLE_PEER_TRUST` 的直接对端信任，与通常的受信任根链验证区分。本项目使用的是 `Root`，不能称为已实施严格的叶证书固定；这只是语义区分，不是改造建议。[W1][W3]

**应删除“CA:FALSE cannot issue other certificates”的绝对保证**

- RFC 5280 §4.2.1.9 规定 `cA=FALSE` 不应作为证书签名验证的 CA；§4.2.1.3 的 Key Usage 也要求相应签发用途。它们约束验证者如何接受证书，不会在密码学上阻止私钥产生签名。[R3]
- 但 §6.1 将自签信任锚排除在待验证路径之外，§6.1.1 把其名称／公钥作为输入，§6.2 允许平台另加限制。**标准推论：不能仅凭锚证书中的 `CA:FALSE`、缺少 `keyCertSign`、SAN 或 EKU，保证每种信任锚实现都将这些扩展作为对子证书的限制。**[R3]
- 因而，将这一身份放入 Windows `CurrentUser\Root` 或 macOS `trustRoot` 后，不应承诺“只能认可这张叶证书”“绝不能认可它签发的证书”。反过来，也不能宣称两平台必然接受其子证书：Apple 的公开 `SecPVCParentCertificateChecks` 仍包含 CA／Key Usage 检查；具体路径如何构建及应用这些检查，须由对应系统与客户端决定。本次没有验证子证书链的接受结果。[A1][A4][W1]

**平台设置的准确含义**

| 平台／本项目操作 | 能说明什么；不能说明什么 |
| --- | --- |
| macOS：用户级 `add-trusted-cert -r trustRoot -p ssl -s localhost`，写入 login Keychain | Apple 将 SSL 的 `kSecTrustSettingsPolicyString` 定义为 hostname；`SecPolicyCreateSSL` 接收 hostname，源码按 SSL hostname 比较策略字符串。因此此命令表达 SSL／localhost 的使用约束，**没有端口限制**；把字符串改成完整 URL 或 `localhost:15724` 不会变成端口策略（API／源码推论）。此 hostname 条件也不等同于允许 IP 字面量 `127.0.0.1`／`::1`。命令未设置 application 约束；不能称“只供本应用使用”。其他既有信任仍可能影响最终结果。[A1–A4] |
| Windows：`certutil -user -addstore Root` | 将该证书加入当前用户的受信任根存储；不是完整 URL／端口白名单。SSL 策略的 `pwszServerName` 是服务器名称，没有端口参数；结合 RFC，可知本项目此操作未建立端口限制（文档／代码推论）。用户存储位置不代表按应用隔离，也不保证每个浏览器、Office 或 WebView 都使用相同信任策略。[R2][W1][W2] |

**与当前实现的对应关系（仅静态检查）**

- [`certificate.rs`](../src-tauri/src/certificate.rs) 的 `generate_identity()` 配置 SAN localhost／127.0.0.1／::1、自签、`ExplicitNoCa`、DigitalSignature、ServerAuth，未配置 URI-ID、nameConstraints 或端口。`ensure()` 会复用已有证书，不能把新生成参数当作所有旧身份的已验证属性。
- macOS `trusted()` 运行 SSL／localhost 验证；Windows `trusted()` 仅检查对应指纹在 Root 中及日期有效。二者均不证明端口隔离；Windows 这一检查也不是完整 SSL 链策略验证。
- [`gateway.rs`](../src-tauri/src/gateway.rs) 的 `validate_local()` 精确匹配 `localhost:<config.port>`、`127.0.0.1:<config.port>`、`[::1]:<config.port>` 的 Host；有 Origin 时匹配配置白名单。`handle()` 在预检和 `/health` 后对其他路由执行令牌鉴权。**代码推论：**无 Origin 并不绕过令牌；Host／Origin／CORS 不能替代鉴权，也不能收窄其他服务对证书的使用。
- 监听器决定实际绑定地址与端口；Host 是请求声明，不能证明真实绑定。本次检查的 `gateway.rs` 不含监听绑定逻辑，未核验实际监听状态。浏览器 origin 虽包含端口，却不是证书信任范围。[R2]

**建议应用内文案**

安装确认主文案：

> 将本应用的本地 HTTPS 证书加入当前用户信任，用于连接 https://localhost:15724。证书信任不限定端口，也不等于信任所有 localhost 证书。

平台补充：

> macOS：为此证书设置 SSL／localhost 的用户级信任。Windows：将此证书加入当前用户的“受信任的根证书颁发机构”。

若需解释 `CA:FALSE`，使用：

> 新生成的证书标记为非 CA（CA:FALSE），用途为 TLS 服务器认证。作为系统信任锚时，实际限制由系统及客户端的验证策略决定。

上述地址须使用当前配置端口。不要写“证书仅对 localhost:15724 有效”“仅本应用可用”或“CA:FALSE 保证不能签发其他证书”。这些文案依据本调研，不代表已完成对应平台或 Office 客户端验证。

**官方来源**

- [R1 — RFC 9525: Service Identity in TLS](https://www.rfc-editor.org/rfc/rfc9525.html#section-7.2)：§4.2、§6.3–6.5、§7.2；URI-ID 明确忽略 scheme／host 以外组件。
- [R2 — RFC 9110: HTTP Semantics](https://www.rfc-editor.org/rfc/rfc9110.html#section-4.3.3)：§4.3.1 的 origin 三元组；§4.3.3 的端口与私钥说明；§4.3.4–4.3.5 的 HTTPS 身份校验。
- [R3 — RFC 5280: Internet X.509 Public Key Infrastructure Certificate and CRL Profile](https://www.rfc-editor.org/rfc/rfc5280.html)：§4.2.1.3、§4.2.1.6、§4.2.1.9–10、§6.1、§6.1.1、§6.2。
- [A1 — Apple Security: SecTrustSettings.h](https://github.com/apple-oss-distributions/Security/blob/db15acbe6a7f257a859ad9a3bb86097bfe0679d9/trust/headers/SecTrustSettings.h#L106)：SSL hostname、application 约束、`trustRoot`／`trustAsRoot`；多个使用约束以 OR 组合。
- [A2 — Apple Security: SecPolicy.h / SecPolicyCreateSSL](https://github.com/apple-oss-distributions/Security/blob/db15acbe6a7f257a859ad9a3bb86097bfe0679d9/trust/headers/SecPolicy.h#L187)：SSL 验证 API 的 hostname 参数。
- [A3 — Apple security(1): add-trusted-cert](https://github.com/apple-oss-distributions/Security/blob/db15acbe6a7f257a859ad9a3bb86097bfe0679d9/SecurityTool/macOS/security.1#L1285)：用户／管理员域、策略、应用与字符串约束。
- [A4 — Apple Security: SecPolicyServer.c](https://github.com/apple-oss-distributions/Security/blob/db15acbe6a7f257a859ad9a3bb86097bfe0679d9/trust/trustd/SecPolicyServer.c)：`SecPVCContainsString`（SSL hostname 比较）、`SecPVCMeetsConstraint`、`SecPVCParentCertificateChecks`（CA／Key Usage 检查）。开源快照不是对任一本机二进制行为的证明。
- [W1 — Microsoft: System Store Locations](https://learn.microsoft.com/en-us/windows/win32/seccrypto/system-store-locations)：当前用户存储及 Root 的受信任根语义。
- [W2 — Microsoft: HTTPSPolicyCallbackData structure (SSL_EXTRA_CERT_CHAIN_POLICY_PARA)](https://learn.microsoft.com/en-us/windows/win32/api/wincrypt/ns-wincrypt-httpspolicycallbackdata)：SSL 链策略参数与 `pwszServerName`。
- [W3 — Microsoft: CertGetCertificateChain function](https://learn.microsoft.com/en-us/windows/win32/api/wincrypt/nf-wincrypt-certgetcertificatechain)：`CERT_CHAIN_ENABLE_PEER_TRUST` 与 `TrustedPeople` 的直接对端信任。

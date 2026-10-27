# Windows 证书检查控制台闪现

修复版本：`0.2.0-beta.2`。日期：2026-10-27。

## 原因与修复

用户报告安装证书后 PowerShell 窗口一直出现和消失。界面状态每 5 秒刷新并检查证书，网关运行时另有每 15 秒的信任检查。Windows 实现通过 PowerShell 查询当前用户证书存储，旧启动方式没有显式禁止控制台分配；桌面 GUI 进程运行控制台程序时可能短暂弹出窗口。

`certificate.rs` 中的后台输出封装统一设置 Windows `CREATE_NO_WINDOW`（`0x08000000`），覆盖 PowerShell 检测及 certutil 安装／卸载。标准输出、标准错误和进程退出状态保持可读，原有失败处理保留。macOS 不设置此 Windows 标志。

证书信任检查继续按原周期执行；仅改变后台进程的显示方式。系统自身需要的证书确认不由此标志绕过。

## 原生回归

测试直接调用生产使用的启动封装，以 `GetConsoleWindow()` 检查控制台句柄。普通 CI 由无交互桌面的父进程启动，旧实现也可能得到 0，因此不能只凭普通子进程测试宣布修复。

增加正向对照，明确请求 `CREATE_NEW_CONSOLE`，先断言环境能分配非零控制台；再要求后台封装覆盖该请求且返回 0。旧代码在 Actions run `40468310260` 的 Windows 原生回归中失败：实际句柄 `327904`，预期 `0`。正向对照和其余 22 项测试均通过。

修复后继续运行同一回归，并检查连续三次 PowerShell 启动无控制台，以及退出码 7 和标准错误内容仍原样返回。结果将在发布验证完成后补入。

CI 验证的是进程控制台分配与错误语义，用户桌面上的可见窗口、原生证书确认和 Office 交互仍需实际 Windows 设备复测。

官方参考：[Windows process creation flags](https://learn.microsoft.com/en-us/windows/win32/procthread/process-creation-flags)。

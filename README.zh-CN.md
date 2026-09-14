# rnc：面向 Windows 与 CTF 的 Netcat

[English](README.md) · [CI](https://github.com/jacek4yang/rnc/actions/workflows/ci.yml) · [贡献指南](CONTRIBUTING.md)

使用 Rust 编写的原生 `nc.exe`：支持 TCP/UDP、IPv4/IPv6、DNS、全双工字节流、
TCP 半关闭和 Ctrl+C 退出。发行构建静态链接 C 运行时，无需安装 Rust、Python
或 VC 运行库即可使用。

```console
nc 127.0.0.1 8080
nc -l 8080
nc -u 127.0.0.1 8080
nc -6 ::1 8080
nc 127.0.0.1 8080 --encoding utf-8
nc 127.0.0.1 8080 --encoding gbk
nc 127.0.0.1 8080 --encoding gb18030
nc 127.0.0.1 8080 --encoding auto
```

## 编译和使用

开发需要最新稳定版 Rust MSVC 工具链与 Visual Studio C++ Build Tools：

```powershell
cargo build --release --locked
.\target\release\nc.exe --help
```

把 `nc.exe` 放到 PATH 中的目录，即可直接执行 `nc host port`。也可在项目根目录
执行 `cargo install --path . --locked`。

GitHub Actions 成功运行后，会提供 Windows ZIP 与 SHA-256 校验文件，位于运行
页面的 Artifacts 区域，保留 14 天。这些是开发构建，不代表正式 Release。

## 字节和编码

- 网络传输层只处理原始字节，不解码文本。
- 真正的 Windows 控制台使用 `ReadConsoleW` / `WriteConsoleW`，避免依赖当前代码页。
- 支持 UTF-8、GBK/CP936、GB18030，能处理被多次 socket read 拆开的字符。
- 默认 `auto` 不会因为 ASCII 欢迎信息提前锁定编码；检测只在确定编码前进行。
- 自动检测存在歧义，不保证识别所有数据。已知服务端编码时应显式指定。
- stdin/stdout 只要是管道或重定向，该方向就保留原始字节，即使指定了 `--encoding`。
- `--raw` 完全关闭应用层编码和换行转换。真正的控制台仍可能按操作系统设置解释字节。

在 **cmd.exe** 中，下列用法经过全字节值测试：

```bat
nc 127.0.0.1 8080 > output.bin
nc 127.0.0.1 8080 < input.bin > output.bin
type input.bin | nc 127.0.0.1 8080
```

PowerShell 的 `type` 是读取文本的 `Get-Content` 别名；旧版 PowerShell 的重定向
也可能转换编码。二进制数据建议在 PowerShell 中调用 cmd：

```powershell
cmd /d /c 'type input.bin | nc 127.0.0.1 8080 > output.bin'
```

## EOF 与退出

TCP stdin EOF 只关闭发送方向，继续接收；对端发送 FIN 后，本地仍可发送。
默认等待两个方向都结束。控制台按 Ctrl+Z 后回车表示 EOF，Ctrl+C 立即结束会话。

`-q1` 表示 stdin EOF 后等待一秒退出；`-w5` 表示连接建立或网络空闲超时五秒。
UDP 没有 EOF，应使用 `-q`、`-w` 或 Ctrl+C 结束。UDP 监听模式服务第一个发送方；
TCP 监听模式服务一个连接。完整行为与限制见 [英文 README](README.md)。

## 开发与测试

main 受保护，所有修改通过 PR；Windows 和 Linux CI 必须通过，仅允许 squash 合并。
个人维护模式暂不强制另一人的审批，CODEOWNERS 用于分配评审。

```powershell
.\scripts\verify.ps1
.\scripts\package.ps1
```

测试脚本需要 Python 3.10+，程序本身不需要。测试覆盖真实 Windows Unicode
控制台、cmd.exe 管道/重定向、IPv4/IPv6、TCP/UDP、半关闭、大文件和编码分片。

[性能记录](docs/PERFORMANCE.md) · [验证记录及已知限制](docs/VALIDATION.md) · [安全报告](SECURITY.md)

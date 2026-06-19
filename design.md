Goal: 参考https://github.com/luojiahai/code-by-wire的设计，实现一款windows上的IDE，能连接remote linux machine，像vscode 如何连接remote linux server的过程一样(类似 vscode server bootstrap)，自动下载需要的server/code/binary及其他任何远程开发所需到linux server上，并启动指定agent的cli instance（如果remote没安装过agent环境，可以自己安装），将其输出渲染到windows上的IDE前端来并支持交互

这里类vs code server boostrap要完成的功能包括

连上远端 → 安装/启动 remote agent host → 本地前端连它并管理 Claude/Copilot CLI 会话

整体架构

总体架构（分层）
A. Frontend（Windows，现有 renderer 为主）
Terminal UI（xterm）
Session 列表/状态
CLI 状态面板（未来支持local and “远端 CLI 状态”，现阶段以remote为主）
通过 TerminalApi 调用，不感知底层是本地还是远端
B. Desktop Core（Windows，main/preload）
连接管理（SSH config、认证、重连）
Remote bootstrap（上传/下载 remote host）
Transport 适配（IPC / SSH / WS 三种可插拔）
维持现有 window.api.terminal.* 对前端兼容
C. Remote Agent Host（Linux，新模块）
Session Registry（session->pty->pid->tool）
PTY Worker（启动/管理 claude/copilot cli 等）
Tool Installer（下载、校验、升级 CLI）
Probe 服务（版本/auth/path 检测）
输出流控（ack/backpressure）
审计日志/指标
D. Optional Control Plane（后续可加）
多机器管理
统一策略（允许工具列表、版本策略）
资产与升级策略
2. 关键数据流（核心链路）
用户在前端点击“新会话”
Frontend 调 terminal.spawn(req)
Desktop Core 将请求转给 Remote Host（SSH 隧道内的 WS/stdio 协议）
Remote Host 创建 PTY，启动 claude --session-id ...
PTY onData 按 chunk 推送 terminal:data
Frontend xterm 渲染，按消费量回 ack
Remote Host 根据 ack 调 pause/resume（高低水位）





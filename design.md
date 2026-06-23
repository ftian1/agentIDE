# Remote AI IDE — Design Document

Goal: 参考 https://github.com/luojiahai/code-by-wire 的设计，实现一款 Windows 上的 IDE，能连接 remote Linux machine，类似 VS Code Remote-SSH 的 bootstrap 过程，自动下载/部署所需组件到 Linux server 上，并启动指定 agent 的 CLI instance，将其输出渲染到 Windows IDE 前端并支持交互。

---

## 1. 总体架构（分层）

### A. Frontend（React + TypeScript + Tauri v2）
- Terminal UI（xterm.js）
- ActivityBar 侧边栏导航
- AgentManager（连接管理 + Docker 容器支持）
- FileBrowser（远程文件浏览，默认展开）
- SessionManager（会话管理中心）
- StatusBar + RightPanel（SessionDetail）
- 通过 TerminalApi 调用，不感知底层是本地还是远端

### B. Desktop Core（Rust + Tauri v2）
- 连接管理（SSH config、认证、重连）
- Remote bootstrap（检测平台 → 上传 agent → 启动 agent）
- Transport 适配（IPC / SSH channel 两种）
- 维持 `window.api.terminal.*` 对前端兼容
- SQLite 持久化（连接记录、session 历史、settings）

### C. Remote Agent Host（Rust，Linux 二进制）
- Session Registry（session → PTY → pid → tool）
- PTY Worker（启动/管理 claude/copilot CLI）
- Tool auto-install（检测 → npm install → 验证）
- Docker 支持（`docker exec -it <container> <cmd>`）
- Probe 服务（版本/auth/path 检测）
- 输出流控（ack/backpressure）

### D. Optional Control Plane（后续）
- 多机器管理、统一策略、资产与升级策略

---

## 2. 术语定义

| 术语 | 含义 | 备注 |
|------|------|------|
| **remote-agent-host** | 我们写的 Rust 二进制，PTY 管理 + 多路复用代理 | 上传到远程机器，~3.4MB |
| **Agent CLI** | Claude Code CLI / GitHub Copilot CLI 等 AI 工具 | 远程机器上通过 PATH 找到或自动安装 |
| **Bootstrap** | 检测平台 → 上传 remote-agent-host → chmod → 启动 | 首次连接执行，后续跳过 upload |
| **Transport** | 双向 MessagePack 帧通道 | IPC（本地）或 SSH channel（远程） |
| **Session** | 一个 PTY 内运行的 CLI 实例 | 由 remote-agent-host 管理 |

---

## 3. 核心数据流

```
用户在 AgentManager 点 "Confirm & Connect"
  → Frontend 调 connectionStore.connect(req)     [SSH 参数 + 可选 container]
  → Rust connect command:
      1. 保存到 SQLite
      2. SSH 连接到远程机器 (russh)
      3. Bootstrap: detect → upload remote-agent-host → start
      4. 建立 SshChannelTransport 双向通道
      5. 存入 connection_transports map
  → Frontend 调 sessionStore.spawn(connId, req)
  → Rust spawn_session command:
      查找 connection_transports[connId]
      → 发送 ProtocolMessage::SpawnSession { tool, container?, ... }
  → remote-agent-host 收到消息:
      1. ensure_tool_installed(): which → 没找到就 npm install
      2. 如果有 container: docker exec -it <ctr> <cmd>
      3. 在 PTY 中启动 CLI
  → PTY onData 按 chunk 推送 terminal:data
  → Frontend xterm 渲染
```

---

## 4. 设计决策记录

### 4.1 SSH Channel 是二进制安全的（非"只能传文本"）

**结论:** SSH exec channel 传输原始字节流（raw bytes），是二进制安全的。Wire format 为 `[4-byte BE length][MessagePack ProtocolMessage]`。

**Bootstrap 的真正目的:** 把 remote-agent-host 二进制安装到远程机器上并启动，不是"解决 SSH 只能传文本的限制"。

**remote-agent-host 为什么用 `--mode stdio`:** SSH exec channel 把远程进程的 stdin/stdout 连成本地双向字节管道，这正是 stdio 模式需要的。未来可能有 `--mode tcp` 或 `--mode unix-socket` 用于非 SSH 场景。

### 4.2 remote-agent-host 与 Agent CLI 分离

**结论:** remote-agent-host（我们上传的代理）和 Claude/Copilot CLI（远程已安装或自动安装）是两个独立的东西。

- remote-agent-host: 多路复用、PTY 管理、事件推送 — 一条 SSH channel 承载多个 session
- Agent CLI: 远程机器上通过 `PATH` 找到，不存在则 `npm install -g` 自动安装

**没有 agent 时的架构:**
```
Desktop Core ──SSH exec 1── bash (PTY)
              ──SSH exec 2── claude (PTY)
              ──SSH exec 3── copilot (PTY)
```
每个 session 一个 SSH channel，无法复用、无法集中管理。

**有 agent 后的架构:**
```
Desktop Core ──SSH exec (唯一一条)── remote-agent-host
                                       ├── bash (PTY)
                                       ├── claude (PTY)
                                       └── copilot (PTY)
```

### 4.3 Docker 容器支持 — 方案 A（Agent 在宿主机，CLI 在容器）

**结论:** 选择方案 A。remote-agent-host 跑在宿主机，`handle_spawn` 检测到 container 参数时自动包裹 `docker exec -it <container> <cmd>`。

**备选方案 B（agent 进容器）** 复杂度更高：需要把 agent 传进容器、容器内需要更多依赖。当前不采用。

**协议变更:** `SpawnSession` 新增 `container: Option<String>` 字段。

### 4.4 连接持久化策略

**结论:** 双层缓存。
- **SQLite** (`%APPDATA%/remote-ai-ide/app.db`): 存所有连接过的机器配置，启动时恢复到内存
- **localStorage** (`agentMgr:*`): 缓存上次表单输入值（host/port/user/agentType/container），密码不缓存

### 4.5 serde 字段命名规范

**结论:** 方案 A — Rust struct 加 `#[serde(rename_all = "camelCase")]`，前端保持 camelCase，Rust 保持 snake_case，serde 自动转换。这是 Rust/Tauri 生态的标准做法。

### 4.6 Per-Connection Transport Map

**结论:** 用 `DashMap<String, Arc<dyn Transport>>` 存储每个连接的 transport，替代单一 `agent_transport`。`spawn_session` 优先按 `connection_id` 查找，fallback 到本地 agent。

### 4.7 远程平台支持

| 平台 | 支持状态 | 原因 |
|------|---------|------|
| Linux x86_64 | ✅ 已支持 | remote-agent-host 有预编译二进制 |
| Linux aarch64 | ⚠️ 架构就绪 | 需真实二进制（当前 12 字节 stub） |
| macOS | ❌ 未支持 | portable-pty 支持但无预编译二进制 + bootstrap 命令差异 |
| Windows | ❌ 不可行 | 无 POSIX PTY（需 ConPTY）、无原生 SSH server |

### 4.8 UI 架构

```
ActivityBar (48px)
├── Agent Manager   (Bot 图标，最上)    → AgentManagerPanel
├── File Explorer   (FolderTree 图标)   → FileBrowser (默认展开)
├── Session Manager (MonitorPlay 图标)  → SessionManagerPanel
├── Search          (Search 图标)
├── Source Control  (GitBranch 图标)
└── Settings        (Settings 图标)
```

- AgentManager: 两级面板 — Overview（已保存连接+session 列表） + Connect Form（SSH 信息 + Docker 容器选项）
- FileBrowser: 默认展开所有目录，无需 collapse
- SessionManager: 统一 session 管理中心（active + ended + spawn bar）

### 4.9 Auto-Install 逻辑

`ensure_tool_installed()` 在 remote-agent-host 的 `handle_spawn` 中调用：
1. `which <tool>` 检查是否已安装
2. 未找到 → 检查 npm 是否可用
3. `npm install -g <package>` 自动安装
4. 再次 `which` 验证
5. 失败则返回友好错误信息

| Tool | Command | Install Package |
|------|---------|----------------|
| Claude | `claude` | `@anthropic-ai/claude-code` |
| Copilot | `gh` | `@github/copilot-cli` |
| Custom | 用户指定 | 无自动安装 |

**2026-06 修复** — 三个导致 auto-install 失败的问题：
1. **Node.js 下载到 `/tmp`**：部分服务器 `/tmp` 磁盘满 → 改为下载到 `~`（home 目录，通常有更多空间）
2. **npm 拷贝方式错误**：旧脚本 `cp bin/npm ~/.local/bin/npm` 单独拷贝 shell wrapper，但 npm 依赖 `../lib/node_modules/npm/` 的相对路径 → 改为 `tar -xf ... -C ~/.local --strip-components=1` 完整提取
3. **损坏 npm 的检测**：之前只要 `~/.local/bin/npm` 存在就认为 npm 可用 → 现在增加 `npm --version` 验证，如果返回错误则自动重新安装

**测试覆盖补齐**：新增 `live_probe_test.rs` 覆盖 Probe + Install API，完整测试 `ensure_tool_installed()` → `ensure_nodejs()` → curl/wget download → tar extract → npm install 路径。

### 4.10 SSH 认证支持

- ✅ Password 认证
- ✅ SSH Key（ed25519, RSA）
- ✅ SSH Agent forwarding
- ❌ known_hosts 验证（当前接受所有服务器密钥）
- ❌ SSH config 文件解析（`~/.ssh/config`）
- ❌ Jump host / bastion 支持

### 4.11 SshChannelTransport：双任务死锁 → 单 Actor 设计

**问题：** 原设计用两个 tokio task（reader + writer）共享 `Arc<Mutex<Channel>>`。
Reader 在 `channel.wait()` 期间持有 mutex，如果此时 writer 需要发送新请求（如
SpawnSession），writer 阻塞在 mutex 上无法发送。Reader 的 `wait()` 永远等不到
响应数据 → **死锁**。

**修复：** 改为 **单 actor task** 拥有 Channel，用 `tokio::select!` 竞态处理读写：
```rust
loop {
    tokio::select! {
        frame = write_rx.recv() => { channel.data_bytes(frame).await; }
        msg = channel.wait() => { /* decode + push to read_rx */ }
    }
}
```
- 同一时刻只有一个 future 持有 Channel 的 mutable borrow
- `select!` 在一个分支完成时 drop 另一个分支的 future（释放 borrow）
- 无需 Mutex，无死锁风险

**验证：** live_ssh_test + live_terminal_test 全部通过（之前死锁卡死 30s+）。

### 4.12 Upload 优化：base64 chunked echo → raw binary channel write

**问题：** 旧方案将 1.7MB 二进制 → base64 编码（膨胀 33% ≈ 2.3MB）→ 按 64KB
分 ~35 chunks → 每个 chunk 一次独立 SSH exec（`echo '...' | base64 -d >/>>`）
→ chunk 间 sleep 350ms。

瓶颈：
- 35 次 SSH exec 往返延迟叠加
- 34 × 350ms ≈ 12s 纯 sleep
- Base64 额外 33% 数据传输
- `echo '...'` 潜在的 shell 转义风险

**新方案：** 单次 SSH channel 原始二进制写入：

1. `mkdir -p && rm -f`（一次 exec_remote）
2. 打开 exec channel → `cat > /path/to/agent`
3. `channel.data_bytes(raw_binary)` — 单次操作写入全部 1.7MB
4. `channel.eof()` → 等待 exit status
5. `stat -c%s` 验证文件大小（一次 exec_remote）
6. `chmod +x`（一次 exec_remote）

总计：**3 次 exec_remote + 1 次 data_bytes**，消除 base64/chunk/sleep。

**效果：** 总测试时间从 45s+（含死锁）降到 ~2s，upload 部分从 ~35 RTT + 12s sleep
降为 ~1 RTT。

**权衡：** 如果网络极不稳定，单次大数据写入失败需要重传整个文件。但 SSH 基于 TCP，
数据完整性由传输层保证；且 1.7MB 对现代网络微不足道。若以后 binary 增长到 50MB+，
可以考虑加入分片 + resume 能力。

### 4.13 UI Layout 与底层代码分离（三层 + 槽位化布局）

**背景：** Penpot 初稿（page 2「Agent IDE — GUI Layout」）描述了一个比当前实现丰富得多的
界面：顶部菜单栏 + 全局搜索、左栏文件树 + 审批流队列、中间代码编辑器 + 右侧 Claude Code
Agent 面板（THOUGHT/ACTION/OBSERVATION）、底部多 Tab（Agent Stdout / MCP 日志 / File Sync
/ 问题 / 端口）+ Agent 输入框、以及一组下拉菜单（dd-*）和模态框（modal-*）。目标是让
**布局可以独立迭代而不触碰业务逻辑** —— 即"底层代码 ↔ UI layout 分离"。

**决策：** 采用三层架构 + 一份声明式布局 schema，并以 Design Tokens 作为贯穿三层的视觉契约。

**第 1 层 — 逻辑/状态层（`stores/*` + `api/*`）**
- 已是无 JSX 的纯逻辑，保持纯净。
- **唯一约束：只有这一层可以 import Tauri / 协议类型。** 组件和 hook 都不直接碰 Tauri。

**第 2 层 — View-Model 层（headless hooks，关键解耦层）**
- 每个 region 一个 hook：`useFileTree()`、`useApprovalQueue()`、`useAgentConversation()`、
  `useEditorTabs()`、`useBottomPanelTabs()`、`useConnectionStatus()` 等。
- hook 内部 select store 数据 + 暴露 action，**不含任何 JSX**。
- 组件**永远不直接 `useSessionStore` 等 store**，只消费这些 hook。
- 收益：换布局时逻辑层零改动。当前耦合点是 `App.tsx` 直读 store（如
  `useSessionStore((s)=>s.sessions)`），第一步即抽出这些读取。

**第 3 层 — 布局/展示层（纯组件 + 声明式槽位）**
- Penpot 的每个 region 映射成一个具名槽位，用一份布局 config 描述"谁放哪"，
  `AppShell` 按 config 渲染：
  ```
  { topBar:  <MenuBar/> <GlobalSearch/> <ConnStatus/>,
    left:    { primary: <FileTree/>, secondary: <ApprovalQueue/> },
    center:  { tabs: <EditorTabs/>, body: <CodeEditor/> },
    right:   <AgentPanel/>,
    bottom:  [<AgentStdout/>, <McpLogs/>, <FileSync/>, ...],
    overlays:[<KernelModal/>, ...] }
  ```
- 迭代布局 = 改这份 config，不动 hook 和 store。
- 现有 `AppShell` 已是 slot 风格（`sidebar/bottomPanelContent/rightPanel/children`），
  只需扩出 `topBar` 槽位、把 right 改成 Agent 面板。

**贯穿三层的桥 — Design Tokens**
- Penpot 的颜色/间距/字号 → CSS 变量（现有 `bg-bg-primary`、`text-text-secondary`、
  `border-border` 即语义 token，继续沿用）。
- 约定：**Penpot 拥有视觉 token，代码只消费。** 设计改色不需要改组件。

**命名 1:1 对应**
- Penpot board 名 = React 组件名（`Activity Bar`→`ActivityBar`、`审批流队列`→
  `ApprovalQueue`、`Claude Code`→`AgentPanel`）。看设计稿即知改哪个文件。

**权衡：** 不做 VS Code 那种可序列化的插件 contribution 框架 —— 对单体 IDE 过重。
先用"轻量槽位 AppShell + headless hooks"拿到 ~90% 解耦收益；未来真要插件化再升级。

**执行顺序：** 先做第 2 层抽取（风险最低、立即解耦），再扩展第 3 层槽位以承载新 region。

### 4.14 设计稿缺失 region 的补全（菜单栏 / 审批队列 / Agent 面板 / 编辑器 / 设置弹窗）

**背景：** 按 §4.13 的三层架构，把 Penpot 初稿缺失的 region 全部补齐。前端先行、后端跟进，
对外契约通过新增协议消息 + Tauri 事件中继实现。

**前端（三层，全部对照设计稿文案）：**
- 顶部 chrome：`MenuBar`（5 个下拉 File/Agent/Git/View/Help）+ `GlobalSearch` + `ConnectionBadge`，
  经 `useMenuCommands` 调用 layoutStore 的 toggle/openModal。新增 `topBar` 槽位（AppShell）。
- 审批流队列：`approvalStore`(+`initApprovalListeners`) → `useApprovalQueue` → `ApprovalQueue`/`ApprovalCard`，
  监听 `approval:request` 事件，`respond_approval` 命令回传决策。
- Agent 面板：`agentStore`(+`initAgentListeners`) → `useAgentConversation` → `AgentPanel`/`AgentTurn`/`AgentInput`，
  渲染 THOUGHT/ACTION/OBSERVATION 块，监听 `agent:event` / `agent:status`，`send_agent_message` 命令发消息。
  挂在 AppShell 新增的 `agentPanel` 右栏槽位。
- 代码编辑器：`fileBufferStore` + `useFileBuffer` + `CodeEditor`/`EditorBreadcrumb`，Monaco 可编辑，
  Ctrl+S 经 `read_file`/`write_file` 命令读写远程文件（base64 安全传输 + 临时文件原子 rename）。
  `useWorkspaceView` 新增 `{kind:'file'}` 分支。
- 底部面板：`PanelTabBar` 改为设计稿 5 Tab（Agent Stdout / MCP 日志 / 文件同步 / 问题 / 端口）。
  `agentLogStore`(+`initAgentLogListeners`) 聚合 `agent:event`/`session:event` 为日志流喂 `AgentStdout`，
  其余为脚手架空态。
- 设置弹窗：`agentSettingsStore` + `AgentBackendModal`（Claude/Aider/MCP 三 Tab），经 `load/save_agent_settings`
  命令持久化到 SQLite `settings` 表（key=`agent_settings` 的 JSON blob）。

**后端：**
- 协议（`shared-protocol`）：新增 `AgentEvent`、`ApprovalRequest`、`ApprovalResponse` 三个 ProtocolMessage 变体
  + `AgentEventKind`(Thought/Action/Observation) / `ApprovalDecision`(Allow/AllowAll/Reject) 类型 + round-trip 测试。
- 流解析（`remote-agent-host/src/agent_parse.rs`）：纯增量解析器 `AgentStreamParser::push_line`，把 Claude Code
  stream-json（thinking→Thought、tool_use→Action、tool_result→Observation、control_request/can_use_tool→ApprovalRequest）
  转为协议消息。在 PTY reader 线程按行喂入（仅 Claude/Custom 工具，bash 不解析），配 9 个离线单测。
- 命令：`files::read_file/write_file`、`settings::load/save_agent_settings`、`approval::respond_approval`、
  `session::send_agent_message`；relay loop 新增 `AgentEvent`→`emit("agent:event")`、
  `ApprovalRequest`→`emit("approval:request")` 两个臂；`server.rs` 新增 `ApprovalResponse` 分发 → 写 PTY stdin。

**契约决策：** 决策字符串前端用 camelCase（`allowAll`），`ApprovalDecision` 用 `#[serde(rename_all="camelCase")]`
对齐；`AgentEventKind` 的 Debug lowercased（`thought`/`action`/`observation`）作为事件 `kind` 字段，匹配前端联合类型。

**不可本地验证项：** agent 流解析的端到端正确性需 live agent CLI；read/write_file、respond_approval 的真实远程
行为需 live SSH。本地仅保证编译 + 离线单测（样例 JSON / base64 round-trip）。底部 MCP/端口/文件同步面板暂为空态脚手架。

---

### 4.15 ActivityBar 对齐设计稿图标集（采用初稿 IA）

**背景：** 左侧 `ActivityBar` 此前用一组随手挑的 lucide 图标（Bot/FolderTree/MonitorPlay/Search/GitBranch/Settings），
与 Penpot 初稿 `ActivityRail` 的图标矢量在**顺序、字形、布局**三方面都不一致。经与用户确认，选择「完全采用初稿 IA」
而非仅重新换皮。

**初稿矢量字形（自 SVG path 几何反推，自上而下）：** folder, bot, shield-check, sun, wrench, git-branch；
外加一个 gear **单独固定在底部**（初稿中 `rail-settings` 组的 y 远低于上方簇）。通知 badge 落在 **shield**（=审批）上。

**实现映射：**
- 顶部组：Folder→Explorer、Bot→Agent Manager、ShieldCheck→Approvals(新)、Sun→Session Manager、
  Wrench→Tools(新)、GitBranch→Source Control。
- 底部固定：Settings(gear)→Settings，用 flex `flex-1` spacer 推到底。
- badge：ShieldCheck 显示 `useApprovalQueue().pendingCount`，GitBranch 保留待处理变更数。
- Search 移出侧栏 rail（初稿中搜索在 TitleBar 的 `cmd-search` 命令框里）；`ActivityId` 仍保留 `'search'` 以兼容
  `useMenuCommands` 等既有调用，只是不再从 rail 可达。

**取舍：** shield-check 与 wrench 在底层代码无对应面板，故新增 `'approvals'` / `'tools'` 两个 `ActivityId`。
`approvals` 直接复用既有 `ApprovalQueue` 组件（它本就是自洽的侧栏面板，且仍持续挂在侧栏底部）；`tools` 为空态
脚手架（同 `SettingsPanel` 风格）。字形语义（盾/扳手）与各面板实际功能未必精确对应，属初稿对齐而非功能定义。

---

### 4.16 启动白屏闪烁消除（隐藏窗口 + 原生深色背景 + 首帧后显示）

**问题：** 启动 exe 时窗口先闪一下、白屏 1~2 秒后才进入深色首页。

**根因：** 窗口此前以 `visible:true` + `maximized:true` 立即创建，且**未设置原生背景色**。启动时序为
①OS 创建窗口→绘制白色；②WebView2 冷启动（Windows 上约 1~2s）→其表面默认也是白色；③JS bundle 加载、
React 挂载后 `index.html` 内联的 `#0d1117` 才生效。`index.html` 的深色样式正确，但只能在最慢的步骤**之后**才能上色，
所以白屏覆盖了整个冷启动窗口期。

**方案（三层，缺一不可）：**
1. `tauri.conf.json` 窗口加 `"backgroundColor": "#0d1117"` —— 让原生窗口与 WebView2 表面从第一帧就是深色，单这一项即可消除白闪。
2. 窗口加 `"visible": false` —— 隐藏创建，避免 maximize 尺寸跳动与冷启动期残留闪烁。
3. 前端 `main.tsx` 在 React 首帧合成后（双 `requestAnimationFrame`）调用 `getCurrentWindow().show()` 揭示窗口。

**安全网：** `lib.rs` setup 内 spawn 一个 3s 兜底定时器，若届时窗口仍隐藏（前端崩溃/加载失败）则强制 `show()`，
确保用户绝不会卡在无窗口状态。需在 `capabilities/default.json` 放开 `core:window:allow-show` 权限。

**取舍：** 选择「前端首帧后显示 + Rust 兜底」而非纯 Rust 端 `on_page_load` 显示——前者能保证 React 真正绘制完成
（而非仅页面 DOM ready），视觉上更干净；Rust 兜底则覆盖前端异常路径。

---

### 4.17 Bootstrap 上传提速（关闭 Nagle + 融合 SSH 往返）

**问题：** SSH 连接后，1.8MB 的 remote-agent-host 上传到远端要 ~8s。

**根因（带宽不是瓶颈）：** 1.8MB / 8s ≈ 228 KB/s，远低于任何真实链路带宽。真正原因是 `russh::client::Config`
默认 `nodelay: false`，即 **TCP Nagle 算法开启**。russh 把数据切成 32KB SSH 包，Nagle 与接收端 delayed-ACK
互相等待，每包产生 ~40~200ms 停顿，~57 个包累积成 8 秒。这是 SSH 批量传输慢的经典症状。

**方案：**
1. `ssh.rs::connect` 设 `config.nodelay = true` —— 关闭 Nagle，每个 SSH 数据包立即发出。**这是主要提速点**，
   预期把上传从 ~8s 降到接近"带宽下限"（局域网内亚秒级）。
2. 融合上传往返：原 `upload_agent` 用 4 个独立 SSH 通道（mkdir+rm / cat 写入 / stat 校验 / chmod），
   高延迟链路上各 1 个 RTT。改为：
   - 新增 `ssh::upload_raw_cmd(session, data, command)`，允许把 `mkdir -p dir && cat > tmp` 作为上传命令，
     **mkdir 与数据写入合并到同一通道**；`upload_raw` 改为它的薄封装。
   - 上传到 `agent.tmp`，随后用**单条** exec 完成 `stat 校验 + 原子 mv + chmod +x + echo OK`。

**取舍：**
- 上传到临时文件再 `mv -f` 原子替换：避免部分上传（中断/尺寸不符）覆盖掉一个可用的旧二进制。
- 未做 gzip 压缩传输（二进制 gzip 后约 780KB，可省 ~57%）：远端解压需额外依赖/命令且增加复杂度，
  关闭 Nagle 后上传已不再是瓶颈，故暂不引入。若未来跨公网大延迟场景仍慢，可在 `upload_raw_cmd` 端用
  `gzip -dc > tmp` 配合本地压缩字节实现，已为此预留了「自定义上传命令」的接口。
- 「start claude + 读取 session」剩余的约 10s 主要是 `claude` CLI 自身的 Node.js 冷启动（不受本项目控制）
  与 spawn 链路上 `recv()` 的 50ms 轮询间隔叠加；前者无法在本层优化，后者单次仅数十 ms、且改动会牵动
  spawn 超时计数（600×50ms=30s）逻辑，风险不对等，故本次不动。

---

### 4.18 三区工作台：行内 Patch 预览 / 终端右置 / 上下文文件 / 状态栏监视器

一次性落地的四块工作台能力。整体布局定型为 **编辑器（中）· Agent 终端（右）· 状态栏（底）**。

**① 行内 Patch 预览 + 行侧 ✓/✗（Staged Changes）**
- 放弃 Monaco 原生 `DiffEditor`（并排、只读，且**无法在行侧挂接受/拒绝按钮**），改为**自研行内 diff 渲染**。
- 新增 `lib/diff.ts`：纯函数 LCS 行级 diff，产出 unified 行列表 + hunk 列表；`reconstruct(old, hunks, decisions)`
  按「每个 hunk accepted→用新行，其余（rejected/pending）→保留旧行」重建文件内容——即"pending=已暂存未落盘"。
  含 4000 行规模保护（超限退化为整体替换，避免卡 UI）。已用 19 条断言离线验证（accept/reject/pending、
  多 hunk 部分接受、纯增/纯删、结尾换行保留、accept-all==new）。
- `codeChangeStore` 增 `hunkDecisions` + `setHunkDecision` + `applyAcceptedHunks`；落盘复用既有 `write_file`
  命令（base64 + 临时文件原子 rename）。落盘后把 accepted 内容折叠进 `oldContent`，使已接受 hunk 收起、
  剩余 pending hunk 基于当前文件重新 rebase。
- **取舍：** 自研 diff 比接 Monaco diff 装饰更可控，且纯逻辑可离线单测（本环境无法跑浏览器/Monaco）。

**② 终端右置**
- `useWorkspaceView` 删除 `terminal` 这一中心视图——中心列**仅编辑器**（文件 buffer + patch 预览）。
- 新增 `AgentTerminalColumn`（右列）= 顶部栏（含 ③ 下拉）+ `TerminalPane`。复用既有 `agentPanelVisible`
  开关与 `AppShell.AgentColumn` 槽位；原右列的 `AgentPanel`（对话）暂无槽位（保留组件，未删）。

**③ Show Active Files 上下文一览**
- 数据来源：从 agent 的 tool_use 流派生。`agent_parse` 给 `Read` 也补了 `"Read <path>"` 标签（原先落到 generic 分支）。
- `contextFileStore` 监听 `agent:event` 的 Action 块，用 `parseActionLabel` 解析 `Read/Edit/Write <path>`，
  按 session 维护上下文文件集合（edit/write origin 优先级高于 read）。
- **"点 x 踢出"语义（已与用户确认）：先 UI 移除，预留升级口**——`kickFile` 是独立 store action，当前只把路径加入
  `kicked` 集合从展示中过滤；注释标注了升级点（未来在此再向 agent 发指令，UI 契约不变）。Claude Code 无标准 API
  在会话中驱逐某文件，故不做"真驱逐"。

**④ 状态栏：节点 + Agent 状态 + 远程性能监视器**
- 节点 + Agent 状态：纯前端派生。`deriveAgentActivity` 从 session 末尾 Action 块推 `[Executing Tool: Bash]` 等；
  工具名映射 `claude→Claude Code`。
- **远程性能采集（已与用户确认：复用 SSH session 定时 exec）**：新增 `connection/perf.rs`，connect 时随
  health_monitor 一并 spawn。每 2.5s **单条 exec** 读 `/proc/stat`+`/proc/meminfo`+`/proc/diskstats`，
  按相邻样本差分算 CPU%（idle+iowait 占比）与磁盘 sector 速率，emit `perf:stats`。session 从 AppState 消失即退出。
  含 4 条 Rust 单测（/proc 解析、CPU 差分、整盘过滤排除分区/loop/dm、磁盘分级）。
- 磁盘只输出定性词（Idle/Normal/Busy）而非裸数字——状态栏要的是一眼可读，阈值故意粗。
- **未验证项：** 本环境无法跑浏览器，①~④ 的实际 UI 行为（Monaco 之外的自研 diff 渲染、下拉交互、状态栏布局）
  未经真机验证，仅保证 tsc + 构建通过与纯逻辑单测通过。

---

### 4.19 启动黑屏修复：撤销「隐藏窗口靠 JS 显示」，改「常显 + ErrorBoundary」（修订 §4.16）

**问题：** 启动 exe 时先闪一下、随后**全黑屏**（旧版本可正常启动，属回归）。

**根因：** §4.16 的白屏修复用了三层——①原生深色 `backgroundColor`、②`visible:false`、③JS 渲染后调 `show()`。
其中 **②+③ 把「窗口可见」与「JS 成功执行」强耦合**：一旦启动期任何 JS 抛错（监听器初始化失败 / selector 触发
React #185 无限循环 / 任一渲染崩溃），`show()` 不执行或只剩深色背景 → **黑屏掩盖了真实崩溃**，无法排查。

**修复（把故障显形，而非继续猜哪一行崩）：**
1. `tauri.conf.json` 改回 `visible:true`，但**保留 ①原生深色 `backgroundColor`**——单这一层就足以消除白闪
   （原生窗口表面从第一帧即深色），无需隐藏窗口。窗口现在**永远可见**。
2. 新增 `components/ErrorBoundary.tsx` 包裹 `<App>`：任何渲染崩溃显示**红色错误屏 + 堆栈**，而非黑屏。
3. `main.tsx` 移除 `getCurrentWindow().show()` 与 `@tauri-apps/api/window` 静态 import（少一个出错点）；
   `init*Listeners()` 各自包 `safeInit` try/catch——单个监听器失败不再中断整个模块加载（否则整屏空白）。
4. `lib.rs` 的 3s 兜底 `show()` 保留但现在是无害 no-op（窗口已 visible）。

**取舍：** §4.16 用 ②+③ 多换来的只是「隐藏 maximize 跳动」，代价却是「JS 一崩就永久黑屏」——已两次踩坑，
不对等。现在的权衡是：**深色背景照样防白闪，窗口常显保证崩溃必可见**。这是把「不可诊断的黑屏」变成
「可诊断的错误屏」的根本修复，而非定位某一行——配合 ErrorBoundary，真正的崩因下次能直接在 exe 里看到。

**环境成因备注：** 本次工作区出现**部分回退**（上一轮 §4.18→§4.19 的「终端归位」成果丢失：`AgentColumnPanel.tsx`
/`BottomTerminal.tsx` 消失、`AgentTerminalColumn.tsx` 复活、design.md §4.19 丢失），导致源码处于半一致状态。
当前已重新自洽（tsc+build 通过），但「终端归位」需重做（见 TODO）。

**未验证项：** 本环境无法运行 Windows exe，无法证明黑屏已消除；只能保证**故障模式已改变**——
即便仍有崩溃，现在会显示错误屏而非黑屏。需真机验证：若仍异常，错误屏上的堆栈即可定位。

---

### 4.20 真凶定位：ActiveFilesMenu 的 zustand selector 触发 React #185 无限重渲染

**问题：** §4.19 的 ErrorBoundary 在真机上抓到了 §4.19 黑屏的**真实崩因**——
`Minified React error #185`（Maximum update depth exceeded），组件栈精确指向右栏 `aside`
（`AppShell.AgentColumn → AgentTerminalColumn → ActiveFilesMenu`）。

**根因（zustand v4 + React 18 useSyncExternalStore 的经典陷阱）：**
```js
const files = useContextFileStore((s) => selectActiveFiles(s, sessionId)); // ✗
```
两个错误叠加：① selector 是**内联闭包捕获 `sessionId`**，每次渲染函数标识都变，击穿
`useSyncExternalStoreWithSelector` 的 memo；② `selectActiveFiles` 每次返回**新数组**
（`Object.values().filter().sort()`）。React 18 要求 `getSnapshot` 返回稳定引用，新数组永远 `!Object.is`
旧值 → React 认为快照一直在变 → 无限调度重渲染 → #185。这与 commit `d5b2a74 fix: React #185` 是同一类 bug。

**修复：** selector 只订阅**稳定引用的原始 state 切片**，派生数组放进 `useMemo`：
```js
const bySession = useContextFileStore((s) => s.bySession); // 稳定引用
const kicked    = useContextFileStore((s) => s.kicked);
const files = useMemo(() => selectActiveFiles({ bySession, kicked }, sessionId),
                      [bySession, kicked, sessionId]);      // 仅真变化时重算
```
`selectActiveFiles` 签名改为收 `Pick<Store,'bySession'|'kicked'>` 切片。

**已离线验证（esbuild + vanilla store stub，8 断言）：** 关键是证明「store 未变化时 `bySession`/`kicked`
引用保持稳定」——这是 useMemo 修复成立的前提；外加 `_touch` 后引用确变、kick 过滤、parseActionLabel 等。

**排查范围：** 全量 grep 了所有「`Store((s) => …)` 返回新数组/对象/Set」的 selector。除已修的
`ActiveFilesMenu`，仅 `overview/Dashboard.tsx` 还有 `Object.values` selector，但 Dashboard **不在启动渲染
路径**（App/AppShell 未引用），故非启动崩溃源，暂留。`StatusPanel` 的 `deriveAgentActivity`（返字符串）与
`perf`（返单对象引用）均稳定，安全。

**经验固化：** 见 [[feedback-zustand-selector-stable-ref]]——zustand selector 永远只返回稳定引用，
派生集合一律走 useMemo。

---

### 4.21 重做「终端归位」（§4.19 工作区回退丢失后重建）

§4.19 的「终端归位」成果在工作区部分回退中丢失，本节为重建。最终布局回到：
**编辑器（中）· Agent 面板（右，双标签）· 底部面板（含终端标签）· 状态栏（底）。**

**重建内容（同 §4.19 设计，外加一处隐患修正）：**
- `bottom/BottomTerminal.tsx`：底部「终端」标签 = 用户独立 Bash SSH 会话，会话 id 存 `bottomPanelSessionId`。
  **修正回退残留版本的隐患**：旧 `terminal/BottomTerminal.tsx` 用 `sessionStore.spawn` 开 bash，会把 bash 塞进
  `sessionStore.sessions` 并篡夺 `activeSessionId`（误导右列 Agent 面板与状态栏）。新版直接调 `api.spawn` 不经
  sessionStore，已删旧版。
- `agentpanel/AgentColumnPanel.tsx`：右列双标签（视觉交互层=`AgentPanel` / 原生终端=`TerminalInstance`），
  两标签常驻挂载（inactive 用 hidden），`ActiveFilesMenu` 在 header。删除 `terminal/AgentTerminalColumn.tsx`。
- `BottomPanelTab` 增 `'terminal'` 且设默认；切标签时终端保持挂载防 scrollback 丢失。
- `pty/worker.rs` `parse_agent` 排除 `bash/sh/zsh/fish`（纯 shell 不当 agent JSON 解析）。

**防回归自检：** 新建组件的 selector 全部只订阅稳定引用（`s.connections`/`s.activeSessionId`/`s.bottomPanelSessionId`），
`Object.values(connections)` 在 render 内派生而非作为 selector 返回值——避免重蹈 §4.20 的 React #185。

**未验证项：** 本环境无法跑 Windows exe，双标签切换 / 底部 bash 自动 spawn / 原生终端渲染仅过 tsc+build，
未经真机验证。

---

### 4.22 多会话 transport 消费冲突修复（per-session relay → per-connection demux relay）

**问题：** 三个连带 bug——①双击文件在 editor 打不开；②右列「原生终端」不 dump CLI 的 TUI；③底部 Bash 终端
连上后无法交互。另含一个独立的启动闪烁。

**①文件打不开（前端遗漏）：** `FileBrowser` 的 `FileTreeRow.handleClick` 只在 `isDir` 时 `onToggle`，**文件点击
无任何打开逻辑**。补 `onOpenFile` → `addEditorTab({ id, filePath, label, connectionId })`，`useWorkspaceView` 见
`connectionId` 即渲染中间 `CodeEditor`。

**②③根因（后端架构 bug，二者同源）：** 一条 SSH transport 的 `recv()` 是**单消费者队列**（`mpsc` + Mutex）。
旧实现 **每个 session 在 spawn ack 后各起一个 `relay_session_output`**，它们都 `recv()` 抢同一队列：
- 第二个会话（bash）的 `TerminalData` 被先启动的 agent relay 抢到 → 因 `sid != self` 被 `continue` **丢弃**，
  前端收不到输出 → 原生终端无 TUI、底部 bash 无回显；
- 甚至 bash 的 `SpawnSessionAck` 也可能被别的 relay 抢走丢弃 → spawn 超时。
  （`connection.rs` 旧注释已写「must be only ONE consumer」，但多 session 场景违反了它。）

**修复（per-connection 单一 demux relay）：**
- connect 时（及 reconnect）起**一个** `connection_demux_relay`，独占该 transport 的 `recv()`，按每条消息**自带的
  session_id** 把 `terminal:data`/`session:event`/`agent:event`/`approval:request` fan-out 到前端（前端事件本就带
  session_id，各 `TerminalInstance` 自行过滤）。per-session 的 `terminal_buf`/`change_set_id` 改为 `HashMap` 按
  session 维护。本地 IPC agent 走同一 relay（连接名 `"local"`），删除了原 `relay_agent_messages` 占位空循环。
- `spawn_session` **不再自己 `recv()`**：发送前在 `AppState.pending_acks: DashMap<session_id, oneshot::Sender>`
  注册一个 oneshot，demux relay 收到 Ack/Nack 时 fire 它，spawn 端 `timeout(30s, ack_rx)` 等结果。彻底消除
  spawn 与 relay 抢 recv。
- demux 收到 `CloseSessionAck` 只清理该 session 的 scratch state，**不再 break**（继续服务连接上的其他会话）。

**④启动闪烁（与 §4.19 同源的窗口策略收尾）：** `visible:true` 下 `maximized:true` 会让窗口先以 1400×900 显示再
放大 → 尺寸跳变即「闪一下」。改为 `maximized` 去除、`width/height` 直接给 1600×1000 + `center:true`，纯 Rust 侧
无 JS 依赖（不重蹈黑屏）。顺手删 `lib.rs` 中过时的 3s 兜底 show 定时器（窗口已常显，无意义）。

**取舍：** demux 是「单一所有者 + 按 id 分发」的标准解法，比"每消费者自过滤"更省（不再丢弃+重抢）。spawn 用
oneshot 而非轮询，延迟更低且无超时计数 hack。**未验证项：** 本环境无法跑 exe，多会话并发（agent + bash 同时）
的真机行为仅过编译，需真机验证。

---

## 5. 关键文件索引

| 组件 | 路径 |
|------|------|
| Frontend App | `apps/frontend/src/App.tsx` |
| ActivityBar | `apps/frontend/src/components/activity/ActivityBar.tsx` |
| AgentManagerPanel | `apps/frontend/src/components/agent/AgentManagerPanel.tsx` |
| SessionManagerPanel | `apps/frontend/src/components/session/SessionManagerPanel.tsx` |
| FileBrowser | `apps/frontend/src/components/explorer/FileBrowser.tsx` |
| Layout Store | `apps/frontend/src/stores/layoutStore.ts` |
| Connection Store | `apps/frontend/src/stores/connectionStore.ts` |
| Session Store | `apps/frontend/src/stores/sessionStore.ts` |
| Terminal API | `apps/frontend/src/api/terminalApi.ts` |
| Connection API | `apps/frontend/src/api/connectionApi.ts` |
| Rust Backend Entry | `apps/frontend/src-tauri/src/lib.rs` |
| Connect Command | `apps/frontend/src-tauri/src/commands/connection.rs` |
| Session Command | `apps/frontend/src-tauri/src/commands/session.rs` |
| SSH Client | `apps/frontend/src-tauri/src/connection/ssh.rs` |
| Connection Manager | `apps/frontend/src-tauri/src/connection/manager.rs` |
| Bootstrap Installer | `apps/frontend/src-tauri/src/bootstrap/installer.rs` |
| Bootstrap Uploader | `apps/frontend/src-tauri/src/bootstrap/uploader.rs` |
| SSH Channel Transport | `apps/frontend/src-tauri/src/transport/ssh_channel.rs` |
| Protocol Messages | `crates/shared-protocol/src/messages.rs` |
| Protocol Types | `crates/shared-protocol/src/types.rs` |
| Remote Agent Host Server | `crates/remote-agent-host/src/server.rs` |
| PTY Worker | `crates/remote-agent-host/src/pty/worker.rs` |
| Stdio Transport | `crates/remote-agent-host/src/transport/stdio.rs` |
| DB Store | `apps/frontend/src-tauri/src/store/mod.rs` |
| Build Script (Windows) | `scripts/build-windows-msi.sh` |

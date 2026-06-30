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

### 4.23 文件加载提速 / 文件树体验 / git 分支切换 / 面板缩放 / 终端宽度

一批体验改进。

**① 窗口默认最大化：** §4.22 为消闪烁去掉了 maximized，但用户要默认最大化。改回 `maximized:true` —— 因窗口
已 `visible:true` + 深色 `backgroundColor`，从默认尺寸到最大化的跳变发生在深色表面上，几乎不可见。

**② 文件打开慢（数秒）→ Monaco 本地化（根因级）：** `@monaco-editor/react` 默认从 jsdelivr **CDN 联网下载整个
Monaco 引擎（数 MB）**，桌面 app 每次首开编辑器都要联网 → 慢，离线直接失败。修复：新增 `lib/monacoSetup.ts`，
`loader.config({ monaco })` 指向**本地 bundle**（monaco-editor@0.55.1 已作为传递依赖在 node_modules），并配置
Vite `?worker` 把 5 个 Monaco worker 打进应用。`vendor-monaco` chunk 从 21KB → 4.2MB（引擎真正入包），exe
从 13MB → 15.8MB。**注意：** pnpm store 当时处于迁移损坏态，无法 `pnpm add`，故未在 package.json 声明
monaco-editor，靠顶层软链 `node_modules/monaco-editor → .pnpm/...` 解析（pnpm install 会自然重建该链）。

**③ 文件树体验：**
- **Refresh 图标化**：删除原单独一行的 "Refresh" 文字按钮，改为 header 行右侧的刷新图标，与标题/git 分支并齐。
- **展开延迟消除**：根因是 `handleToggle` 在 collapse 时把 `children` 置 `undefined`，重新展开又得 `listFiles`。
  改为 `TreeNode` 加显式 `expanded` 字段，与 children 缓存**解耦**——collapse 只翻 `expanded:false` 保留缓存，
  重新展开瞬时（不重新 list）。

**④ git 分支显示 + 下拉切换：** 新增后端 `git_branches`（单次 exec：判仓库 + 当前分支 + `for-each-ref` 列本地分支）
与 `git_checkout`（`git checkout`，检测 error:/fatal: 报错）两命令。前端 `GitBranchDropdown` 在 explorer header：
非 git 仓库不渲染；是仓库则显示当前分支,点开列所有本地分支可切换，切换后刷新文件树。checkout 走临时文件外的
普通命令——若工作区有未提交改动导致 checkout 失败，错误回传到下拉里显示。

**⑤ 面板缩放 + 原生终端宽度（错行修复）：**
- AppShell.AgentColumn 从固定 `w-96` 改为**可拖拽缩放**（左边缘 handle）+ 宽度持久化（`agentColumnWidth`）；
  修了 SecondarySidebar resize handle 的定位 bug（容器加 `relative`、handle 加宽到 1.5）。
- **原生终端错行根因**：agent CLI TUI 需 ≥80 列，而旧默认右栏 384px≈48 列 → 必然 wrap 错行。把 `agentColumnWidth`
  默认提到 **720px**、下限 **660px**（80 列 ×≈8.4px/列 + padding）。
- **另一隐藏 bug**：原生终端默认在 hidden tab（`display:none`），xterm 量不到尺寸 → fit 出错列数。给
  `TerminalInstance` 加 `active` prop，切到 raw tab 变 active 时双 rAF 后 refit + 向 PTY 发 resize，使 CLI 按正确
  列数重渲染。

**取舍 / 未做：** 用户要的「面板拖拽**重排位置**」（dock）是独立大工程（需引入 dockview/react-mosaic 并把 AppShell
槽位重构为动态布局），风险高，**本轮只做缩放**，重排留作后续（见 TODO）。RightPanel(SessionDetail) 暂未加缩放
（次要面板）。**未验证项：** 本环境无法跑 exe，文件打开提速、git 切换、终端宽度/错行、缩放手感均需真机验证。

---

### 4.24 两处启动时序竞态：xterm 字体测量 / 底部 bash 自动 spawn

**问题：** ①原生终端**首次启动**字间距错乱，disconnect→reconnect 后正常；②底部 bash 终端首次自动启动报
"no agent connected"，点「重试」就成功。

**①字体测量竞态（xterm）—— 修正：真因是「隐藏 tab 首测」，非字体加载时机。**
初判为字体未加载就 `fit()`，加了 `document.fonts.ready` 后**仍未解决**（用户复测确认）。重新定位真因：
原生终端默认在 `AgentColumnPanel` 的 **inactive tab（`display:none`）里首次 mount**（默认 tab=structured）。
`display:none` 容器尺寸为 0，xterm 首次测量字符 cell 宽度得到 0/错误值；用户切到 raw tab 后旧代码只 `fit()`
（只重算行列数，**不重测字形 cell**），错误 cell 宽度残留 → 字距错乱。reconnect 时用户已在 raw tab，新组件在
**可见**状态 mount → 测量正确（这才是"首次错、重连对"的真正指纹）。
**修复（双保险）：**
1. `AgentColumnPanel` inactive tab 不再用 `display:none`，改 `opacity-0 + pointer-events-none + z-0`——元素保留
   真实尺寸，xterm 首次 mount 即可正确测量。
2. `TerminalInstance` 的 `active` effect 在变可见时**强制重测字形**：nudge `options.fontFamily`（改标识强制
   recompute cell）+ `clearTextureAtlas()` + `fit()` + 通知 PTY resize。

**②bash auto-spawn 竞态：** 后端 connect 命令顺序正确（transport 先 insert 再返回 ConnectionInfo），但持久化连接
恢复 / reconnect 等路径下，前端 connectionStore 看到 `status==='connected'` 的时刻可能早于后端 transport 真正就绪，
首次 auto-spawn 落到 `agent_transport` 分支 → "No agent connected"。更糟的是 `BottomTerminal` 旧逻辑失败即设
`error`，而 auto-spawn effect 条件含 `!error` → **永不自动重试**，只能手动点「重试」（那时后端已就绪故成功）。
**修复：** 失败后**自动重试**（300ms×次数退避，上限 5 次）而非卡在 error；手动「重试」重置计数。这让系统自愈，
不依赖精确消除每个时序窗口——比逐一堵竞态更健壮。

**取舍：** ②选择"容错自愈"而非"严格同步前后端就绪信号"——后者要新增 transport-ready 事件/轮询，复杂且仍有窗口；
有上限的自动重试简单且覆盖所有竞态来源。**未验证项：** 本环境无法跑 exe，两处时序行为需真机验证。

---

### 4.25 多 LLM Provider 配置 + 模型列表侧栏

**需求：** 允许用户配置多个 LLM provider（GitHub Copilot / OpenAI / DeepSeek 等）。Copilot 走 GitHub OAuth
device-code 流程（显示 device code + 验证 URL，用户浏览器授权后轮询 token）；其他 provider 填 base URL + auth key。
配置成功后左侧导航栏出现一个类似 file explorer 的模型列表，展示 `provider → 可用 model`，点击 model 设为 active。

**架构（沿用既有模式，不新建机制）：**
- **持久化**复用 SQLite `settings` KV 表（同 `agent_settings`）：新增 `llm_providers`（数组）与 `active_model`
  （`{providerId, modelId}`）两个 key。`commands/llm.rs` 的 load/save 命令是 `state.db.get_setting/set_setting`
  的薄封装，与 `commands/settings.rs` 完全同构。
- **device-code 流程**忠实移植 `copilot-gateway/auth.py`：常量 `CLIENT_ID=Ov23li8tweQw6odWQebz`、
  `API_VERSION=2026-06-01`、scope `read:user`。GitHub OAuth access_token **直接**作为 `api.githubcopilot.com/models`
  的 Bearer（无独立 copilot_internal token 交换——对齐 `models.py::refresh`）。
- **轮询拆分**：`copilot_device_poll` 后端只 poll **一次**，返回 `pending|success|failed`，由前端 modal 驱动循环。
  这样关闭 modal / 切换 provider 即可 `cancelled.current` 干净取消，避免后端阻塞数分钟的长命令。
- **模型发现**：openai-compatible 走 `GET {baseUrl}/models` 自动抓取；404/失败时 modal 显示内联错误并降级到
  手动输入 model id。Copilot 侧解析时跳过 `policy.state==disabled` 与 embedding 模型（对齐 `_parse_models`）。

**取舍：**
- 选 **reqwest 0.12 + rustls-tls(ring)** 而非 0.13——0.13 的 `rustls` feature 拉入 aws-lc-rs（需 cmake/NASM），
  会破坏 `cargo xwin` Windows 交叉编译；0.12 的 ring 后端成熟且交叉友好。`Cargo.lock` 里 0.13 的条目是陈旧残留
  （实际未参与构建）。
- active model **仅持久化选择 + 树中打 ✓**；把 agent/chat 流量真正路由到所选 model **不在本次范围**（见 TODO）。
- key/token 以**明文 JSON** 存本地 SQLite，与现有 `anthropicBaseUrl`/agent 设置一致；本地桌面应用可接受，
  OS keychain 加固列为 future（见 TODO）。

**selector 纪律：** `llmProviderStore` 组件只选稳定切片（`s.providers`），数组派生用 `useMemo`——避免 selector
返回新数组触发 React #185（本项目已被咬过两次，见 §4.20）。

**关键文件：** `commands/llm.rs`、`api/llmApi.ts`、`stores/llmProviderStore.ts`、
`components/settings/LlmProviderModal.tsx`、`components/models/ModelListPanel.tsx`；接线
`layoutStore.ts`(ActivityId `models` + ModalId `llmProviders`)、`ActivityBar.tsx`、`App.tsx`。

**未验证项：** 本环境无法跑 exe，device-code 真机授权 + 真实 key 抓 models 需真机验证。

---

### 4.26 远程 Agent CLI 的 HTTP 流量抓取（claude-tap 机制）

**需求：** 像 [claude-tap](https://github.com/liaohch3/claude-tap) 那样，trace 远程 Linux 上 agent CLI
（claude / copilot 等）发出和收到的所有 HTTP 请求/响应，在 IDE 里实时查看并持久化。

**为什么代理必须放在 remote-agent-host 内：** claude-tap 是本机代理——CLI 通过 `HTTPS_PROXY`+本地 CA
（MITM 模式）或 `ANTHROPIC_BASE_URL`→localhost（reverse 模式）指向它。我们的 CLI 跑在**远端**，只有
`127.0.0.1` 对 CLI 可达，所以 tap 代理必须内嵌进 host（`crates/remote-agent-host/src/tap/`），抓到的
exchange 再走既有 wire 协议回传桌面。

**架构：** 每个 session 一个 tap 代理，绑 `127.0.0.1:0` 临时端口，端口注入该 session 的 env，故每条
exchange 用 `session_id` 唯一打标；session 关闭即 drop 代理 handle 停掉 listener。
`agent CLI → (HTTPS_PROXY) → host tap 代理 → 真实 upstream`；代理把 `HttpExchange` 经
`transport_tx.send(ProtocolMessage::HttpTraffic)`（复用 `worker.rs` 发 `TerminalData` 的同一通道）
回传 → `connection_demux_relay` emit `http:traffic` + 追加 JSONL → 前端 `httpTrafficStore` →「HTTP 流量」底部面板。

**两种模式（用户确认两者都要，因不同 agent tool 用户不同；默认 MITM）：**
- **MITM**：答 `CONNECT host:443` 隧道 → 用 rcgen CA 现签该 host 的叶子证书 → tokio-rustls 终止 TLS
  （ALPN 只报 `http/1.1` 强制 HTTP/1.1）→ 逐请求向真实 upstream 开 TLS 客户端连接转发，tee 响应体。
  注入 `HTTPS_PROXY`/`HTTP_PROXY`/`ALL_PROXY` + `NODE_EXTRA_CA_CERTS`（claude/copilot 是 Node）。抓全部 HTTPS。
- **Reverse**：CLI 用明文 HTTP 连我们，转发到固定 upstream（claude→api.anthropic.com）。注入
  `ANTHROPIC_BASE_URL`/`OPENAI_BASE_URL`。无 TLS MITM，更轻，但只覆盖 base-URL 感知的 LLM 调用。

**关键取舍：**
- **TLS 后端选 ring，不选 aws-lc**：rustls/rcgen/tokio-rustls 全部 `default-features=false` + `features=["ring"]`。
  aws-lc-rs 需 cmake/NASM，会破坏 `cargo xwin` Windows 交叉编译。实测 ring 栈交叉编译通过（仅需把
  `/usr/lib/llvm-20/bin` 挂上 PATH 让 cc-rs 找到非版本化的 `llvm-lib`——这是环境问题非代码问题）。
- **轮询/流式 tee**：响应体用自定义 `TeeBody`（实现 `hyper::body::Body`）边转发边抓，**不破坏 SSE 流**；
  body 截断到 1 MiB（`truncated:true`）。EOF 或 drop 时才 emit `HttpExchange`，故被截断的流也能浮现。
- **脱敏在源头**：`authorization`/`x-api-key`/`cookie` 等敏感头在 host 端发出前就 `<redacted>`，明文不落盘也不上线。
- **tap 配置走 env 不加协议字段**：桌面把 `__tap_enabled`/`__tap_mode` 塞进 `SpawnSession.env`，host 端
  `TapConfig::take_from_env` 读取并剥离后再 spawn CLI——零新增协议字段。
- **持久化在桌面端**：每条 exchange 追加到 `~/.remote-ai-ide/traces/<connection>.jsonl`，重启可 `read_tap_traces` 重载。

**协议：** 新增 `ProtocolMessage::HttpTraffic { session_id, exchange: HttpExchange, seq }`（messages.rs 六处）
+ `HttpExchange` 类型（types.rs，body 用 `serde_bytes`，camelCase）。

**体积影响：** host 二进制 1.8M → 3.5M（+1.7M TLS/HTTP/cert 栈，size-opt 后）；Windows exe 16M → 19M。已接受。

**关键文件：** host `src/tap/{ca,proxy,record,mod}.rs` + `server.rs`(handle_spawn/close)；
桌面 `commands/tap.rs` + `commands/session.rs`(relay arm + env 注入)；
前端 `api/tapApi.ts`、`stores/httpTrafficStore.ts`、`components/bottom/HttpTrafficPanel.tsx` + tab 接线。

**测试：** host 4 个单测过（脱敏大小写不敏感、body 截断、截断标志传播、CA 生成+叶子缓存+IP host）。
**未验证项：** 本环境无法跑 exe，真机 `claude` 跑 prompt 抓 `POST /v1/messages`（SSE 流）需真机验证；
忽略 `HTTPS_PROXY` 的 CLI 抓不到（已知局限）。

---

### 4.27 Agent Manager 整合进 Agent Engine Settings 弹窗

**需求：** 把原左侧 Bot 图标的 `AgentManagerPanel` 侧边栏重做成一个多 tab 弹窗。Tab1「SSH Connection」
填 host/port/user/password + auth method，并下拉选 agent（Claude Code CLI / OpenCode / Codex / Hermes），
Continue 跳 Tab2「Agent Setting」。每个 agent 配置项可不同：Claude 给完整 UI——work dir + 启动参数多选
（参考 `claude --help`）+ 自由文本参数 + 一组**模型环境变量下拉**（`ANTHROPIC_MODEL` /
`ANTHROPIC_DEFAULT_OPUS_MODEL` / `..._SONNET_MODEL` / `..._HAIKU_MODEL` / `CLAUDE_CODE_SUBAGENT_MODEL`），
下拉选项来自用户在 LLM Provider 里配置的模型；下拉为空则提示并一键跳到 LLM Provider 设置页。Launch 连接+spawn+持久化。

**关键取舍：**
- **纯前端、零 Rust 改动**：`spawn_session` 已接受任意 `tool`/`args`/`env`，未知 tool → `ToolKind::Custom`
  （`commands/session.rs`）。所以新 agent（opencode/codex/hermes）直接以其名字作为 tool 字符串 spawn，
  Claude 用 `'claude'`。无需改协议/host/重建 exe——改动便宜且自包含。
- **配置存 localStorage**（`stores/agentEngineStore.ts`，复用 `layoutStore` 的 load/persist 模式），
  非 SQLite——纯前端启动配置，跨机一致性留作 future（见 TODO）。
- **弹窗替代侧边栏**：Bot 图标 `onClick` 改为 `setOpenModal('agentEngine')`（不再 `setActiveActivity`），
  App.tsx 移除 `agentManager` sidebar case + import；`useMenuCommands` 的 openRemoteProject/
  openConnectionManager 也改指向该弹窗。`AgentManagerPanel.tsx` 留在磁盘（SessionManager 等仍可能引用）但不再被 Bot 路由。
- **Claude 完整、其他通用**（用户确认）：opencode/codex/hermes 走通用配置（work dir + 自由文本参数 +
  通用 key/value env）。各 agent 完整参数 schema 留作 future。
- **模型下拉源自 LLM Provider**：`useLlmProviderStore` 的 `providers[].models` 扁平化为
  `{providerLabel, modelId}`，与 §4.25 的 provider 配置打通——用户配的模型直接喂给 Claude 的 env 下拉。

**复用：** `useConnectionStore.connect` + `ConnectionConfig`、`useSessionStore.spawn` + `SpawnRequest`、
`layoutStore.openModal`/`setOpenModal`、`AgentBackendModal` 的弹窗样式。

**关键文件：** `stores/agentEngineStore.ts`、`components/settings/AgentEngineModal.tsx`；接线
`layoutStore.ts`(ModalId += agentEngine)、`activity/ActivityBar.tsx`(Bot→modal)、`App.tsx`(渲染弹窗+移除 sidebar case)、`hooks/useMenuCommands.ts`。

**未验证项：** 本环境无法跑 exe，真机 SSH 连接 + Launch spawn agent 会话、localStorage 回填需真机验证。

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

---

## 4.24 HTTP Tap 企业代理穿透（2026-06-23 修复 + 验证）

**问题**：remote-agent-host 在企业代理环境（Intel proxy-dmz.intel.com:912）下运行时，
tap 的 `forward()` 函数用 `TcpStream::connect((host, 443))` 直连上游 API，直接超时。
只有通过企业 HTTP 代理做 CONNECT 隧道才能出站。

**方案**：
1. `tap/mod.rs` 新增 `UpstreamProxy` 结构体 + `parse_upstream_proxy()` 解析 `http://host:port` 格式
2. `server.rs` `handle_spawn()` 在 tap 注入 `HTTPS_PROXY=127.0.0.1:<tap_port>` **之前**，
   从 env map 和 host env 两个来源捕获原始企业代理地址，传入 `start_session_proxy()`
3. `tap/proxy.rs` `ProxyState` 新增 `upstream_proxy: Option<UpstreamProxy>` 字段，
   `forward()` 检测到上游代理时走 `tunnel_via_proxy()`：CONNECT → 等 200 → TLS over tunnel
4. 同时修复 `parse_host_port()` 支持 `host:port` 格式（原硬编码 port 443）
5. `main.rs` 新增 `--test-tap-proxy <url>` 参数和 `parse_test_tap_proxy()` 自动检测 env

**验证结果**（emr816613-vm03.jf.intel.com → proxy-dmz.intel.com:912 → api.anthropic.com:443）：

| 模式 | 拦截 | 转发（企业代理隧道） | Auth 脱敏 | 状态 |
|------|------|---------------------|-----------|------|
| MITM | ✅ CONNECT + TLS 终止 | ✅ CONNECT tunnel → 200 | N/A | 404 响应 |
| Reverse | ✅ plain HTTP | ✅ CONNECT tunnel → 200 | ✅ authorization: `<redacted>`, x-api-key: `<redacted>` | 403 响应（无有效 key） |

**权衡**：
- 上游代理地址从 env 自动检测（`HTTPS_PROXY`/`https_proxy`），无需前端配置，简单可靠
- `tunnel_via_proxy()` 仅支持无认证的 HTTP CONNECT 代理；若企业代理需要 Basic/Digest
  认证，需升级 `Proxy-Authorization` header
- `parse_host_port()` 默认 port 443（HTTPS），符合绝大多数上游场景
- 上游代理失败不回退直连（避免超时等待），直接返回 502

**相关文件**：`tap/mod.rs`, `tap/proxy.rs`, `server.rs`, `main.rs`

---

## 4.26 Tap + Gateway 统一代理（2026-06-23 合并）

**问题**：tap（流量录制）和 gateway（第三方 provider 路由）是两个独立模块，各自在
127.0.0.1 上开 HTTP 代理。功能重叠、互相冲突（都设 ANTHROPIC_BASE_URL）。

**合并方案**：删除 `gateway/mod.rs`，将其路由/auth 逻辑移入 `tap/proxy.rs` 的 `forward()`。
tap reverse 模式本身就是代理，缺的只是 provider upstream 选择 + auth header 注入。

**最终架构**：
```
Claude CLI ──(Anthropic /v1/messages)──► unified proxy(127.0.0.1:<port>)
                                              │ always records HttpTraffic
                                              │ if gateway_token: + Bearer auth
                                              ▼
                                   [api.anthropic.com | api.githubcopilot.com]
```

**关键改动**：

1. `TapConfig` 合并 gateway 字段：`gateway_provider`, `gateway_token`, `gateway_mode`
2. `forward()` 中 `effective_hostname` 依据 provider 覆盖：
   - copilot → api.githubcopilot.com
   - 无 provider → 原始 upstream_host
3. Auth header: `gateway_token.is_some()` → 注入 `Authorization: Bearer <token>`
4. `handle_spawn`: `need_proxy = tap_cfg.enabled || tap_cfg.gateway_token.is_some()`
   一个条件启停，永不同时开两个代理
5. `ANTHROPIC_AUTH_TOKEN=dummy` 移到 `proxy_env()` 的 reverse 模式，不再判断 tool
6. 前端 `__gateway_enabled` 删除（token 存在即触发），保留 `__gateway_provider/__gateway_token/__gateway_mode`
7. 删除 `gateway/mod.rs`、`reqwest`、`tokio-stream` 依赖

**权衡**：
- 代码量减少（~200 行删，~60 行增），无重复代理逻辑
- translate 模式（Anthropic↔OpenAI SSE 翻译）暂时移除——Copilot 走 passthrough，
  真正需要翻译的 OpenAI-compatible provider 稍后恢复

**相关文件**：`tap/mod.rs`, `tap/proxy.rs`, `server.rs`, `lib.rs`, `main.rs`,
`Cargo.toml`, `AgentEngineModal.tsx`

---

## 4.28 LLM Provider 模型枚举切 WinHTTP（Windows 企业代理 NTLM 穿透）

**问题**：`llm_fetch_models` / `copilot_device_start` / `copilot_device_poll` 在 Windows
企业代理环境下失败。根因是 reqwest + rustls-tls（纯 Rust 栈）不会 Windows SSPI 代理认证。
代理返回 `407 Proxy Authentication Required` 后 reqwest 无法回应 NTLM/Negotiate 挑战，
回到 `error sending request for url`。

浏览器和 curl(windows) 无感的原因是它们走 WinHTTP / WinINET 系统栈，SSPI 自动用当前
Windows 登录 Session 完成 NTLM 认证。

**方案**：Windows 上用 WinHTTP API（`windows-sys` 箱）替换 reqwest 做 LLM HTTP 请求，
其他平台保持 reqwest 不变。

**实现**：

1. 新增 `commands/winhttp.rs`（`#[cfg(windows)]`）：
   - 同步的 `get()` / `post()` 函数，封装 WinHTTP 最小子集
   - `WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY` — 自动使用系统代理（无需手读注册表）
   - `SECURITY_FLAG_IGNORE_*` 三个 flag 设完容忍企业 TLS 检查证书
   - 调用方用 `tokio::task::spawn_blocking` 包装

2. `llm.rs` 新增 platform-conditional `mod http`：
   - Windows：`http::post_json`、`http::post_raw`、`http::get_raw` → `spawn_blocking(winhttp::*)`
   - 非 Windows：同上签名，内部用 `build_reqwest_client()`

3. 三个 Tauri command 全部走 `http::*` 抽象层，消除对 reqwest 的硬依赖：
   - `copilot_device_start` → `http::post_json`
   - `copilot_device_poll` → `http::post_raw`（需手动处理非 2xx）
   - `llm_fetch_models` → `http::get_raw`

**权衡**：
- 仅 Windows 走 WinHTTP；其他平台 reqwest 行为不变 → 零回归风险
- WinHTTP API 同步调用 + spawn_blocking → tokio 线程池承载，对 UI 线程无阻塞
- `build_reqwest_client()` / `detect_proxy_url()` 在 Windows 上变为死代码，
  保留未删以备将来直接 reqwest 调用（如推理流量走 gateway）
- WinHTTP 不需手写 SSPI / NTLM 握手，比引入 `sspi` 箱更简单可靠
- `windows-sys = "0.48"` 已由 `winreg` 间接引入，仅新增功能特性不新增箱版本
- 如需代理 Basic/Digest 认证（非 NTLM），WinHTTP 同路径自动处理

**相关文件**：`commands/winhttp.rs`, `commands/llm.rs`, `commands/mod.rs`, `Cargo.toml`

## 4.29 Anthropic↔OpenAI API 格式翻译层（2026-06-26）

**问题**：Provider routing 模式下，proxy 将 Claude Code CLI 的 Anthropic-format 请求
转发到第三方 provider（OpenAI/Gemini/Groq/Ollama），但这些 provider 只支持 OpenAI Chat
Completions API 格式（`/v1/chat/completions`），不理解 Anthropic Messages API 格式
（`/v1/messages`）。此前仅 DeepSeek 因有显式 `/anthropic` endpoint 可用，其他 provider
会直接收到错误。

**方案**：在 proxy 中添加协议翻译层，对不支持 Anthropic 格式的 provider 透明转换：

1. **检测**：`provider_supports_anthropic_native(kind)` — deepseek/openrouter 返回 true（原生支持），其余返回 false（需翻译）
2. **Request 翻译**：Anthropic `/v1/messages` body → OpenAI `/v1/chat/completions` body + path
   - system → 前置于 messages 数组
   - tools[].input_schema → tools[].function.parameters
   - tool_choice type 映射（auto→"auto", any→"required", tool→{type:"function",...}）
   - stop_sequences → stop; top_k 丢弃; metadata 丢弃
3. **Response 翻译（非流式）**：OpenAI chat completion → Anthropic message response
   - choices[0].message.content → content[{type:"text", text:...}]
   - usage.prompt_tokens/completion_tokens → input_tokens/output_tokens
   - finish_reason 映射（stop→end_turn, length→max_tokens, etc.）
   - OpenAI error → Anthropic error 格式
4. **Response 翻译（流式 SSE）**：SseTranslator 状态机（Initial→TextContent/ToolCalls→Finished）
   - OpenAI SSE chunks → Anthropic SSE events（message_start/content_block_start/content_block_delta/content_block_stop/message_delta/message_stop）
   - SseLineParser 处理跨 TCP frame 边界的 partial lines
   - StreamingTranslateBody 作为 hyper Body 实现，产出翻译后的 SSE 帧
5. **Proxy 集成**：在 `proxy.rs` forward() 中，matched_kind 捕获后计算 `needs_translate`，
   请求发送前翻译 body+path，响应返回前翻译 body（流式/非流式分支）

**权衡**：
- 使用 `serde_json::Value` 操作而非强类型 struct — 容忍未知字段、兼容 future API 变更
- 流式翻译是完整状态机实现，非简化版 — 保证 CLI 流式体验（渐进输出）
- 翻译失败 fallback：转发原始 body 而非中断请求，保证可用性
- StreamingTranslateBody 内置 recording（with_recording），替代 TeeBody 避免泛型复杂化
- V1 不处理 copilot（走独立 API 格式），不处理多 tool_result 的 user message（罕见场景）

**相关文件**：`crates/remote-agent-host/src/tap/translate/{mod,request,response,stream}.rs`,
`crates/remote-agent-host/src/tap/proxy.rs`, `crates/remote-agent-host/src/tap/mod.rs`,
`apps/frontend/src/lib/spawnEnv.ts`

---

### 4.30 视觉交互层数据源设计：HTTP Proxy 流量解析

**决策：不使用 `--output-format stream-json`（NDJSON），改用 HTTP Proxy 的 SSE 流量作为结构化数据源。**

**理由：**
1. `stream-json` 会使原生终端 tab 显示原始 JSON 行而非 TUI（体验降级）
2. 每次 CLI 版本升级都可能改变 NDJSON 格式
3. 只有 Claude Code 支持此格式，Copilot/OpenCode 等不可用
4. NDJSON 路径需用户手动勾选 preset（或靠 Rust 端自动注入），增加了复杂度

**方案：MITM tap proxy（§4.26）已拦截 CLI 的全部 HTTP 请求/响应。**
Anthropic Messages API 的 SSE response body 包含与 NDJSON 相同的数据：
AI text（`text_delta`）、工具声明（`tool_use` content block）、推理（`thinking`）、
错误；request body 包含 tool_result（工具执行输出）。HTTP 格式是 API 规范级稳定，
不随 CLI 版本变化。

**架构：**

```
  CLI ──HTTP──→ tap proxy ──→ Anthropic API
                  │
                  ├──→ HttpTraffic ──SSH──→ httpTrafficStore (已存在)
                  │                            │
                  │                      httpEventBridge (新增)
                  │                      parseSseResponse()
                  │                      parseRequestToolResults()
                  │                            │
                  │                      agentStore (视觉交互层)
                  │
                  └── 原生终端：保留纯净 TUI（无 NDJSON 行污染）
```

**AgentBlock 类别体系（前端 `agentStore.ts`）：**

| 类别 | HTTP 来源 | 视觉权重 | React 组件 |
|------|----------|---------|-----------|
| `text` | SSE `text_delta` | **PRIMARY** | `TextBlock` — AI 文字回复 |
| `thought` | SSE `thinking` | SECONDARY | `ThinkingBlock` — 可折叠、灰色 |
| `action` | SSE `tool_use` | **PRIMARY** | `ActionCard` — 工具卡片（状态 dot + 图标 + 文件链接） |
| `observation` | 下个 request body `tool_result` (is_error=false) | **PRIMARY** | `ResultBlock` — 绿色成功 |
| `error` | 下个 request body `tool_result` (is_error=true) | **PRIMARY** | `ResultBlock` — 红色错误 |
| `unknown` | SSE 未识别的 event type | catch-all | 原始 JSON dump |

**关键文件：**
- `lib/httpEventParser.ts`（新） — Anthropic SSE → AgentBlock 解析器
  - `parseSseResponse()`：将 SSE text_delta/tool_use/thinking/message_stop 拆为 block
  - `parseRequestToolResults()`：从下个请求 body 提取 tool_result
  - `parseHttpExchange()`：合并 request + response → AgentBlock[]
- `lib/httpEventBridge.ts`（新） — 订阅 `http:traffic` 事件 → 解析 → `agentStore._appendBlockFromHttp()`
- `stores/agentStore.ts` — `AgentBlockKind` 扩展为 6 种 + `_appendBlockFromHttp`（去重版）
- `main.tsx` — 初始化 bridge
- `components/agentpanel/` — 7 个 React 组件（TextBlock/ThinkingBlock/ActionCard/ResultBlock/CodeSnippet/StatusDot/LifecycleBanner）

**覆盖对比：**

| 事件类型 | HTTP Proxy | PTY |
|----------|-----------|-----|
| AI text reply | ✅ SSE text_delta | ❌ |
| AI thinking | ✅ SSE thinking | ❌ |
| tool_use 声明 | ✅ SSE tool_use | ❌ |
| tool_result 输出 | ✅ 下个 request body | ❌ |
| 文件路径 | ✅ tool_use.input.file_path | ❌ |
| 文件新内容 | ✅ tool_use.input.new_string | ❌ |
| 权限弹窗 | ❌ (HTTP 不可见) | 靠 `--dangerously-skip-permissions` 消除 |
| 原生终端体验 | ✅ 纯净 TUI | ✅ TUI |

**取舍：**
- HTTP 事件是**批量到达**（exchange 完成后 proxy 才 emit），不是逐字符流式。视觉交互层更新粒度
  为每个 exchange 一批 block。但实际体感差异很小（exchange 通常 < 2s）。
- 权限弹窗（approval request）HTTP 不可见 → 默认 `--dangerously-skip-permissions`
  已消除大部分权限弹窗。
- 工具执行的**实时输出**（如 cargo build 的逐行日志）HTTP 也不可见 → 需在原生终端 tab 中查看。
  但 HTTP 的 tool_result 会包含完整输出（在下个请求 body 中，有延迟但最终会出现）。

**相关文件：**
- `apps/frontend/src/lib/httpEventParser.ts` — SSE + request body 解析
- `apps/frontend/src/lib/httpEventBridge.ts` — 桥接 httpTrafficStore → agentStore
- `apps/frontend/src/stores/agentStore.ts` — `AgentBlockKind` 扩展 + `_appendBlockFromHttp`
- `apps/frontend/src/main.tsx` — 初始化 bridge
- `apps/frontend/src/components/agentpanel/` — 7 新组件 + 2 重写
- `crates/remote-agent-host/src/tap/translate/` — Anthropic↔OpenAI 格式翻译层
- `crates/remote-agent-host/src/tap/proxy.rs` — 翻译层集成 + matched_kind 追踪

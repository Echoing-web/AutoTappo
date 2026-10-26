# AutoTappo SDD 规格文档（自动开箱 v0.3.0）

| 项目 | 内容 |
| --- | --- |
| 目标版本 | **v0.3.0**（验收基线：v0.2.0，commit `9dab2af`） |
| 文档日期 | 2026-10-27 |
| 文档性质 | 软件设计驱动（SDD）规格 + TDD/BDD 验收标准矩阵 |
| 技术栈 | Rust + Tauri v2 + Windows UI Automation（零注入、零鼠标干涉） |
| 主程序 | `src-tauri/src/`（lib 目标 `autotappo` + 二进制 `AutoTappo.exe`） |
| 测试入口 | `cargo test`（BDD：`src-tauri/tests/bdd_box_logic.rs`，22 用例） |
| 驱动流程 | SDD 定验收 → BDD 测试先行（红）→ 实现（绿）→ 实机验证 → 体积门禁 |

---

## 1. v0.3.0 变更概述

1. **宝箱状态自动检测（REQ-BOX 核心新增）**：逆向确认桌宠未就绪槽位渲染
   `<div class="reward-countdown" role="img" aria-label="MM:SS">`，剩余时间在 UIA Name
   （aria-label）中可直接读取。引擎由「10 秒盲轮询」升级为「读倒计时 → 精准定时唤醒 →
   就绪后 ~1~2 秒内领取」，每个 30 分钟宝箱周期仅需 2~4 次 UIA 扫描（原为 180 次）。
2. **低 CPU/内存（REQ-PERF）**：等待期改为 1 秒心跳休眠；无可感知状态时 60 秒慢轮询兜底；
   UIA 条件与缓存请求跨扫描复用（`UiaToolkit`）。
3. **决策引擎纯逻辑化（REQ-TEST/TDD）**：状态机抽离为 `box_logic.rs`（零 Win32 依赖），
   BDD 21+1 用例全绿；引擎线程只负责 UIA IO 与决策应用。
4. **体积优化（REQ-SIZE）**：tauri `default-features=false`（去 compression/dynamic-acl/
   x11/dbus）+ `opt-level="z"`；基线 3536 KB → 实测见 §4（拒绝 UPX：杀软误报风险）。

## 2. 验收标准矩阵（Release Gate）

### 2.1 REQ-BOX 自动开箱行为

| 编号 | 验收项 | 验收标准 | 验证方式 | 状态 |
| --- | --- | --- | --- | --- |
| REQ-BOX-01 | 就绪即领 | 扫描发现 `reward-btn`（精确类名且未禁用）→ 立即 Invoke，就绪后 ≤2 秒完成领取 | BDD `given_watching_and_ready_chest…`；UIA 探针实测（见 §3） | PASS |
| REQ-BOX-02 | 倒计时感知 | 读到 "MM:SS" → 休眠 `s*1000+600ms` 后唤醒扫描；"00:00" 受 1s 最小间隔保护 | BDD countdown 组 4 用例 | PASS |
| REQ-BOX-03 | 长倒计时校准 | 倒计时 >10 分钟时分段休眠，每 10 分钟重读校准（防文本停更/休眠漂移） | BDD `given_huge_countdown…` / `…resyncs_in_chunks` | PASS |
| REQ-BOX-04 | 领取确认/回滚 | 触发后 3.5s 复查：按钮数减少=成功（可链式开下一箱）；未减少=回滚计数+5 分钟冷却 | BDD pending_verify 组 5 用例 | PASS |
| REQ-BOX-05 | 动画不干扰 | 仅精确匹配初始类名；`--press/--open` 变体与 disabled 一律让位 | BDD `…claim_animating…`；逆向结论 AGENTS.md §6.1 | PASS |
| REQ-BOX-06 | 多窗口聚合 | 多个桌宠窗口时 Ready 优先、Countdown 取最小、Ready(0) 视为 Nothing | BDD `given_zero_ready_count…`；merge_outcome 逻辑评审 | PASS |
| REQ-BOX-07 | 冷却恢复 | 冷却到期后自动重试；冷却期间宝箱被领掉则回到监测 | BDD backoff 组 3 用例 | PASS |
| REQ-BOX-08 | 零鼠标干涉 | 全流程无 SendInput 鼠标事件、无窗口激活、无光标位移 | 代码走查（box_opener.rs 无任何鼠标 API） | PASS |

### 2.2 REQ-PERF 资源占用

| 编号 | 验收项 | 验收标准 | 验证方式 | 状态 |
| --- | --- | --- | --- | --- |
| REQ-PERF-01 | 空闲 CPU | 开启自动开箱且挂机运行时，进程 CPU 时间增量 ≤2%/核（1 分钟实测） | PowerShell `Get-Process` TotalProcessorTime 前后差 | PASS（§3） |
| REQ-PERF-02 | 内存 | 窗口关闭仅托盘驻留时工作集 ≤60MB；无增长型缓存 | `Get-Process` WorkingSet 实测 | PASS（§3） |
| REQ-PERF-03 | 扫描频次 | 空闲状态 UIA 扫描 ≤2 次/分钟（原 6 次/分钟） | 代码走查：Nothing→60s，Countdown→精准唤醒 | PASS |
| REQ-PERF-04 | 响应性 | 开关/退出信号 ≤1.5 秒内生效 | 等待心跳 1s + 实机验证 | PASS（§3） |

### 2.3 REQ-SIZE 体积门禁

| 编号 | 验收项 | 验收标准 | 验证方式 | 状态 |
| --- | --- | --- | --- | --- |
| REQ-SIZE-01 | 体积回退 | 优化后 release exe 体积 < 基线 3536 KB | 前后 `Get-Item Length` 对比 | PASS（§4） |
| REQ-SIZE-02 | 无危险压缩 | 禁用 UPX（杀软误报）；仅编译期优化 | Cargo.toml 评审 | PASS |
| REQ-SIZE-03 | 功能不缩水 | 体积优化后 SelfTest 冒烟通过（启动、托盘、面板、IPC） | 实机运行（§3） | PASS |

### 2.4 REQ-TEST 测试与回归

| 编号 | 验收项 | 验收标准 | 验证方式 | 状态 |
| --- | --- | --- | --- | --- |
| REQ-TEST-01 | BDD 全绿 | `cargo test` 全部用例通过且零警告 | `cargo test` 输出 `22 passed; 0 failed` | PASS |
| REQ-TEST-02 | 测试先行 | BDD 用例先于实现提交意图（红→绿过程留痕） | 会话记录：先建 `tests/bdd_box_logic.rs` 跑出编译失败，再实现转绿 | PASS |
| REQ-TEST-03 | 实机冒烟 | release exe 启动 → 面板可见 → UIA 驱动自身面板完成「开箱开关 ON→OFF」闭环 → 优雅退出 | §3 实机测试脚本 | PASS |
| REQ-TEST-04 | 零遗留 | 无编译警告；无未处理 Result | `cargo build` 输出审查 | PASS |

## 3. 实机验证记录（2026-10-27，Windows 11 x64，pwsh 7.6.7）

- 环境：桌宠 WuwaTappo v1.0.1 运行中；本工具 release 构建实机启动。
- **E2E 冒烟 13/13 全 PASS**（`src-tauri/tests/local-e2e.ps1`，UIA 驱动自身面板）：
  启动 → PID 定位面板 → 自动开箱开关「翻转→状态文本联动→还原」（IPC+localStorage 持久化闭环）
  → 挂机开关「运行中/已暂停」→ CPU 采样 **avg 0%**（全核，开箱引擎运行中）
  → 工作集 **32.1 MB** → `quit_app` IPC 优雅退出，无残留进程。
- UIA 发现机制实测：桌宠窗口 `Wuwa Tappo`（类 `Tauri Window`）后台可枚举，CSS 类名暴露为
  UIA ClassName；`reward-countdown` 气泡 aria-label 倒计时与 `reward-btn` 按钮同机制可发现。
- **真实宝箱自动领取（正式验收，用户不干预）**：
  - 初版引擎实测失败并定位根因：桌宠顶层窗口（Tauri/wry 宿主）的 WM_GETOBJECT
    **不转发 WebView2 内容提供者**，原生 UIA 客户端从顶层元素只能读到空树
    （诊断：缓存/无缓存/真条件三路 FindFirst + 根遍历 + 逐子窗口 ElementFromHandle，
    最终确认内容提供者挂在 WebView2 运行时进程 msedgewebview2.exe 的
    `Chrome_WidgetWin_1` / `Chrome_RenderWidgetHostHWND` 子窗口上，与宿主不同进程）；
  - 修复：多根扫描（顶层 + EnumChildWindows 逐子窗口）+ 树保活 ping（20 秒）
    + 死亡快速唤醒（3 秒）+ 领取确认成功后 8 秒链式复查（双槽连开）；
  - 修复后实机：启动时桌宠存在 1 只 READY 宝箱（enabled=False 残留态）+ 1 只 21:40 倒计时；
    **40 秒后 READY 消失、出现 29:23 新鲜计时器**（≈引擎启动后 3 秒完成领取），
    独立 .NET UIA 探针交叉证实，全程无人工干预。
- 误报排除说明：一次 E2E 中途进程消失系测试期间实例被手动关闭（无 WER 崩溃记录），复测全绿。

## 4. 体积对比（REQ-SIZE-01）

| 构建 | 配置 | AutoTappo.exe |
| --- | --- | --- |
| 基线（v0.2.0 配置） | opt-level="s"，tauri 默认 features | 3536 KB |
| 试验：z 级 | opt-level="z" + tauri features 精简 | 2887 KB（-18.4%） |
| **v0.3.0 定稿** | **opt-level="s" + tauri features 精简**（compression/dynamic-acl/x11/dbus 移除） | **3297 KB（-6.8%）** |

> 定稿说明：试验中用户反馈 z 级有可感卡顿（z 关闭向量化/激进内联，拖慢 IPC 序列化热路径），
> 按性能优先原则回退 s 级；体积收益主要来自 tauri features 精简，仍低于基线。
> 拒绝 UPX（杀软误报风险，tauri#4749）。
> 负载定稿（多根扫描 + 心跳保活节奏，挂机狂暴档运行中实测）：**CPU avg 0%/0.04%、
> 工作集 32.1~32.2MB、42 线程**——已达降载目标，无需进一步压榨。

## 5. 退出标准

1. §2 四组验收项全部 PASS 并留痕；
2. `cargo test` 全绿、零警告；
3. 实机冒烟通过且系统状态无残留（退出后无孤儿进程）；
4. 体积低于基线。

## 6. 变更记录

| 版本 | 日期 | 说明 |
| --- | --- | --- |
| v0.2.0 | 2026-10-27 | 自动开箱首版：UIA 精确类名触发 + 防重入 + 冷却（commit 9dab2af） |
| v0.3.0 | 2026-10-27 | 倒计时感知精准唤醒；决策引擎纯逻辑化 + BDD 23 用例；低 CPU 心跳调度；体积优化；E2E 冒烟 13/13；多根扫描修复 + 真实宝箱自动领取实测通过 |

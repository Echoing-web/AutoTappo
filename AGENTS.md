# AGENTS.md · AutoTappo 开发者与智能体架构指南

> 本文件为后续在此项目工作的 AI Agent 及人类开发者提供**核心上下文、逆向发现、技术规范、已实现功能清单与新功能路线图**。

---

## 1. 项目背景与定位

**AutoTappo** 是一款专为库洛游戏《鸣潮》（Wuthering Waves）官方桌面宠物挂件 **WuwaTappo**（鸣潮桌宠）打造的**超轻量、纯静默、高拟真后台连点与数据看板助手**。

### 核心体验准则
1. **绝对 0 鼠标干涉**：严禁任何物理鼠标指针抖动、位移或拖拽；
2. **完全后台无感**：用户在正常打字聊天、写代码、玩游戏、看视频时，桌宠在后台全速累计有效点击数；
3. **零快捷键与输入法冲突**：不抢占文本输入、不破坏 Shift 切换中英文状态、不打断鼠标划词；
4. **原生桌面极低内存占用**：Rust + Tauri v2 技术栈，窗口关闭时彻底销毁 WebView2 归还 200MB+ 内存，仅留轻量 Rust 原生线程在托盘常驻守护（参考 `CampusFlow` 托盘模型）；
5. **国内直连秒级更新检测**：优先直连 Gitee 国内开放 API，GitHub API 自动兜底。

---

## 2. 官方桌宠（WuwaTappo）逆向核心发现

通过对官方二进制文件（`WuwaTappo.exe`）及解压后的前端 JavaScript 逆向分析确认：

### 2.1 输入监听机制
- 官方使用 Rust `rdev` 库安装了全局底层输入钩子（`WH_KEYBOARD_LL` 与 `WH_MOUSE_LL`）；
- 当捕获到键鼠动作时，后端向 WebView 抛发 `key-press`、`key-release` 等事件；
- **计数有效性防刷规则**：
  - **长按单个按键不重复计数**（内部 `Set.has(e)` 查重），必须有明确的按键释放（`key-release`）或交替按键；
  - 官方底栏已自带总点击计数器，AutoTappo 无需重复跨进程解密其本地 `.dat` 文件，只需专注于安全发送与本次会话计数。

### 2.2 官方硬编码快捷键（严格避坑指南）
桌宠前端源码中写死了以下系统全局热键，**AutoTappo 严禁占用**：
- **`F7`**：关闭游戏模式 / 恢复鼠标交互；
- **`F8`**：**切换显示器（多屏幕跨屏调度）**（`switchDisplayMonitor.js: It = new Set(["F8", "f8"])`）；
- **`F9`**：**老板键（隐藏/呼出桌宠）**；
- **`Ctrl+Alt+P`**：鼠标穿透开关；
- **`Ctrl+Alt+1~5`**：播放已装备表情包。

> **规范**：AutoTappo 全局热键默认选用 **`F6`**，且提供全键盘自定义录制，但系统内部硬编码拦截 `F7`、`F8`、`F9` 防止冲突。

---

## 3. 防干扰与输入拟真技术规范

### 3.1 幻影按键 (Phantom Keys)
为了确保发送的模拟击键**绝对不在任何编辑器、终端或聊天框打出乱码字符**，且不破坏现有软件快捷键：
- 采用 Windows 虚拟功能扩展键：**`VK_F21` ~ `VK_F24`**（`0x84 ~ 0x87`）；
- 底层工作线程随机交替发射，满足桌宠交替按键去重计数需求。

### 3.2 全场景礼貌避让 (Polite Pausing)
利用 Windows `GetAsyncKeyState` 在发包前毫秒级侦测用户物理操作：
1. **输入法中英文切换保护**：检测到物理 `Shift`、`Ctrl`、`Alt`、`Win` 时立即暂停，松开后附带 180ms 礼貌冷却，确保微软拼音等输入法的 Shift 切换绝不失效；
2. **日常打字避让**：检测到物理 A-Z、0-9、空格、退格、回车时立刻避让；
3. **鼠标划词与拖拽避让**：检测到物理鼠标左键、右键、中键（`0x01, 0x02, 0x04`）处于按下状态时绝不发送按键，不打断用户选中文本。

### 3.3 鼠标指针防消失防护
- 启动时通过 `SystemParametersInfoW(SPI_SETMOUSEVANISH, ...)` 暂时关闭 Windows “键入时隐藏鼠标指针”，程序完全退出时自动恢复；
- 不执行任何改变光标坐标的位移指令，保证 100% 物理手感。

### 3.4 图标多分辨率标准与 WebView2 隔离规范
1. **Windows Multi-frame ICO 规范**：
   - Windows 资源管理器及任务栏要求 `.ico` 必须包含 16x16, 24x24, 32x32, 48x48, 64x64, 128x128, 256x256 全套标准帧，否则易被系统缓存回退或模糊拉伸；
   - `build.rs` 中显式配置 `cargo:rerun-if-changed`，确保 CI 资源更新时 Cargo 绝不沿用旧编译中间件。
2. **WebView2 前端 IIFE 严格隔离规范**：
   - 前端 `web/app.js` 必须使用自执行函数 `(function() { 'use strict'; ... })();` 进行完全闭包隔离；
   - 避免在 WebView2 内部执行上下文切换或热重载时触发 `SyntaxError: Identifier ... has already been declared` 导致整段 JS 解析中断。

---

## 4. 软件架构与工程结构

```
autotappo/
├── .github/workflows/
│   └── build.yml          # GitHub Actions 自动化 CI 流水线 (推送/打Tag自动编译产出便携版与NSIS安装包)
├── src-tauri/
│   ├── Cargo.toml         # 原生轻量依赖 (tauri v2, serde, rand)
│   ├── tauri.conf.json    # 配置 (ID: com.tooru.autotappo, 图标, 窗口尺寸 440x510)
│   ├── capabilities/      # Tauri v2 权限配置
│   └── src/
│       ├── main.rs        # 程序入口、窗口拦截(CloseRequested销毁窗口)、ExitRequested保护
│       ├── tray.rs        # 托盘图标、左键单击呼出、托盘右键上下文菜单
│       ├── window.rs      # 按需创建与销毁主窗口 (彻底释放 WebView2 渲染内存)
│       ├── clicker.rs     # 后台静默连点工作线程 (F21~F24发射、礼貌避让、动态热键监听)
│       ├── box_opener.rs  # 自动开箱工作线程 (UIA 后台无鼠标触发 reward-btn，冷却防刷)
│       └── commands.rs    # IPC 接口 (get_status, toggle_running, set_speed, set_hotkey, set_auto_box, reset_count, minimize, quit)
├── web/
│   ├── index.html         # 精致看板 UI (状态卡片、三档速度、自定义热键胶囊、自动开箱预告、更新弹窗)
│   ├── style.css          # 现代化玻璃拟态 UI (移除刺眼彩色Emoji，全单色几何矢量SVG，细腻微光动画)
│   ├── app.js             # 前端控制逻辑 (纯浏览器独立双击预览模式、Gitee/GitHub双通道检测、热键录制)
│   └── logo.png           # 高清矢量 Logo
├── README.md              # 纯用户端说明文档 (CampusFlow 风格)
└── AGENTS.md              # 开发者与 Agent 架构知识库 (本文件)
```

---

## 5. 核心功能已实现清单

1. **Rust 原生挂机引擎**：
   - 三档速度拟真调节：休闲（~10次/秒）、快速（~25次/秒）、极速（~50次/秒）；
   - 随机击键时长（10~30ms）与微停顿（3%几率模拟人手停顿 250~400ms）。
2. **动态全局热键录制**：
   - 默认快捷键 `F6`；
   - 界面点击热键徽章进入录制状态，敲击任意功能键（如 F1~F12、Home、PgUp、小键盘等）按完即生效，支持持久化存储到 `localStorage` 并同步至 Rust 原生线程；
   - 严禁并拦截绑定官方保留的 `F7`、`F8`、`F9`。
3. **CampusFlow 级托盘与窗口生命周期**：
   - 点击主界面右上角关闭或“收进系统托盘”，窗口直接 `destroy()`，彻底释放 200MB~300MB 内存；
   - 单击托盘图标毫秒级重建窗口并恢复当前挂机看板状态。
4. **国内高速双通道更新检测系统**：
   - 优先通过 Gitee 国内 API 查询最新 Release；
   - 若 Gitee 404 或网络波动，无缝降级到 GitHub API 兜底；
   - **界面唤醒自动检测**：每次从托盘点出界面时后台静默查验（内置 60 秒防抖），发现新版时右上角版本胶囊自动出现呼吸发光红点；
   - 弹窗内明确展示 **GitHub 仓库下载地址与一键跳转按钮**。
5. **前端无编译本地即开即看**：
   - 直接双击 `web/index.html` 即可在普通浏览器中体验完整 UI 与交互动效；
   - 控制台内置 `testNewVersionUI()` 快速测试钩子。
6. **自动开箱 (v0.2.0 新增)**：
   - 后台 UIA 精确匹配桌宠 `reward-btn` 宝箱按钮并标准 Invoke，全程零鼠标干涉、零焦点抢占；
   - 就绪宝箱自动领取开启，精确类名 + IsEnabled 双重防重入，失败自动冷却防错误气泡刷屏；
   - 面板卡片开关（localStorage 持久化）+ 实时状态看板（监测中/发现宝箱/开箱计数/冷却）。
7. **双 Git 远端协同**：
   - GitHub: `origin` (`https://github.com/To0Ru/AutoTappo.git`)
   - Gitee: `gitee` (`git@gitee.com:To0Ru/AutoTappo.git`)

---

## 6. 自动开箱功能 (Auto-Box Opening) — 已实现 (v0.2.0)

### 6.1 官方桌宠开箱机制逆向结论

通过对 `WuwaTappo.exe` 内嵌前端资源（Tauri asset 表，明文键名 + brotli 值，可用 Node `zlib.brotliDecompressSync` 直接雕刻解包）的逆向分析确认：

1. **宝箱生成规则**：每累计 30 分钟活跃时间，宝箱槽生成一个宝箱；双槽（`left_treasure_chest` / `right_treasure_chest`）最多存 2 个，存满后活跃计时暂停；
2. **领取条件**：登录账号 + 消耗 1,000 字符数（点数不足报 `code.70005`，无宝箱报 `code.70004`，冷却中报 `code.70006`）；
3. **前端交互实现**：宝箱就绪时桌宠 WebView 内渲染 **`<button class="reward-btn">`**（左右槽各一），`onClick` 触发 `openChest(place, click_num)` → POST `api/frontend/chests/open`；
4. **关键状态信号**：点击动画期间按钮类名临时附加 `reward-btn--press` / `reward-btn--open`，且按钮进入 `disabled`（前端单飞锁 `Co` 标志）；
5. **官方 UIA 实测验证**：桌宠窗口标题 `Wuwa Tappo`（窗口类 `Tauri Window`），Chromium 将 CSS 类名暴露为 UIA `ClassName` 属性（实测 `settings-btn` 可被 .NET UIAutomation 枚举命中），即 `reward-btn` 按钮在 UIA 树中按类名可直接定位。

### 6.2 实现方案（事件注入 · 方案 A 落地）

采用**纯后台 UI Automation**，彻底避免方案 B 的前台激活与鼠标位移：

- **发现**：`EnumWindows` 按标题精确匹配 `Wuwa Tappo` → `ElementFromHandle` → `FindAllBuildCache(Descendants, ControlType=Button)` 缓存 `ClassName` + `IsEnabled`；
- **触发**：类名**精确等于** `reward-btn`（排除 press/open 动画态）且 `IsEnabled` 为真 → `InvokePattern.Invoke()`，失败回退 `LegacyIAccessible.DoDefaultAction()`——等价 DOM click，零鼠标位移、零焦点抢占；
- **节奏**：常规 10s 一轮扫描（宝箱粒度为 30 分钟）；单轮最多开一箱（官方前端单飞锁）；触发后 3.5s 复查，若按钮仍以初始形态存在视为未生效（点数不足/未登录），进入 5 分钟冷却，避免在桌宠侧刷错误气泡；
- **状态同步**：`get_status` 返回 `auto_box_enabled / auto_box_opens / auto_box_status`（0 未开启 / 1 监测中 / 2 发现宝箱 / 3 开箱成功 / 4 冷却重试），前端 500ms 轮询渲染；开关状态由 localStorage 持久化并经 `set_auto_box` IPC 同步原生线程。

> 逆向解包与 UIA 验证脚本位于仓库外本地工作区 `..\_autotappo_analysis\`（严禁入库——含官方版权资源的解包产物）。

---

## 7. 开发者与 Agent 工作准则

- **严禁提交垃圾文件**：严禁提交临时 Python、AHK、.bat 脚本或编译产物；
- **保持 README.md 纯净**：`README.md` 仅包含用户关心的安装、使用和特色，技术与逆向细节统一沉淀在 `AGENTS.md`；
- **多平台同步**：完成功能后，使用 `git push origin main && git push gitee main` 保持两端同步；
- **打 Tag 发布正式版**：
  ```bash
  git tag v0.1.0
  git push origin v0.1.0
  ```
  GitHub Actions 会全自动打包 Release 并挂载 NSIS 安装包与 Portable 便携版。

//! 自动开箱引擎：后台 UI Automation 无鼠标触发桌宠宝箱按钮
//!
//! 官方桌宠逆向结论（详见 AGENTS.md §6）：
//! - 宝箱每 30 分钟活跃时间生成一个，双槽（left/right）最多存 2 个；
//! - 宝箱就绪时，桌宠 WebView 内渲染 `<button class="reward-btn">`（左/右槽各一），
//!   点击即向后端发起 openChest 请求（消耗 1,000 字符数）；
//!   点击过程中按钮类名会临时附加 `reward-btn--press` / `reward-btn--open`；
//! - 未就绪的槽位渲染 `<div class="reward-countdown" role="img" aria-label="MM:SS">`
//!   倒计时气泡——剩余时间就写在 aria-label 里，UIA 可直接读取。
//!
//! 实机关键结论（v0.3.1 逆向）：桌宠顶层窗口（Tauri/wry 宿主）的 WM_GETOBJECT
//! **不转发** WebView2 内容提供者——原生 UIA 客户端从顶层元素只能读到空树；
//! 内容提供者挂在 WebView2 运行时进程（msedgewebview2.exe）的子窗口
//! `Chrome_WidgetWin_1` / `Chrome_RenderWidgetHostHWND` 上。
//! 因此扫描/保活/领取必须「顶层 + EnumChildWindows 逐子窗口」多根发现。
//!
//! 调度设计（详见 SPEC.md）：
//! - 读倒计时 s 秒 → 精准休眠到就绪后 ~600ms 再扫描领取（就绪后 1~2 秒内开箱）；
//! - 领取 → 3.5 秒复查 → 8 秒链式复查（双槽就绪时第二箱立即连开）；
//! - 复查未生效（点数不足/未登录）→ 回滚计数并冷却 5 分钟；
//! - 20 秒树保活 ping + 3 秒死亡唤醒重试（查询本身触发 Chromium 建树）；
//! - 全程零鼠标位移、零焦点抢占，与连点引擎的「绝对 0 鼠标干涉」准则完全一致。

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use windows::core::{Interface, BOOL};
use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationCacheRequest, IUIAutomationCondition,
    IUIAutomationElement, IUIAutomationInvokePattern, IUIAutomationLegacyIAccessiblePattern,
    TreeScope_Descendants, UIA_ButtonControlTypeId, UIA_ClassNamePropertyId,
    UIA_ControlTypePropertyId, UIA_ImageControlTypeId, UIA_InvokePatternId,
    UIA_IsEnabledPropertyId, UIA_LegacyIAccessiblePatternId, UIA_NamePropertyId,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumChildWindows, EnumWindows, GetWindowTextW, IsWindowVisible,
};

use crate::box_logic::{decide, parse_countdown_text, Phase, ScanOutcome, TREE_WAKE_RETRY_MS};

// 桌宠窗口标题与宝箱按钮类名（来自官方前端逆向，详见 AGENTS.md §6）
const PET_WINDOW_TITLE: &str = "Wuwa Tappo";
const REWARD_BTN_CLASS: &str = "reward-btn";
const REWARD_BTN_PREFIX: &str = "reward-btn";
const COUNTDOWN_CLASS: &str = "reward-countdown";

// 开箱状态码（通过 get_status 同步给前端看板）
pub const BOX_IDLE: u8 = 0; // 未开启自动开箱
pub const BOX_WATCHING: u8 = 1; // 监测中
pub const BOX_FOUND: u8 = 2; // 宝箱即将就绪（倒计时 ≤3s 唤醒窗口）
pub const BOX_OPENED: u8 = 3; // 最近一次开箱成功
pub const BOX_BACKOFF: u8 = 4; // 开箱未生效（点数不足/未登录等），冷却中

/// 等待期间的轮询心跳：既保证能及时响应关闭/开关，又几乎不耗 CPU
const WAIT_HEARTBEAT_MS: u64 = 1000;
/// 树保活 ping 间隔（远短于 Chromium 无障碍树空闲回收窗口）
const KEEPALIVE_TICK_MS: u64 = 20_000;
/// ping 连续失败次数上限，超过后降频为 30 秒（桌宠可能已退出，避免空转）
const TREE_DEAD_STREAK_CAP: u32 = 40;

pub struct BoxOpenerController {
    pub enabled: Arc<AtomicBool>,
    pub opens: Arc<AtomicU64>,
    pub status: Arc<AtomicU8>,
    pub shutdown: Arc<AtomicBool>,
}

impl BoxOpenerController {
    pub fn new() -> Self {
        Self {
            enabled: Arc::new(AtomicBool::new(false)),
            opens: Arc::new(AtomicU64::new(0)),
            status: Arc::new(AtomicU8::new(BOX_IDLE)),
            shutdown: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn start_worker(&self) {
        let enabled = Arc::clone(&self.enabled);
        let opens = Arc::clone(&self.opens);
        let status = Arc::clone(&self.status);
        let shutdown = Arc::clone(&self.shutdown);

        thread::spawn(move || {
            // UIA 客户端推荐 MTA：后台线程无需消息泵
            unsafe {
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            }

            // 单调时钟基准：与 box_logic 纯逻辑共用同一时间轴
            let origin = Instant::now();
            let now_ms = || origin.elapsed().as_millis() as u64;

            let mut toolkit: Option<UiaToolkit> = None;
            let mut phase = Phase::Watching;
            let mut next_scan_at = 0u64; // 首轮立即扫描
            let mut next_ping_at = 0u64; // 首轮立即保活
            let mut tree_dead_streak = 0u32;

            loop {
                if shutdown.load(Ordering::Relaxed) {
                    break;
                }

                if !enabled.load(Ordering::Relaxed) {
                    status.store(BOX_IDLE, Ordering::Relaxed);
                    phase = Phase::Watching;
                    next_scan_at = 0;
                    next_ping_at = 0;
                    tree_dead_streak = 0;
                    thread::sleep(Duration::from_millis(500));
                    continue;
                }

                // 心跳式等待：到点前每秒醒来检查一次开关/退出信号，CPU 占用可忽略
                let now = now_ms();
                let next_event = next_scan_at.min(next_ping_at);
                if now < next_event {
                    if status.load(Ordering::Relaxed) == BOX_IDLE {
                        status.store(BOX_WATCHING, Ordering::Relaxed);
                    }
                    let remain = next_event - now;
                    thread::sleep(Duration::from_millis(remain.min(WAIT_HEARTBEAT_MS)));
                    continue;
                }

                let now = now_ms();
                let mut scan_deferred = false;

                // 惰性创建 UIA 工具集（条件与缓存请求只建一次，跨扫描复用）
                if toolkit.is_none() {
                    toolkit = UiaToolkit::create();
                }

                // ---- 树保活 / 死亡唤醒 ----
                // ping 只看子窗口根（顶层天然无内容提供者，不作为死亡判据）：
                // 任一子窗口可枚举按钮 = 树存活；全部失败 = 触发快速唤醒重试
                // （查询本身会触发 Chromium 重建无障碍树，3 秒内即可恢复）。
                if now >= next_ping_at {
                    let handles = find_pet_windows();
                    let ping_roots: Vec<isize> = handles
                        .iter()
                        .flat_map(|&h| collect_scan_roots(HWND(h as *mut c_void)))
                        .collect();
                    let mut any_dead = ping_roots.len() <= 1;
                    if ping_roots.len() > 1 {
                        if let Some(tk) = &toolkit {
                            any_dead = true;
                            for &r in ping_roots.iter().skip(1) {
                                let hwnd = HWND(r as *mut c_void);
                                if unsafe { tree_alive(tk, hwnd) } {
                                    any_dead = false;
                                    break;
                                }
                            }
                        }
                    }
                    if any_dead {
                        tree_dead_streak = tree_dead_streak.saturating_add(1);
                        // 连续唤醒失败（桌宠可能已退出或异常）：降频至 30 秒避免空转
                        next_ping_at = now + if tree_dead_streak >= TREE_DEAD_STREAK_CAP {
                            30_000
                        } else {
                            TREE_WAKE_RETRY_MS
                        };
                        // 若本次唤醒恰好也是扫描时点：推迟到树苏醒后再观察，避免读到空树误判
                        if now >= next_scan_at {
                            next_scan_at = next_ping_at;
                            scan_deferred = true;
                        }
                    } else {
                        tree_dead_streak = 0;
                        next_ping_at = now + KEEPALIVE_TICK_MS;
                    }
                }

                // ---- 到点全量观察与决策 ----
                // 树死亡期间跳过扫描：空树会误判为 Nothing，
                // 对 PendingVerify 尤其危险（误确认领取成功）。等树唤醒后再扫。
                if !scan_deferred && tree_dead_streak == 0 && now >= next_scan_at {
                    let Some(tk) = &toolkit else {
                        thread::sleep(Duration::from_millis(2000));
                        continue;
                    };

                    let handles = find_pet_windows();
                    let roots: Vec<isize> = handles
                        .iter()
                        .flat_map(|&h| collect_scan_roots(HWND(h as *mut c_void)))
                        .collect();
                    let mut outcome = ScanOutcome::Nothing;
                    let mut uia_broken = false;

                    for &r in &roots {
                        if shutdown.load(Ordering::Relaxed) || !enabled.load(Ordering::Relaxed) {
                            break;
                        }
                        let hwnd = HWND(r as *mut c_void);
                        match unsafe { observe_window(tk, hwnd) } {
                            Ok(o) => outcome = merge_outcome(outcome, o),
                            Err(e) => {
                                eprintln!("[AutoTappo][开箱] UIA 扫描失败，将重建自动化对象: {e}");
                                uia_broken = true;
                                break;
                            }
                        }
                        // 已发现可领取按钮，无需继续扫其余窗口
                        if matches!(outcome, ScanOutcome::Ready(_)) {
                            break;
                        }
                    }
                    if uia_broken {
                        toolkit = None;
                        thread::sleep(Duration::from_millis(2000));
                        continue;
                    }

                    // 纯逻辑决策（box_logic，BDD 全覆盖）
                    let decision = decide(phase, outcome, now);

                    // 触发结果跨作用域使用：opens 计数只在确实触发时预记
                    let mut claimed = false;

                    if decision.claim {
                        // 从就绪根中领取一个就绪按钮
                        let mut claim_broken = false;
                        for &r in &roots {
                            let hwnd = HWND(r as *mut c_void);
                            match unsafe { claim_one(tk, hwnd) } {
                                Ok(true) => {
                                    claimed = true;
                                    break;
                                }
                                Ok(false) => {}
                                Err(e) => {
                                    eprintln!("[AutoTappo][开箱] 领取触发失败: {e}");
                                    claim_broken = true;
                                    break;
                                }
                            }
                        }
                        if claim_broken {
                            toolkit = None;
                        }
                        if claimed {
                            println!("[AutoTappo][开箱] 发现就绪宝箱，已后台触发开箱");
                        }
                    }

                    match decision.opens_delta {
                        1 => {
                            if claimed {
                                opens.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                        -1 => {
                            let _ = opens.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
                                Some(n.saturating_sub(1))
                            });
                            println!("[AutoTappo][开箱] 上次触发未生效（点数不足/未登录），冷却重试");
                        }
                        _ => {}
                    }

                    // 状态码同步给前端看板
                    status.store(
                        if decision.claim {
                            BOX_OPENED
                        } else if matches!(decision.next_phase, Phase::BackoffUntil { .. }) {
                            BOX_BACKOFF
                        } else if matches!(phase, Phase::PendingVerify { .. })
                            && matches!(decision.next_phase, Phase::Watching)
                        {
                            BOX_OPENED
                        } else if matches!(outcome, ScanOutcome::Countdown(s) if s <= 3) {
                            BOX_FOUND
                        } else {
                            BOX_WATCHING
                        },
                        Ordering::Relaxed,
                    );

                    phase = decision.next_phase;
                    next_scan_at = now + decision.next_scan_in_ms;
                }
            }
        });
    }
}

/// UIA 工具集：自动化对象 + 复用条件与缓存请求，避免每轮扫描重复创建
struct UiaToolkit {
    uia: IUIAutomation,
    btn_cond: IUIAutomationCondition,
    img_cond: IUIAutomationCondition,
    cache: IUIAutomationCacheRequest,
}

impl UiaToolkit {
    fn create() -> Option<Self> {
        unsafe {
            let uia: IUIAutomation =
                match CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) {
                    Ok(uia) => uia,
                    Err(e) => {
                        eprintln!("[AutoTappo][开箱] 创建 UIAutomation 失败: {e}");
                        return None;
                    }
                };
            let btn_cond = uia
                .CreatePropertyCondition(
                    UIA_ControlTypePropertyId,
                    &VARIANT::from(UIA_ButtonControlTypeId.0),
                )
                .ok()?;
            // 倒计时气泡 div 带 role="img"，在 UIA 中是 Image 控件，Name 即 aria-label 倒计时文本
            let img_cond = uia
                .CreatePropertyCondition(
                    UIA_ControlTypePropertyId,
                    &VARIANT::from(UIA_ImageControlTypeId.0),
                )
                .ok()?;
            let cache = uia.CreateCacheRequest().ok()?;
            cache.AddProperty(UIA_ClassNamePropertyId).ok()?;
            cache.AddProperty(UIA_NamePropertyId).ok()?;
            cache.AddProperty(UIA_IsEnabledPropertyId).ok()?;
            Some(Self { uia, btn_cond, img_cond, cache })
        }
    }
}

/// 枚举所有可见的桌宠窗口句柄（标题精确匹配，排除本工具自身面板）
fn find_pet_windows() -> Vec<isize> {
    struct Ctx(Vec<isize>);

    unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let ctx = &mut *(lparam.0 as *mut Ctx);
        if IsWindowVisible(hwnd).as_bool() {
            let mut buf = [0u16; 64];
            let n = GetWindowTextW(hwnd, &mut buf);
            let title = String::from_utf16_lossy(&buf[..n as usize]);
            if title == PET_WINDOW_TITLE {
                ctx.0.push(hwnd.0 as isize);
            }
        }
        true.into()
    }

    let mut ctx = Ctx(Vec::new());
    unsafe {
        let _ = EnumWindows(Some(enum_proc), LPARAM(&mut ctx as *mut Ctx as isize));
    }
    ctx.0
}

/// 收集一个桌宠窗口的全部 UIA 扫描根：顶层窗口本身 + 所有 Win32 子窗口。
/// 实机逆向：桌宠顶层窗口（Tauri/wry 宿主）的 WM_GETOBJECT 不转发 WebView2 内容提供者，
/// 内容提供者挂在 WebView2 运行时进程的子窗口（Chrome_WidgetWin_1 等）上。
fn collect_scan_roots(top: HWND) -> Vec<isize> {
    struct C(Vec<isize>);
    unsafe extern "system" fn cb(hwnd: HWND, lp: LPARAM) -> BOOL {
        let c = &mut *(lp.0 as *mut C);
        c.0.push(hwnd.0 as isize);
        true.into()
    }
    let mut c = C(vec![top.0 as isize]);
    unsafe {
        let _ = EnumChildWindows(Some(top), Some(cb), LPARAM(&mut c as *mut C as isize));
    }
    c.0
}

/// 树存活探测：任一按钮可枚举即认为无障碍树存活（底栏 settings-btn 恒在）
unsafe fn tree_alive(tk: &UiaToolkit, hwnd: HWND) -> bool {
    match tk.uia.ElementFromHandle(hwnd) {
        Ok(root) => root
            .FindFirstBuildCache(TreeScope_Descendants, &tk.btn_cond, &tk.cache)
            .is_ok(),
        Err(_) => false,
    }
}

/// 聚合多扫描根结果：优先级 Ready > Animating > Countdown(取最小) > Nothing
fn merge_outcome(acc: ScanOutcome, incoming: ScanOutcome) -> ScanOutcome {
    match (acc, incoming) {
        (ScanOutcome::Ready(n), _) | (_, ScanOutcome::Ready(n)) => ScanOutcome::Ready(n),
        (ScanOutcome::Animating, _) | (_, ScanOutcome::Animating) => ScanOutcome::Animating,
        (ScanOutcome::Countdown(a), ScanOutcome::Countdown(b)) => ScanOutcome::Countdown(a.min(b)),
        (ScanOutcome::Countdown(a), ScanOutcome::Nothing) => ScanOutcome::Countdown(a),
        (ScanOutcome::Nothing, ScanOutcome::Countdown(b)) => ScanOutcome::Countdown(b),
        (ScanOutcome::Nothing, ScanOutcome::Nothing) => ScanOutcome::Nothing,
    }
}

/// 观察一个扫描根的宝箱状态：就绪按钮数 / 动画中 / 最早倒计时
unsafe fn observe_window(tk: &UiaToolkit, hwnd: HWND) -> windows::core::Result<ScanOutcome> {
    let root = tk.uia.ElementFromHandle(hwnd)?;

    // 1. 按钮态：reward-btn 精确匹配计为就绪；带 --press/--open 变体计为动画中
    let btns = root.FindAllBuildCache(TreeScope_Descendants, &tk.btn_cond, &tk.cache)?;
    let mut ready = 0u32;
    let mut animating = false;
    for i in 0..btns.Length()? {
        let el = btns.GetElement(i)?;
        let class = el.CachedClassName()?.to_string();
        let class = class.trim();
        if class == REWARD_BTN_CLASS {
            if el.CachedIsEnabled()?.as_bool() {
                ready += 1;
            }
        } else if class.starts_with(REWARD_BTN_PREFIX) {
            animating = true;
        }
    }
    if ready > 0 {
        return Ok(ScanOutcome::Ready(ready));
    }
    if animating {
        return Ok(ScanOutcome::Animating);
    }

    // 2. 倒计时态：reward-countdown 气泡的 UIA Name = aria-label = "MM:SS"
    let imgs = root.FindAllBuildCache(TreeScope_Descendants, &tk.img_cond, &tk.cache)?;
    let mut earliest: Option<u32> = None;
    for i in 0..imgs.Length()? {
        let el = imgs.GetElement(i)?;
        let class = el.CachedClassName()?.to_string();
        if class.trim() != COUNTDOWN_CLASS {
            continue;
        }
        let name = el.CachedName()?.to_string();
        if let Some(secs) = parse_countdown_text(&name) {
            earliest = Some(match earliest {
                Some(m) => m.min(secs),
                None => secs,
            });
        }
    }

    Ok(match earliest {
        Some(s) => ScanOutcome::Countdown(s),
        None => ScanOutcome::Nothing,
    })
}

/// 领取一个就绪按钮：UIA Invoke 标准路径，失败时回退 MSAA DoDefaultAction。
/// 返回 Ok(false) 表示该扫描根当前没有可领取按钮。
unsafe fn claim_one(tk: &UiaToolkit, hwnd: HWND) -> windows::core::Result<bool> {
    let root = tk.uia.ElementFromHandle(hwnd)?;
    let btns = root.FindAllBuildCache(TreeScope_Descendants, &tk.btn_cond, &tk.cache)?;
    for i in 0..btns.Length()? {
        let el = btns.GetElement(i)?;
        let class = el.CachedClassName()?.to_string();
        if class.trim() != REWARD_BTN_CLASS || !el.CachedIsEnabled()?.as_bool() {
            continue;
        }
        invoke_element(&el)?;
        return Ok(true);
    }
    Ok(false)
}

/// 无鼠标触发按钮：UIA Invoke 标准路径，失败时回退 MSAA DoDefaultAction
unsafe fn invoke_element(el: &IUIAutomationElement) -> windows::core::Result<()> {
    if let Ok(unk) = el.GetCurrentPattern(UIA_InvokePatternId) {
        if let Ok(pattern) = unk.cast::<IUIAutomationInvokePattern>() {
            return pattern.Invoke();
        }
    }
    let unk = el.GetCurrentPattern(UIA_LegacyIAccessiblePatternId)?;
    let pattern = unk.cast::<IUIAutomationLegacyIAccessiblePattern>()?;
    pattern.DoDefaultAction()
}

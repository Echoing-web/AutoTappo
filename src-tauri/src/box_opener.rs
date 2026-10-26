//! 自动开箱引擎：后台 UI Automation 无鼠标触发桌宠宝箱按钮
//!
//! 官方桌宠逆向结论（详见 AGENTS.md §6）：
//! - 宝箱每 30 分钟活跃时间生成一个，双槽（left/right）最多存 2 个；
//! - 宝箱就绪时，桌宠 WebView 内会渲染 `<button class="reward-btn">`（左/右槽各一），
//!   点击即向后端发起 openChest 请求（消耗 1,000 字符数）；
//!   点击过程中按钮类名会临时附加 `reward-btn--press` / `reward-btn--open`；
//! - 因此只需在后台通过 UIA 精确匹配该按钮并触发 Invoke，
//!   全程零鼠标位移、零焦点抢占，与连点引擎的「绝对 0 鼠标干涉」准则完全一致。
//!
//! 防呆设计：
//! - 仅精确匹配初始形态类名 `reward-btn`（排除开箱动画中的 press/open 状态），避免重复触发；
//! - 官方前端持有单飞锁（claim 中禁用按钮），UIA 侧同时检查 IsEnabled 双保险；
//! - 触发后延时复查：按钮仍以初始形态存在则视为本次未生效（点数不足/未登录等），
//!   进入冷却期，避免在桌宠侧刷错误提示。

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
    CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationInvokePattern,
    IUIAutomationLegacyIAccessiblePattern, TreeScope_Descendants, UIA_ButtonControlTypeId,
    UIA_ClassNamePropertyId, UIA_ControlTypePropertyId, UIA_InvokePatternId,
    UIA_IsEnabledPropertyId, UIA_LegacyIAccessiblePatternId,
};
use windows::Win32::UI::WindowsAndMessaging::{EnumWindows, GetWindowTextW, IsWindowVisible};

// 桌宠窗口标题与宝箱按钮类名（来自官方前端逆向，详见 AGENTS.md §6）
const PET_WINDOW_TITLE: &str = "Wuwa Tappo";
const REWARD_BTN_CLASS: &str = "reward-btn";

// 开箱状态码（通过 get_status 同步给前端看板）
pub const BOX_IDLE: u8 = 0; // 未开启自动开箱
pub const BOX_WATCHING: u8 = 1; // 监测中
pub const BOX_FOUND: u8 = 2; // 发现就绪宝箱
pub const BOX_OPENED: u8 = 3; // 最近一次开箱成功
pub const BOX_BACKOFF: u8 = 4; // 开箱未生效（点数不足/未登录等），冷却中

const SCAN_INTERVAL_SECS: u64 = 10; // 常规扫描间隔（宝箱以 30 分钟为粒度，无需高频）
const CLAIM_VERIFY_MS: u64 = 3500; // 触发后复查等待（需覆盖官方按压缩放动画时长）
const BACKOFF_MS: u64 = 5 * 60 * 1000; // 开箱未生效后的冷却期
const UIA_RETRY_MS: u64 = 2000; // UIA 实例创建失败重试间隔

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

            let mut automation: Option<IUIAutomation> = None;
            // 首轮立即扫描
            let mut last_scan = Instant::now() - Duration::from_secs(SCAN_INTERVAL_SECS);
            // 触发开箱后进入「复查模式」：CLAIM_VERIFY_MS 后再扫一次确认是否生效
            let mut last_attempt: Option<Instant> = None;
            let mut backoff_until: Option<Instant> = None;

            loop {
                if shutdown.load(Ordering::Relaxed) {
                    break;
                }

                if !enabled.load(Ordering::Relaxed) {
                    status.store(BOX_IDLE, Ordering::Relaxed);
                    last_attempt = None;
                    backoff_until = None;
                    thread::sleep(Duration::from_millis(500));
                    continue;
                }

                if let Some(t) = backoff_until {
                    if Instant::now() < t {
                        status.store(BOX_BACKOFF, Ordering::Relaxed);
                        thread::sleep(Duration::from_millis(500));
                        continue;
                    }
                    backoff_until = None;
                }

                // 计算下次扫描时间：复查模式 3.5s 后立刻复查；常规模式 10s 一轮
                let due = match last_attempt {
                    Some(t) => t + Duration::from_millis(CLAIM_VERIFY_MS),
                    None => last_scan + Duration::from_secs(SCAN_INTERVAL_SECS),
                };
                let now = Instant::now();
                if now < due {
                    if status.load(Ordering::Relaxed) == BOX_IDLE {
                        status.store(BOX_WATCHING, Ordering::Relaxed);
                    }
                    thread::sleep(Duration::from_millis(400));
                    continue;
                }
                last_scan = now;

                // 惰性创建 UIA 自动化对象
                if automation.is_none() {
                    automation = create_automation();
                }
                let Some(uia) = &automation else {
                    thread::sleep(Duration::from_millis(UIA_RETRY_MS));
                    continue;
                };

                let handles = find_pet_windows();
                let mut claimed_now = false;
                let mut found_ready = false;
                let mut uia_broken = false;

                for &h in &handles {
                    if shutdown.load(Ordering::Relaxed) || !enabled.load(Ordering::Relaxed) {
                        break;
                    }
                    let hwnd = HWND(h as *mut c_void);
                    match unsafe { scan_and_claim(uia, hwnd) } {
                        Ok(claimed) => {
                            if claimed {
                                claimed_now = true;
                                found_ready = true;
                                break; // 官方前端持单飞锁，一轮只开一箱
                            }
                        }
                        Err(e) => {
                            eprintln!("[AutoTappo][开箱] UIA 扫描失败，将重建自动化对象: {e}");
                            automation = None;
                            uia_broken = true;
                            break;
                        }
                    }
                }
                if uia_broken {
                    thread::sleep(Duration::from_millis(UIA_RETRY_MS));
                    continue;
                }

                if claimed_now {
                    // 触发成功：预记一次开箱，稍后复查确认是否真正生效
                    opens.fetch_add(1, Ordering::Relaxed);
                    status.store(BOX_OPENED, Ordering::Relaxed);
                    last_attempt = Some(Instant::now());
                } else if last_attempt.is_some() {
                    // 复查扫描：按钮仍以初始形态存在 → 上一次触发未生效（点数不足/未登录等）
                    if found_ready {
                        // 回滚预记的开箱计数，进入冷却期，避免在桌宠侧刷错误气泡
                        let _ = opens.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
                            Some(n.saturating_sub(1))
                        });
                        backoff_until = Some(Instant::now() + Duration::from_millis(BACKOFF_MS));
                        status.store(BOX_BACKOFF, Ordering::Relaxed);
                    }
                    last_attempt = None;
                } else {
                    status.store(
                        if found_ready { BOX_FOUND } else { BOX_WATCHING },
                        Ordering::Relaxed,
                    );
                }
            }
        });
    }
}

fn create_automation() -> Option<IUIAutomation> {
    unsafe {
        let instance = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER);
        match instance {
            Ok(uia) => Some(uia),
            Err(e) => {
                eprintln!("[AutoTappo][开箱] 创建 UIAutomation 失败: {e}");
                None
            }
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

/// 扫描一个桌宠窗口：发现就绪宝箱按钮则后台触发，返回是否已触发
unsafe fn scan_and_claim(uia: &IUIAutomation, hwnd: HWND) -> windows::core::Result<bool> {
    // 缓存 ClassName + IsEnabled，避免逐元素跨进程取值
    let cache = uia.CreateCacheRequest()?;
    cache.AddProperty(UIA_ClassNamePropertyId)?;
    cache.AddProperty(UIA_IsEnabledPropertyId)?;

    // 仅匹配按钮控件，Chromium 内部树遍历远快于 TrueCondition 全树展开
    let cond = uia.CreatePropertyCondition(
        UIA_ControlTypePropertyId,
        &VARIANT::from(UIA_ButtonControlTypeId.0),
    )?;

    let root = uia.ElementFromHandle(hwnd)?;
    let arr = root.FindAllBuildCache(TreeScope_Descendants, &cond, &cache)?;
    let count = arr.Length()?;

    for i in 0..count {
        let el = arr.GetElement(i)?;
        // 精确匹配初始形态：排除 reward-btn--press / reward-btn--open（开箱动画中）
        let class = el.CachedClassName()?.to_string();
        if class.trim() != REWARD_BTN_CLASS {
            continue;
        }
        // 官方 claim 单飞锁期间按钮 disabled，双保险跳过
        if !el.CachedIsEnabled()?.as_bool() {
            continue;
        }
        invoke_element(&el)?;
        println!("[AutoTappo][开箱] 发现就绪宝箱，已后台触发开箱");
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

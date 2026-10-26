# AutoTappo v0.3.0 实机 E2E 冒烟测试（SDD REQ-TEST-03 / REQ-PERF-01/02/04）
# 流程：启动 release exe → 按启动进程 PID 定位本实例面板 → UIA 驱动开箱开关 ON/OFF 与挂机开关
#       → 读取状态文本验证 IPC 联动 → 采样 CPU/内存 → 优雅退出。
# 用法：powershell -NoProfile -ExecutionPolicy Bypass -File src-tauri\tests\local-e2e.ps1 [-SampleSecs 45]
#（脚本会自动优先换道到 pwsh7；也可直接 pwsh -NoProfile -File ... 调用）

param(
    [string]$ExePath = "D:\WutheringWaves-Optimizer\AutoTappo\src-tauri\target\release\AutoTappo.exe",
    [int]$SampleSecs = 45
)

# 默认优先 pwsh7（项目引擎约定）：检测到 Windows PowerShell 5.1 且存在 pwsh 时自动换道重跑
if ($PSVersionTable.PSVersion.Major -lt 7) {
    $pwshCmd = Get-Command pwsh -ErrorAction SilentlyContinue
    if ($pwshCmd) {
        & $pwshCmd.Source -NoProfile -ExecutionPolicy Bypass -File $PSCommandPath @PSBoundParameters
        exit $LASTEXITCODE
    }
}

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes

function Find-ByAutomationId([System.Windows.Automation.AutomationElement]$root, [string]$id) {
    $cond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::AutomationIdProperty, $id)
    return $root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $cond)
}

# WebView2 首次被 UIA 客户端连接时会异步开启渲染进程无障碍模式并建树，需要重试等待
function Find-ByAutomationIdWithRetry([System.Windows.Automation.AutomationElement]$root, [string]$id, [int]$timeoutSecs = 15) {
    $deadline = (Get-Date).AddSeconds($timeoutSecs)
    while ((Get-Date) -lt $deadline) {
        $el = Find-ByAutomationId $root $id
        if ($null -ne $el) { return $el }
        Start-Sleep -Milliseconds 500
    }
    return $null
}

function Dump-Buttons([System.Windows.Automation.AutomationElement]$root) {
    $cond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::Button)
    $btns = $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, $cond)
    Write-Host ("  [diag] buttons visible: " + $btns.Count)
    foreach ($b in $btns) {
        Write-Host ("    class='" + $b.Current.ClassName + "' id='" + $b.Current.AutomationId + "' name='" + $b.Current.Name + "'")
    }
}

function Get-UiaName([System.Windows.Automation.AutomationElement]$el) {
    if ($null -eq $el) { return '<null>' }
    return $el.Current.Name
}

function Invoke-Element([System.Windows.Automation.AutomationElement]$el) {
    $pat = $el.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)
    $pat.Invoke()
}

# 三重降级调用：InvokePattern → TogglePattern → MSAA DoDefaultAction（与 Rust 引擎同策略）
function Invoke-PatternTolerant([System.Windows.Automation.AutomationElement]$el) {
    try {
        $pat = $el.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)
        $pat.Invoke()
        return $true
    } catch {}
    try {
        $pat = $el.GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern)
        $pat.Toggle()
        return $true
    } catch {}
    try {
        $legacy = $el.GetCurrentPattern([System.Windows.Automation.LegacyIAccessiblePattern]::Pattern)
        $legacy.DoDefaultAction()
        return $true
    } catch {}
    return $false
}

# 韧性调用：WebView2 无障碍树在建树/重建期间会短暂出现"元素在但模式不可用/已失效"，
# 统一重取元素重试，规避偶发 InvalidOperationException
function Invoke-ElementSafe([System.Windows.Automation.AutomationElement]$root, [string]$id, [int]$tries = 8) {
    for ($i = 0; $i -lt $tries; $i++) {
        $el = Find-ByAutomationId $root $id
        if ($el) {
            if (Invoke-PatternTolerant $el) { return $true }
            Start-Sleep -Milliseconds 800
        } else {
            Start-Sleep -Milliseconds 800
        }
    }
    return $false
}

# 状态文本读取：role=status 容器的 Name 为空时，回退读其首个 Text 子节点
function Get-StatusText([System.Windows.Automation.AutomationElement]$root, [string]$id, [int]$timeoutSecs = 10) {
    $deadline = (Get-Date).AddSeconds($timeoutSecs)
    while ((Get-Date) -lt $deadline) {
        $el = Find-ByAutomationId $root $id
        if ($el) {
            if ($el.Current.Name) { return $el.Current.Name }
            $textCond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::Text)
            $child = $el.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $textCond)
            if ($child -and $child.Current.Name) { return $child.Current.Name }
        }
        Start-Sleep -Milliseconds 500
    }
    return $null
}

# 韧性读取：等待元素出现且 Name 非空（role=status 文本由 IPC 轮询驱动更新）
function Get-UiaNameWithRetry([System.Windows.Automation.AutomationElement]$root, [string]$id, [int]$timeoutSecs = 10) {
    $deadline = (Get-Date).AddSeconds($timeoutSecs)
    while ((Get-Date) -lt $deadline) {
        $el = Find-ByAutomationId $root $id
        if ($el -and $el.Current.Name) { return $el.Current.Name }
        Start-Sleep -Milliseconds 500
    }
    return $null
}

$failures = New-Object System.Collections.Generic.List[string]
function Check([string]$name, [bool]$ok, [string]$detail) {
    if ($ok) { Write-Host ("PASS  {0}  {1}" -f $name, $detail) -ForegroundColor Green }
    else { Write-Host ("FAIL  {0}  {1}" -f $name, $detail) -ForegroundColor Red; $failures.Add($name) }
}

# ---------- 1. 启动 ----------
$before = Get-Process AutoTappo -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Id
$proc = Start-Process $ExePath -PassThru
Start-Sleep -Seconds 6
$alive = -not $proc.HasExited
Check "launch" $alive ("pid=" + $proc.Id)
if (-not $alive) { throw "进程启动即退出，无法继续" }

# ---------- 2. 按 PID 定位本实例面板窗口 ----------
$root = [System.Windows.Automation.AutomationElement]::RootElement
$pidCond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ProcessIdProperty, $proc.Id)
$titleCond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, "AutoTappo · 鸣潮桌宠挂机助手")
$both = New-Object System.Windows.Automation.AndCondition($pidCond, $titleCond)
$win = $root.FindFirst([System.Windows.Automation.TreeScope]::Children, $both)
Check "panel-window" ($null -ne $win) ("hwnd=" + $(if ($win) { $win.Current.NativeWindowHandle } else { '<null>' }))

if ($win) {
    # ---------- 3. 自动开箱开关闭环 ----------
    $abToggle = Find-ByAutomationIdWithRetry $win "btn-auto-box"
    Check "find-ab-toggle" ($null -ne $abToggle) "AutomationId=btn-auto-box"

    $descBefore = Get-StatusText $win "auto-box-desc" 8
    Check "ab-desc-initial" ($descBefore -like "*1,000*") ("text='" + $descBefore + "'")

    if ($abToggle) {
        # 状态无关断言：开关状态由 localStorage 持久化，初始可能为 ON 或 OFF
        $descBase = if ($descBefore) { $descBefore } else { '' }

        $onOk = Invoke-ElementSafe $win "btn-auto-box"   # 翻转一次
        Check "ab-invoke-1" $onOk "UIA Invoke btn-auto-box"
        Start-Sleep -Seconds 2
        $descFlip = Get-StatusText $win "auto-box-desc" 8
        Check "ab-toggle-flip" ($onOk -and $descFlip -ne $descBase) ("'" + $descBase + "' -> '" + $descFlip + "'")

        Start-Sleep -Seconds $SampleSecs   # 资源采样窗口（开箱引擎运行中）

        $offOk = Invoke-ElementSafe $win "btn-auto-box"   # 翻转还原
        Check "ab-invoke-2" $offOk "UIA Invoke btn-auto-box"
        Start-Sleep -Seconds 2
        $descRestore = Get-StatusText $win "auto-box-desc" 8
        Check "ab-toggle-restore" ($offOk -and $descRestore -eq $descBase) ("restored='" + $descRestore + "'")
    }

    # ---------- 4. 挂机开关闭环 ----------
    $btnToggle = Find-ByAutomationIdWithRetry $win "btn-toggle" 5
    $badge = Find-ByAutomationIdWithRetry $win "status-badge" 5
    if ($btnToggle -and $badge) {
        $tOn = Invoke-ElementSafe $win "btn-toggle"
        Start-Sleep -Seconds 3
        $badgeOn = Get-StatusText $win "status-badge" 8
        Check "clicker-on" ($tOn -and $badgeOn -eq "运行中") ("badge='" + $badgeOn + "'")
        Invoke-ElementSafe $win "btn-toggle" | Out-Null
        Start-Sleep -Seconds 2
        $badgeOff = Get-StatusText $win "status-badge" 8
        Check "clicker-off" ($badgeOff -eq "已暂停") ("badge='" + $badgeOff + "'")
    } else {
        Dump-Buttons $win
        Check "clicker-on" $false "btn-toggle/status-badge 未找到"
    }

    # ---------- 4.5 测试残留清理：把自动开箱持久化状态归位为 OFF ----------
    $descFinal = Get-StatusText $win "auto-box-desc" 5
    if ($descFinal -and ($descFinal -like '*监测中*' -or $descFinal -like '*点数不足*' -or $descFinal -like '*发现*')) {
        Invoke-ElementSafe $win "btn-auto-box" | Out-Null
        Start-Sleep -Seconds 1
    }
}

# ---------- 5. 资源占用采样（REQ-PERF） ----------
$p = Get-Process -Id $proc.Id -ErrorAction SilentlyContinue
if ($p) {
    $cpu1 = $p.TotalProcessorTime.TotalMilliseconds
    $t1 = Get-Date
    Start-Sleep -Seconds 20
    $p.Refresh()
    $cpu2 = $p.TotalProcessorTime.TotalMilliseconds
    $dt = ((Get-Date) - $t1).TotalMilliseconds
    $cpuPct = [math]::Round(100.0 * ($cpu2 - $cpu1) / $dt / [Environment]::ProcessorCount, 2)
    $memMB = [math]::Round($p.WorkingSet64 / 1MB, 1)
    Check "cpu-usage" ($cpuPct -le 2.0) ("avg=" + $cpuPct + "% (all cores)")
    Check "mem-workingset" ($memMB -le 60) ($memMB.ToString() + " MB")
} else {
    Check "cpu-usage" $false "进程已退出无法采样"
}

# ---------- 6. 优雅退出（quit_app IPC） ----------
$quitBtn = if ($win) { Find-ByAutomationId $win "btn-quit" } else { $null }
if ($quitBtn) {
    Invoke-Element $quitBtn
    Start-Sleep -Seconds 3
    $gone = $null -eq (Get-Process -Id $proc.Id -ErrorAction SilentlyContinue)
    Check "graceful-quit" $gone "quit_app IPC"
} else {
    Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
    Check "graceful-quit" $false "btn-quit 未找到，已强制结束"
}

Write-Host ""
if ($failures.Count -eq 0) { Write-Host "E2E RESULT: ALL PASS" -ForegroundColor Green; exit 0 }
else { Write-Host ("E2E RESULT: {0} FAILED -> {1}" -f $failures.Count, ($failures -join ", ")) -ForegroundColor Red; exit 1 }

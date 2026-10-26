# 探测桌宠宝箱槽状态：reward-countdown 的 aria-label 倒计时 / reward-btn 就绪按钮
$ErrorActionPreference = 'SilentlyContinue'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes

$root = [System.Windows.Automation.AutomationElement]::RootElement
$cond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, 'Wuwa Tappo')
$wins = $root.FindAll([System.Windows.Automation.TreeScope]::Children, $cond)
Write-Output ("pet windows: " + $wins.Count)
foreach ($w in $wins) {
    $imgCond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::Image)
    $imgs = $w.FindAll([System.Windows.Automation.TreeScope]::Descendants, $imgCond)
    $btnCond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::Button)
    $btns = $w.FindAll([System.Windows.Automation.TreeScope]::Descendants, $btnCond)
    foreach ($b in $btns) {
        if ($b.Current.ClassName -like 'reward-btn*') {
            Write-Output ("READY-BUTTON class='" + $b.Current.ClassName + "' enabled=" + $b.Current.IsEnabled)
        }
    }
    foreach ($img in $imgs) {
        if ($img.Current.ClassName -eq 'reward-countdown') {
            Write-Output ("COUNTDOWN slot aria-label='" + $img.Current.Name + "'")
        }
    }
}

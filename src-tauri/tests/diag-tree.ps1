# 诊断：dump 本实例面板的 UIA 子树（ControlType/ClassName/AutomationId/Name/patterns）
# 用法：pwsh -NoProfile -File diag-tree.ps1  （需已有一个 AutoTappo 实例在运行）
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes

$proc = Get-Process AutoTappo -ErrorAction SilentlyContinue | Where-Object { $_.Path -like '*target\release*' } | Select-Object -First 1
if (-not $proc) { Write-Host "no release instance running"; exit 1 }
Write-Host ("pid=" + $proc.Id)

$root = [System.Windows.Automation.AutomationElement]::RootElement
$pidCond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ProcessIdProperty, $proc.Id)
$wins = $root.FindAll([System.Windows.Automation.TreeScope]::Children, $pidCond)
Write-Host ("windows: " + $wins.Count)
foreach ($w in $wins) {
    Write-Host ("== window name='" + $w.Current.Name + "' class='" + $w.Current.ClassName + "'")
    $all = $w.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
    Write-Host ("   descendants: " + $all.Count)
    $i = 0
    foreach ($e in $all) {
        $c = $e.Current
        $pats = @()
        foreach ($p in $e.GetSupportedPatterns()) { $pats += $p.ProgrammaticName }
        Write-Host ("   [" + $i + "] ct=" + $c.ControlType.ProgrammaticName + " class='" + $c.ClassName + "' id='" + $c.AutomationId + "' name='" + $c.Name + "' pats=" + ($pats -join ','))
        $i++
        if ($i -ge 40) { break }
    }
}

# Rust Agent Web UI - Windows 服务安装脚本
# 使用 NSSM (Non-Sucking Service Manager)
# 需要管理员权限运行

#Requires -RunAsAdministrator

$ErrorActionPreference = "Stop"

# 配置
$ServiceName = "RustAgentUI"
$ServiceDisplay = "Rust Agent Web UI"
$ServiceDesc = "Rust Agent Web 界面服务"
$Port = 3000
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$DistDir = Join-Path $ScriptDir "dist"

Write-Host "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━" -ForegroundColor Cyan
Write-Host "  安装 Rust Agent Web UI 系统服务" -ForegroundColor Green
Write-Host "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━" -ForegroundColor Cyan
Write-Host ""
Write-Host "服务名称: $ServiceDisplay"
Write-Host "端口: $Port"
Write-Host "安装目录: $DistDir"
Write-Host ""

# 检查 dist 目录
if (-not (Test-Path $DistDir)) {
    Write-Host "❌ dist\ 目录不存在，请先构建:" -ForegroundColor Red
    Write-Host "   npm run build"
    exit 1
}

# 检查 Python
$Python = $null
try {
    $Python = (Get-Command python -ErrorAction Stop).Path
    Write-Host "✓ 找到 Python: $Python" -ForegroundColor Green
} catch {
    try {
        $Python = (Get-Command python3 -ErrorAction Stop).Path
        Write-Host "✓ 找到 Python3: $Python" -ForegroundColor Green
    } catch {
        Write-Host "❌ 未找到 Python，请先安装:" -ForegroundColor Red
        Write-Host "   https://www.python.org/downloads/"
        exit 1
    }
}

# 下载 NSSM
$NssmDir = Join-Path $env:TEMP "nssm"
$NssmExe = Join-Path $NssmDir "nssm.exe"

if (-not (Test-Path $NssmExe)) {
    Write-Host "📥 下载 NSSM..." -ForegroundColor Yellow
    
    $NssmZip = Join-Path $env:TEMP "nssm.zip"
    $NssmUrl = "https://nssm.cc/release/nssm-2.24.zip"
    
    Invoke-WebRequest -Uri $NssmUrl -OutFile $NssmZip
    Expand-Archive -Path $NssmZip -DestinationPath $env:TEMP -Force
    
    # 找到 nssm.exe（根据系统架构）
    $NssmExtracted = Get-ChildItem -Path $env:TEMP -Filter "nssm-*" -Directory | Select-Object -First 1
    $Arch = if ([Environment]::Is64BitOperatingSystem) { "win64" } else { "win32" }
    $NssmBin = Join-Path $NssmExtracted.FullName "nssm-2.24\$Arch\nssm.exe"
    
    New-Item -ItemType Directory -Path $NssmDir -Force | Out-Null
    Copy-Item $NssmBin -Destination $NssmExe -Force
    
    Remove-Item $NssmZip -Force
    Write-Host "✓ NSSM 已就绪" -ForegroundColor Green
}

# 检查服务是否已存在
$ExistingService = Get-Service -Name $ServiceName -ErrorAction SilentlyContinue
if ($ExistingService) {
    Write-Host "⚠️  服务已存在，将先卸载..." -ForegroundColor Yellow
    & $NssmExe stop $ServiceName
    & $NssmExe remove $ServiceName confirm
    Start-Sleep -Seconds 2
}

# 安装服务
Write-Host "⚙️  安装服务..." -ForegroundColor Yellow

& $NssmExe install $ServiceName $Python "-m" "http.server" $Port
& $NssmExe set $ServiceName AppDirectory $DistDir
& $NssmExe set $ServiceName DisplayName $ServiceDisplay
& $NssmExe set $ServiceName Description $ServiceDesc
& $NssmExe set $ServiceName Start SERVICE_AUTO_START

# 配置日志
$LogDir = Join-Path $ScriptDir "logs"
New-Item -ItemType Directory -Path $LogDir -Force | Out-Null

& $NssmExe set $ServiceName AppStdout (Join-Path $LogDir "service.log")
& $NssmExe set $ServiceName AppStderr (Join-Path $LogDir "error.log")

# 启动服务
Write-Host "▶️  启动服务..." -ForegroundColor Yellow
& $NssmExe start $ServiceName

Start-Sleep -Seconds 2

# 检查状态
$Service = Get-Service -Name $ServiceName
if ($Service.Status -eq "Running") {
    Write-Host ""
    Write-Host "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━" -ForegroundColor Cyan
    Write-Host "✅ 安装成功！" -ForegroundColor Green
    Write-Host "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━" -ForegroundColor Cyan
    Write-Host ""
    Write-Host "服务状态: $($Service.Status)" -ForegroundColor Green
    Write-Host "访问地址: http://localhost:$Port"
    Write-Host ""
    Write-Host "常用命令:"
    Write-Host "  查看状态: Get-Service $ServiceName"
    Write-Host "  停止服务: Stop-Service $ServiceName"
    Write-Host "  启动服务: Start-Service $ServiceName"
    Write-Host "  重启服务: Restart-Service $ServiceName"
    Write-Host "  查看日志: Get-Content `"$LogDir\service.log`" -Tail 50 -Wait"
    Write-Host "  卸载服务: $NssmExe remove $ServiceName confirm"
    Write-Host ""
    
    # 自动打开浏览器
    Start-Process "http://localhost:$Port"
} else {
    Write-Host ""
    Write-Host "❌ 服务启动失败，状态: $($Service.Status)" -ForegroundColor Red
    Write-Host "请查看日志: $LogDir\error.log"
    exit 1
}

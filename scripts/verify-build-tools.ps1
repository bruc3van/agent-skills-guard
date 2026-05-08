# 验证 Windows 构建工具
Write-Host "=== 验证 Rust 和 MSVC 工具链 ===" -ForegroundColor Cyan

# 检查 Rust 工具链
Write-Host "`n1. Rust 工具链:" -ForegroundColor Yellow
rustc --version
rustup show

# 检查 MSVC 链接器
Write-Host "`n2. MSVC 链接器:" -ForegroundColor Yellow
$linkExe = Get-Command link.exe -ErrorAction SilentlyContinue
if ($linkExe) {
    Write-Host "找到 link.exe: $($linkExe.Source)" -ForegroundColor Green
    # 验证这不是 Git 的 link.exe
    if ($linkExe.Source -like "*Git*") {
        Write-Host "警告: 这是 Git 的 link.exe，不是 MSVC 的！" -ForegroundColor Red
        Write-Host "请确保 Visual Studio Build Tools 的路径在 Git 路径之前" -ForegroundColor Yellow
    } else {
        Write-Host "✓ MSVC 链接器已正确配置" -ForegroundColor Green
    }
} else {
    Write-Host "✗ 未找到 link.exe" -ForegroundColor Red
    Write-Host "需要安装 Visual Studio Build Tools" -ForegroundColor Yellow
}

# 检查 Visual Studio 安装
Write-Host "`n3. Visual Studio 安装:" -ForegroundColor Yellow
$vsPaths = @(
    "C:\Program Files\Microsoft Visual Studio\2022",
    "C:\Program Files (x86)\Microsoft Visual Studio\2019",
    "C:\Program Files\Microsoft Visual Studio\2019"
)

$found = $false
foreach ($path in $vsPaths) {
    if (Test-Path $path) {
        Write-Host "找到 Visual Studio: $path" -ForegroundColor Green
        $found = $true
    }
}

if (-not $found) {
    Write-Host "未找到 Visual Studio 或 Build Tools" -ForegroundColor Red
}

Write-Host "`n=== 验证完成 ===" -ForegroundColor Cyan

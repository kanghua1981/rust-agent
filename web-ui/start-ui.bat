@echo off
REM Rust Agent Web UI 启动脚本 (Windows)
REM 双击此文件即可启动 Web UI

title Rust Agent Web UI

set PORT=3000
set DIST_DIR=%~dp0dist

echo ============================================
echo    Rust Agent Web UI
echo ============================================
echo.

REM 检查 dist 目录
if not exist "%DIST_DIR%" (
    echo [ERROR] dist\ 目录不存在，请先构建:
    echo    npm run build
    pause
    exit /b 1
)

echo [INFO] 静态文件: %DIST_DIR%
echo [INFO] 端口: %PORT%
echo.

cd /d "%DIST_DIR%"

REM 检查 Python
python --version >nul 2>&1
if %errorlevel% == 0 (
    echo [OK] 使用 Python HTTP 服务器
    echo ============================================
    echo.
    echo    浏览器访问: http://localhost:%PORT%
    echo.
    echo ============================================
    echo 按 Ctrl+C 停止服务
    echo.
    
    REM 自动打开浏览器
    timeout /t 1 /nobreak >nul
    start http://localhost:%PORT%
    
    python -m http.server %PORT%
    goto :end
)

REM 检查 Node.js
where npx >nul 2>&1
if %errorlevel% == 0 (
    echo [OK] 使用 Node serve
    echo ============================================
    echo.
    echo    浏览器访问: http://localhost:%PORT%
    echo.
    echo ============================================
    
    npx serve -s . -p %PORT%
    goto :end
)

REM 都没有
echo [ERROR] 未找到可用的 HTTP 服务器
echo.
echo 请安装以下任意一个:
echo   • Python 3: https://www.python.org/downloads/
echo   • Node.js:  https://nodejs.org/
echo.
pause
exit /b 1

:end
pause

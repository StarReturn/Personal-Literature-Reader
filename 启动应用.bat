@echo off
chcp 65001 >nul
cd /d "%~dp0"
title 文献综述中心 - 本地服务

if not exist dist (
  echo [首次运行] 正在安装依赖并构建前端，可能需要几分钟...
  call npm install --no-audit --no-fund || goto :err
  call npx vite build || goto :err
)

echo 正在启动本地服务（http://127.0.0.1:8787）...
start "" http://127.0.0.1:8787
cargo run -p litreview-dev-server
goto :eof

:err
echo.
echo 启动失败：请确认已安装 Node.js 20+、Rust（MSVC）并完成环境配置。
pause

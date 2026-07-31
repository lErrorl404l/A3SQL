@echo off
REM A3SQL build script for Windows developers
REM Builds the Rust extension DLL then runs HEMTT for the addon

echo === Building A3SQL ===

echo.
echo -- Rust DLL (Windows x86_64) --
cargo build --release --target x86_64-pc-windows-gnu --manifest-path extension\Cargo.toml
if %ERRORLEVEL% neq 0 exit /b %ERRORLEVEL%

echo.
echo -- Copying DLL to project root --
copy /Y extension\target\x86_64-pc-windows-gnu\release\a3sql.dll a3sql_x64.dll
copy /Y extension\target\x86_64-pc-windows-gnu\release\a3sql.dll a3sql.dll

echo.
echo -- Building addon PBOs --
hemtt build
if %ERRORLEVEL% neq 0 exit /b %ERRORLEVEL%

echo.
echo === Build complete ===
echo Output in .hemttout\build\

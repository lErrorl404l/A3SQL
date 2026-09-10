@echo off
REM A3SQL build script for Windows developers
REM Builds the Rust extension DLLs then runs HEMTT for the addon

echo === Building A3SQL ===

echo.
echo -- Rust DLL (Windows x86_64 MSVC) --
cargo build --release --target x86_64-pc-windows-msvc --manifest-path extension\Cargo.toml
if %ERRORLEVEL% neq 0 exit /b %ERRORLEVEL%

echo.
echo -- Rust DLL (Windows i686 MSVC) --
cargo build --release --target i686-pc-windows-msvc --manifest-path extension\Cargo.toml
if %ERRORLEVEL% neq 0 exit /b %ERRORLEVEL%

echo.
echo -- Copying DLLs to project root --
copy /Y extension\target\x86_64-pc-windows-msvc\release\a3sql.dll a3sql_x64.dll
copy /Y extension\target\i686-pc-windows-msvc\release\a3sql.dll a3sql.dll

echo.
echo -- Building addon PBOs --
hemtt build
if %ERRORLEVEL% neq 0 exit /b %ERRORLEVEL%

echo.
echo === Build complete ===
echo Output in .hemttout\build\

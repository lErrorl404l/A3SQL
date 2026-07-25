@echo off
REM A3DB build script for Windows developers
REM Builds the Rust extension DLL then runs HEMTT for the addon

echo === Building A3DB ===

echo.
echo -- Rust DLL (Windows x86_64) --
cargo build --release --target x86_64-pc-windows-gnu -p a3db
if %ERRORLEVEL% neq 0 exit /b %ERRORLEVEL%

echo.
echo -- Copying DLL to project root --
copy /Y target\x86_64-pc-windows-gnu\release\a3db.dll a3db_x64.dll
copy /Y target\x86_64-pc-windows-gnu\release\a3db.dll a3db.dll

echo.
echo -- Building addon PBOs --
hemtt build
if %ERRORLEVEL% neq 0 exit /b %ERRORLEVEL%

echo.
echo === Build complete ===
echo Output in .hemttout\build\

@echo off
:: ──────────────────────────────────────────────────────────────
:: uninstall_java.bat — quick Temurin-17 remover for Windows
:: Usage:  double-click or run from an elevated CMD / PowerShell
:: ──────────────────────────────────────────────────────────────

REM 1️⃣  Verify winget is available
where winget >nul 2>nul || (
    echo [ERROR] winget not found. Install Windows Package Manager first.
    exit /b 1
)

echo ⬅ Uninstalling Eclipse Temurin 17 JDK …

REM 2️⃣  Silent uninstall (no prompts)
winget uninstall --id EclipseAdoptium.Temurin.17.JDK ^
    -e --silent --accept-source-agreements

IF %ERRORLEVEL% NEQ 0 (
    echo [ERROR] winget uninstall failed with exit code %ERRORLEVEL%
    exit /b %ERRORLEVEL%
)

REM 3️⃣  Remove the PATH symlink winget created (harmless if missing)
IF EXIST "%LOCALAPPDATA%\Microsoft\WinGet\Links\java.exe" (
    del "%LOCALAPPDATA%\Microsoft\WinGet\Links\java.exe" >nul 2>nul
)

echo ✅ Temurin 17 removed.

echo.
echo -------- java --version --------
java --version 2>&1 || echo (java command no longer on PATH)

exit /b 0
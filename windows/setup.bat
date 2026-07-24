@echo off
chcp 65001 >nul 2>&1
setlocal

set "VENV_DIR=%~dp0.venv"

echo Creating virtual environment...
python -m venv "%VENV_DIR%"
if errorlevel 1 (
    echo ERROR: Failed to create venv. Make sure Python 3.10+ is installed.
    pause
    exit /b 1
)

echo Installing dependencies...
"%VENV_DIR%\Scripts\pip.exe" install --upgrade pip
"%VENV_DIR%\Scripts\pip.exe" install -r "%~dp0requirements.txt"
if errorlevel 1 (
    echo ERROR: Failed to install dependencies.
    pause
    exit /b 1
)

echo.
echo Done! Virtual environment created in .venv\
echo Run the app with: run.bat
pause

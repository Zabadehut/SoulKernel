@echo off
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
call "C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat" >nul
cargo %*

@echo off
REM Exemple : lancer cargo avec l'environnement MSVC (Windows).
REM Copiez vers cargo-msvc.cmd (non versionné) et adaptez le chemin vcvars64.bat.
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
call "C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat" >nul
cargo %*

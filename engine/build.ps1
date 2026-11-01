# CMake writes warnings to stderr, so rely on exit codes instead of stopping on stderr.
$ErrorActionPreference = 'Continue'

$dev = 'E:\GaLa\dev'
$env:VCPKG_ROOT = "$dev\vcpkg"
$env:VCPKG_DEFAULT_BINARY_CACHE = "$dev\vcpkg-cache"
New-Item -ItemType Directory -Force $env:VCPKG_DEFAULT_BINARY_CACHE | Out-Null

cmake -S $PSScriptRoot -B "$dev\build\engine" -G "Visual Studio 17 2022" -A x64 `
  "-DCMAKE_TOOLCHAIN_FILE=$dev\vcpkg\scripts\buildsystems\vcpkg.cmake" `
  "-DVCPKG_TARGET_TRIPLET=x64-windows-static" `
  "-DVCPKG_INSTALLED_DIR=$dev\vcpkg-installed"
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

cmake --build "$dev\build\engine" --config Release
exit $LASTEXITCODE

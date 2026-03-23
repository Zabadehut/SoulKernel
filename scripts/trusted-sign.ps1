# Signature Windows via Azure Trusted Signing (trusted-signing-cli).
# Variables attendues : AZURE_CODE_SIGNING_ENDPOINT, AZURE_CLIENT_ID, AZURE_CLIENT_SECRET,
# AZURE_TENANT_ID, AZURE_TRUSTED_SIGNING_ACCOUNT_NAME, AZURE_CERTIFICATE_PROFILE_NAME
# (voir README — builds locaux sans compte : cargo tauri build --no-sign).

param(
    [Parameter(Mandatory = $true, Position = 0)]
    [string]$Path
)

$ErrorActionPreference = 'Stop'

if (-not $env:AZURE_CODE_SIGNING_ENDPOINT) {
    Write-Error 'AZURE_CODE_SIGNING_ENDPOINT manquant. Sans Azure Trusted Signing : cargo tauri build --no-sign'
}

if (-not (Get-Command trusted-signing-cli -ErrorAction SilentlyContinue)) {
    Write-Error 'trusted-signing-cli introuvable. Installer : cargo install trusted-signing-cli --version ''^0.9'' --locked'
}

& trusted-signing-cli `
    -e $env:AZURE_CODE_SIGNING_ENDPOINT `
    -d SoulKernel `
    $Path

exit $LASTEXITCODE

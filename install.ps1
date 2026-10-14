# pdf-encrypt installer for Windows (Windows PowerShell 5.1 or PowerShell 7+).
#
#   irm https://raw.githubusercontent.com/doggy8088/pdf-encrypt/main/install.ps1 | iex
#
# Options:
#   -Version <x.y.z>    or $env:PDF_ENCRYPT_VERSION      default: latest release
#   -InstallDir <dir>   or $env:PDF_ENCRYPT_INSTALL_DIR  default: %LOCALAPPDATA%\Programs\pdf-encrypt\bin
#   -NoModifyPath       or $env:PDF_ENCRYPT_NO_MODIFY_PATH = 1
#
# To pass options with irm, use:
#   & ([scriptblock]::Create((irm https://raw.githubusercontent.com/doggy8088/pdf-encrypt/main/install.ps1))) -Version 0.3.0
#
# The script downloads the raw executable from GitHub Releases, verifies its
# SHA-256 checksum, installs it, and adds the install directory to the user PATH.

param(
    [string]$Version = $env:PDF_ENCRYPT_VERSION,
    [string]$InstallDir = $env:PDF_ENCRYPT_INSTALL_DIR,
    [switch]$NoModifyPath = ($env:PDF_ENCRYPT_NO_MODIFY_PATH -eq '1')
)

function Install-PdfEncrypt {
    param([string]$Version, [string]$InstallDir, [bool]$NoModifyPath)

    $ErrorActionPreference = 'Stop'
    $ProgressPreference = 'SilentlyContinue'
    $repo = 'doggy8088/pdf-encrypt'

    if ($PSVersionTable.PSVersion.Major -ge 6 -and -not $IsWindows) {
        throw "This installer is for Windows. On macOS or Linux, run: curl -fsSL https://raw.githubusercontent.com/$repo/main/install.sh | sh"
    }

    # Windows PowerShell 5.1 does not enable TLS 1.2 by default.
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

    # PROCESSOR_ARCHITEW6432 holds the real architecture when running in a 32-bit or emulated process.
    $cpu = if ($env:PROCESSOR_ARCHITEW6432) { $env:PROCESSOR_ARCHITEW6432 } else { $env:PROCESSOR_ARCHITECTURE }
    $arch = switch ($cpu) {
        'AMD64' { 'x86_64' }
        'ARM64' { 'aarch64' }
        default { throw "Unsupported CPU architecture: $cpu" }
    }

    $asset = "pdf-encrypt-$arch-pc-windows-msvc.exe"
    $Version = "$Version".Trim() -replace '^v', ''
    if (-not $Version -or $Version -eq 'latest') {
        $baseUrl = "https://github.com/$repo/releases/latest/download"
        $label = 'latest'
    }
    else {
        $baseUrl = "https://github.com/$repo/releases/download/v$Version"
        $label = "v$Version"
    }

    if (-not $InstallDir) {
        $InstallDir = Join-Path $env:LOCALAPPDATA 'Programs\pdf-encrypt\bin'
    }

    $tmpDir = Join-Path ([IO.Path]::GetTempPath()) ("pdf-encrypt-" + [Guid]::NewGuid())
    New-Item -ItemType Directory -Path $tmpDir -Force | Out-Null

    try {
        Write-Host "pdf-encrypt-installer: downloading $asset ($label)"
        $exe = Join-Path $tmpDir $asset
        $sumFile = "$exe.sha256"
        Invoke-WebRequest -Uri "$baseUrl/$asset" -OutFile $exe -UseBasicParsing
        Invoke-WebRequest -Uri "$baseUrl/$asset.sha256" -OutFile $sumFile -UseBasicParsing

        $expected = ((Get-Content -Path $sumFile -Raw).Trim() -split '\s+')[0].ToLowerInvariant()
        $actual = (Get-FileHash -Path $exe -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($expected -ne $actual) {
            throw "Checksum mismatch for $asset (expected $expected, got $actual)"
        }
        Write-Host 'pdf-encrypt-installer: checksum verified'

        New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
        $dest = Join-Path $InstallDir 'pdf-encrypt.exe'
        try {
            Copy-Item -Path $exe -Destination $dest -Force
        }
        catch {
            throw "Cannot write $dest. Close any running pdf-encrypt process and try again. $_"
        }

        $installed = & $dest --version
        Write-Host "pdf-encrypt-installer: installed $installed to $dest"

        if (-not $NoModifyPath) {
            $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
            $entries = @($userPath -split ';' | Where-Object { $_ })
            if ($entries -notcontains $InstallDir) {
                [Environment]::SetEnvironmentVariable('Path', (@($entries) + $InstallDir) -join ';', 'User')
                Write-Host "pdf-encrypt-installer: added $InstallDir to your user PATH. Open a new terminal to use it."
            }
            if (($env:Path -split ';') -notcontains $InstallDir) {
                $env:Path = "$env:Path;$InstallDir"
            }
        }
    }
    finally {
        Remove-Item -Path $tmpDir -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Install-PdfEncrypt -Version $Version -InstallDir $InstallDir -NoModifyPath ([bool]$NoModifyPath)

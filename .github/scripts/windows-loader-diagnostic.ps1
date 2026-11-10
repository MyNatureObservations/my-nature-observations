# Run from the repository root on the same Windows image as the failing job.
$ErrorActionPreference = 'Stop'
$repo = (Get-Location).Path
$baseline = 'aa4a5b923d365ceab9623bff2d9358c36a8d2374'
$scratch = Join-Path $env:RUNNER_TEMP ('mno-loader-' + [guid]::NewGuid())
New-Item -ItemType Directory $scratch | Out-Null
$env:CARGO_TARGET_DIR = Join-Path $repo 'app/src-tauri/target'
$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$vs = & $vswhere -latest -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
$tools = (Get-Content "$vs\VC\Auxiliary\Build\Microsoft.VCToolsVersion.default.txt").Trim()
$dumpbin = "$vs\VC\Tools\MSVC\$tools\bin\Hostx64\x64\dumpbin.exe"

Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class MnoLoader {
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true, ExactSpelling=true)]
    public static extern IntPtr LoadLibraryW(string name);
    [DllImport("kernel32.dll", SetLastError=true, ExactSpelling=true)]
    public static extern IntPtr GetProcAddress(IntPtr module, IntPtr name);
    [DllImport("kernel32.dll", ExactSpelling=true)]
    public static extern bool FreeLibrary(IntPtr module);
    [DllImport("kernel32.dll", ExactSpelling=true)]
    public static extern uint SetErrorMode(uint mode);
}
'@
# Suppress inherited critical-error dialogs on the unattended runner.
[void][MnoLoader]::SetErrorMode(0x8003)

function Build-TestExecutable($manifest) {
    $messages = @(cargo test --manifest-path $manifest --locked --lib --no-run --message-format=json)
    if ($LASTEXITCODE -ne 0) { throw "Library test build failed for $manifest" }
    $paths = @($messages | ForEach-Object {
        $message = $_ | ConvertFrom-Json
        if ($message.reason -eq 'compiler-artifact' -and $message.profile.test -and $message.executable) {
            $message.executable
        }
    })
    if ($paths.Count -ne 1) { throw "Expected one library test executable for $manifest" }
    return $paths[0]
}

function Read-Imports($executable, $label) {
    $lines = @(& $dumpbin /nologo /imports $executable)
    if ($LASTEXITCODE -ne 0) { throw "dumpbin failed for $executable" }
    $lines | Set-Content (Join-Path $scratch "$label-imports.txt")
    $dll = $null
    foreach ($line in $lines) {
        if ($line -match '^\s{4}(\S+\.dll)\s*$') {
            $dll = $Matches[1]
        } elseif ($line -match '^\s*(?:Summary|Section contains)') {
            $dll = $null
        } elseif ($dll -and $line -match '^\s+[0-9A-Fa-f]+\s+([A-Za-z_?@][^\s]*)\s*$') {
            [pscustomobject]@{ Dll = $dll; Symbol = $Matches[1]; Ordinal = $null }
        } elseif ($dll -and $line -match '^\s+(?:[0-9A-Fa-f]+\s+)?Ordinal\s+(\d+)\s*$') {
            [pscustomobject]@{ Dll = $dll; Symbol = "#$($Matches[1])"; Ordinal = [int]$Matches[1] }
        }
    }
}

function Test-Exports($imports) {
    foreach ($group in ($imports | Group-Object Dll)) {
        $module = [MnoLoader]::LoadLibraryW($group.Name)
        if ($module -eq [IntPtr]::Zero) {
            $errorCode = [Runtime.InteropServices.Marshal]::GetLastWin32Error()
            Write-Host "DLL load failed: $($group.Name), Win32 error $errorCode"
            continue
        }
        try {
            foreach ($import in $group.Group) {
                $named = $null -eq $import.Ordinal
                if ($named) {
                    $pointer = [Runtime.InteropServices.Marshal]::StringToHGlobalAnsi($import.Symbol)
                } else {
                    $pointer = [IntPtr]::new($import.Ordinal)
                }
                try {
                    if ([MnoLoader]::GetProcAddress($module, $pointer) -eq [IntPtr]::Zero) {
                        Write-Host "Missing export: $($import.Dll)!$($import.Symbol)"
                    }
                } finally {
                    if ($named) { [Runtime.InteropServices.Marshal]::FreeHGlobal($pointer) }
                }
            }
        } finally {
            [void][MnoLoader]::FreeLibrary($module)
        }
    }
}

$current = Build-TestExecutable 'app/src-tauri/Cargo.toml'
# Copy before the baseline build reuses the target directory.
$currentCopy = Join-Path (Split-Path $current) 'mno-c73b-loader-probe.exe'
Copy-Item $current $currentCopy -Force
$currentImports = @(Read-Imports $currentCopy 'current')
if (!$currentImports.Count) { throw 'No imports parsed from current executable' }
Write-Host 'Current import export checks'
Test-Exports $currentImports
& $currentCopy --list
$currentExit = $LASTEXITCODE
Write-Host ('C7.3b --list exit: 0x{0:X8}' -f $currentExit)

$archive = Join-Path $scratch 'baseline.tar'
git archive --output=$archive $baseline
if ($LASTEXITCODE -ne 0) { throw 'Cannot archive C7.3a' }
tar -xf $archive -C $scratch
if ($LASTEXITCODE -ne 0) { throw 'Cannot extract C7.3a' }
$previous = Build-TestExecutable (Join-Path $scratch 'app/src-tauri/Cargo.toml')
$previousImports = @(Read-Imports $previous 'baseline')
if (!$previousImports.Count) { throw 'No imports parsed from baseline executable' }
& $previous --list startup::windows_tests
Write-Host ('C7.3a --list exit: 0x{0:X8}' -f $LASTEXITCODE)
Write-Host 'Import differences, <= C7.3a and => C7.3b'
Compare-Object @($previousImports | ForEach-Object { "$($_.Dll)!$($_.Symbol)" } | Sort-Object -Unique) `
    @($currentImports | ForEach-Object { "$($_.Dll)!$($_.Symbol)" } | Sort-Object -Unique) |
    Format-Table -AutoSize | Out-String | Write-Host

# Loader snaps cover failures below the executable's direct import table.
$debuggers = "${env:ProgramFiles(x86)}\Windows Kits\10\Debuggers\x64"
$cdb = Join-Path $debuggers 'cdb.exe'
$gflags = Join-Path $debuggers 'gflags.exe'
if ($currentExit -ne 0 -and (Test-Path $cdb) -and (Test-Path $gflags)) {
    $image = Split-Path -Leaf $currentCopy
    & $gflags /i $image +sls
    if ($LASTEXITCODE -ne 0) { throw 'Could not enable loader snaps' }
    try {
        & $cdb -g -G $currentCopy --list
    } finally {
        & $gflags /i $image -sls
    }
}
if ($currentExit -ne 0) { throw 'C7.3b test executable could not list tests' }

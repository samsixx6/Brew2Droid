[CmdletBinding()]
param([Parameter(Mandatory=$true)][string]$Serial)

$ErrorActionPreference = 'Stop'
$brewAdb = Join-Path $PSScriptRoot '../.tools/android-sdk/platform-tools/adb.exe'
$brewRoot = '/data/local/tmp/brew-handset-probe'
$brewRuns = "$brewRoot/runs"
$brewStamp = [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ')
$brewBackup = "$brewRoot/runs-preserved-$brewStamp"
$brewAlias = "$brewRoot/fixtures/path-regression-$brewStamp"
$brewWork = "$brewRuns/test"

function Invoke-CheckedAdb {
    param([string[]]$CommandArgs)
    $brewResult = & $brewAdb -s $Serial @CommandArgs 2>&1
    if ($LASTEXITCODE -ne 0) { throw "ADB failed: $($brewResult -join ' ')" }
    return $brewResult
}

# A regressão mexe apenas na raiz temporária desta bancada, com nenhum probe em execução.
$brewProcesses = & $brewAdb -s $Serial shell pidof brew-probe 2>&1
if ($LASTEXITCODE -eq 0 -and ($brewProcesses -join '').Trim()) {
    throw 'Wait for active brew-probe processes before testing path isolation.'
}
$brewActualRoot = (Invoke-CheckedAdb @('shell','readlink','-f',$brewRoot) | Out-String).Trim()
$brewActualRuns = (Invoke-CheckedAdb @('shell','readlink','-f',$brewRuns) | Out-String).Trim()
if ($brewActualRoot -ne $brewRoot -or $brewActualRuns -ne $brewRuns) {
    throw 'The initial Android probe root/runs must be real, unredirected directories.'
}
& $brewAdb -s $Serial shell test -e $brewBackup
if ($LASTEXITCODE -eq 0) { throw 'Preservation directory already exists.' }

$brewMoved = $false
$brewLinked = $false
try {
    Invoke-CheckedAdb @('shell','mkdir','-p',"$brewAlias/test") | Out-Null
    Invoke-CheckedAdb @('shell','mv',$brewRuns,$brewBackup) | Out-Null
    $brewMoved = $true
    Invoke-CheckedAdb @('shell','ln','-s',$brewAlias,$brewRuns) | Out-Null
    $brewLinked = $true

    $brewOutput = & $brewAdb -s $Serial shell "LD_LIBRARY_PATH=$brewRoot/bin $brewRoot/bin/brew-probe synthetic --work-dir $brewWork --output $brewWork/report.json" 2>&1
    $brewExit = $LASTEXITCODE
    if ($brewExit -eq 0) { throw 'Probe accepted redirected runs and wrote inside fixture storage.' }
    if (($brewOutput -join ' ') -notmatch 'run root must not redirect') {
        throw "Probe failed for another reason: $($brewOutput -join ' ')"
    }
    $brewChildren = Invoke-CheckedAdb @('shell','ls','-A',"$brewAlias/test")
    if (($brewChildren -join '').Trim()) { throw 'Redirected source tree was modified before rejection.' }
    Write-Output 'PASS: redirected runs rejected before writing captures, content, storage or reports.'
} finally {
    if ($brewLinked) {
        $brewTarget = (Invoke-CheckedAdb @('shell','readlink',$brewRuns) | Out-String).Trim()
        if ($brewTarget -ne $brewAlias) { throw 'Probe runs link changed; preservation directory was left intact.' }
        # rm sem -r só remove o link criado acima; nunca apaga a árvore de fontes.
        Invoke-CheckedAdb @('shell','rm',$brewRuns) | Out-Null
    }
    if ($brewMoved) {
        Invoke-CheckedAdb @('shell','mv',$brewBackup,$brewRuns) | Out-Null
        $brewRestored = (Invoke-CheckedAdb @('shell','readlink','-f',$brewRuns) | Out-String).Trim()
        if ($brewRestored -ne $brewRuns) { throw 'Original runs preservation requires attention.' }
        Write-Output 'Original native run tree restored.'
    }
}

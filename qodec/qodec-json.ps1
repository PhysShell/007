# qodec-json — pull a uniform record array out of a big JSON file and toon it.
# toon = keys-once table: strips repeated object keys, semantic (value-equal)
# roundtrip. The right tool when the file is one array of same-shaped records
# (findings.json, report.json, ...) — the miner is single-threaded and slow on
# multi-MB input, toon does it in seconds. The wall time is almost all the BPE
# token count for --report; the transform itself is milliseconds.
#
#   pwsh qodec-json.ps1 -File <big.json> [-Key findings] [-Codec toon]
#   $env:QODEC overrides the binary path.
param(
  [Parameter(Mandatory)][string]$File,
  [string]$Key   = 'findings',
  [string]$Codec = 'toon'
)
$ErrorActionPreference = 'Stop'
$q = if ($env:QODEC) { $env:QODEC } else { Join-Path $PSScriptRoot 'target\release\qodec.exe' }
if (-not (Test-Path $q)) { throw "no qodec binary at $q (run: cargo build --release), or set `$env:QODEC" }

# extract .<Key>[] as its raw JSON span — no re-serialize, byte-exact
$doc = [System.Text.Json.JsonDocument]::Parse([System.IO.File]::ReadAllText($File))
$el  = $doc.RootElement.GetProperty($Key)
"records: $($el.GetArrayLength())   array bytes: {0:N0}" -f $el.GetRawText().Length

$tmp = [System.IO.Path]::GetTempFileName()
try {
  [System.IO.File]::WriteAllText($tmp, $el.GetRawText())
  & $q encode -i $tmp --codec $Codec --report 1>$null   # report -> stderr (prints)
} finally {
  Remove-Item $tmp -ErrorAction SilentlyContinue
}

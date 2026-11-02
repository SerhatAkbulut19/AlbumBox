<#
.SYNOPSIS
  Yüz tanıma için gereken dosyaları indirir ve AlbumBox'ın models klasörüne koyar.

.DESCRIPTION
  Dosyalar bir kez indirilir; sonrasında yüz tanıma tamamen çevrimdışı çalışır.
  - InsightFace modelleri (ticari olmayan kullanım içindir; aile arşivi uygundur):
      buffalo_l (varsayılan, ~290 MB indirme): det_10g.onnx + w600k_r50.onnx, daha isabetli
      buffalo_s (~130 MB indirme): det_500m.onnx + w600k_mbf.onnx, daha hızlı
  - ONNX Runtime (Microsoft, MIT lisans): onnxruntime.dll

  İnternet olmayan makinede: başka bir bilgisayarda çalıştırıp "models" klasörünü
  mini PC'de AlbumBox klasörünün içine kopyalayın.

.EXAMPLE
  .\scripts\yuz-modeli-indir.ps1                          # .\models  (geliştirme)
  .\scripts\yuz-modeli-indir.ps1 -Target C:\AlbumBox\models
  .\scripts\yuz-modeli-indir.ps1 -Pack buffalo_s
#>
param(
    [string]$Target = (Join-Path (Split-Path $PSScriptRoot -Parent) 'models'),
    [ValidateSet('buffalo_l', 'buffalo_s')]
    [string]$Pack = 'buffalo_l',
    [string]$OrtVersion = '1.31.1'
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue' # Invoke-WebRequest is very slow with the progress bar
Add-Type -AssemblyName System.IO.Compression.FileSystem
New-Item -ItemType Directory -Force -Path $Target | Out-Null

function Get-FromZip([string]$Url, [string[]]$Names) {
    $missing = $Names | Where-Object { -not (Test-Path (Join-Path $Target $_)) }
    if (-not $missing) { return }
    $zip = Join-Path $env:TEMP ("albumbox-" + [IO.Path]::GetFileName($Url))
    Write-Host "İndiriliyor: $Url"
    Invoke-WebRequest -Uri $Url -OutFile $zip -UseBasicParsing
    $archive = [IO.Compression.ZipFile]::OpenRead($zip)
    try {
        foreach ($name in $missing) {
            $entry = $archive.Entries | Where-Object { $_.Name -eq $name } | Select-Object -First 1
            if (-not $entry) { throw "$name arşivde bulunamadı" }
            [IO.Compression.ZipFileExtensions]::ExtractToFile($entry, (Join-Path $Target $name), $true)
            Write-Host "  $name"
        }
    } finally {
        $archive.Dispose()
        Remove-Item $zip -Force
    }
}

$models = @{ buffalo_l = @('det_10g.onnx', 'w600k_r50.onnx'); buffalo_s = @('det_500m.onnx', 'w600k_mbf.onnx') }[$Pack]
Get-FromZip "https://github.com/deepinsight/insightface/releases/download/v0.7/$Pack.zip" $models
Get-FromZip "https://github.com/microsoft/onnxruntime/releases/download/v$OrtVersion/onnxruntime-win-x64-$OrtVersion.zip" @('onnxruntime.dll')
Write-Host "Yüz tanıma dosyaları hazır: $Target"

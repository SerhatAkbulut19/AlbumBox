<#
.SYNOPSIS
  Harici disk yokken test için örnek bir "aile diski" üretir ve bir sürücü harfine bağlar.

.DESCRIPTION
  ffmpeg ile Türkçe klasör adları, tarihli dosya adları, kopya dosyalar ve kısa
  videolar içeren bir klasör oluşturur, ardından `subst` ile sürücü harfine bağlar.
  Disk harfi değişimini denemek için:
    subst X: /d
    subst Y: <klasör>
  AlbumBox birkaç saniye içinde kütüphaneyi Y:\Aile olarak bulur.

.EXAMPLE
  .\scripts\ornek-disk.ps1                  # %TEMP%\albumbox-ornek-disk -> X:
  .\scripts\ornek-disk.ps1 -Drive Y -Path D:\deneme
  .\scripts\ornek-disk.ps1 -Remove          # sürücü bağlantısını kaldırır
#>
param(
    [string]$Path = (Join-Path $env:TEMP 'albumbox-ornek-disk'),
    [string]$Drive = 'X',
    [switch]$Remove
)

$ErrorActionPreference = 'Stop'

if ($Remove) {
    subst "$($Drive):" /d
    Write-Host "$($Drive): bağlantısı kaldırıldı."
    return
}

if (-not (Get-Command ffmpeg -ErrorAction SilentlyContinue)) {
    throw 'ffmpeg bulunamadı (PATH). winget install Gyan.FFmpeg ile kurabilirsiniz.'
}

$root = Join-Path $Path 'Aile'
$folders = 'Köy 2022', 'Köy 2022\Nişan', 'Ramazan_Tatili_2023', 'Düğün Ayşe', 'Telefon Yedek', '$RECYCLE.BIN'
foreach ($f in $folders) { New-Item -ItemType Directory -Force -Path (Join-Path $root $f) | Out-Null }

function New-Photo([string]$Source, [string]$Rel) {
    & ffmpeg -hide_banner -loglevel error -y -f lavfi -i $Source -frames:v 1 -q:v 3 (Join-Path $root $Rel)
}

$i = 0
foreach ($src in 'mandelbrot=size=1600x1200', 'testsrc2=size=1600x1200', 'smptehdbars=size=1600x900',
                 'cellauto=size=1200x1600:rule=110', 'life=size=1600x1200:mold=10:ratio=0.3') {
    $i++
    New-Photo $src "Köy 2022\IMG_2022071$($i)_14301$($i).jpg"
}
foreach ($n in 1..3) { New-Photo "gradients=size=1600x1200:seed=$n" "Köy 2022\Nişan\IMG_2022081$($n)_19000$($n).jpg" }
foreach ($n in 4..7) { New-Photo "gradients=size=1200x1600:seed=$n" "Ramazan_Tatili_2023\IMG-2023042$n-WA00$n.jpg" }
foreach ($n in 8..9) { New-Photo "mandelbrot=size=1600x1200:start_scale=0.$n" "Düğün Ayşe\DSC_04$n.JPG" }

# Aynı içerik farklı klasörde: AlbumBox bunları tek medya olarak sayar.
Copy-Item (Join-Path $root 'Köy 2022\IMG_20220711_143011.jpg') (Join-Path $root 'Telefon Yedek\kopya1.jpg') -Force
Copy-Item (Join-Path $root 'Ramazan_Tatili_2023\IMG-20230424-WA004.jpg') (Join-Path $root 'Telefon Yedek\kopya2.jpg') -Force
# Taranmaması gereken dosyalar.
New-Photo 'testsrc=size=800x600' '$RECYCLE.BIN\silinmis.jpg'
Set-Content -Path (Join-Path $root 'notlar.txt') -Value 'notlar' -Encoding utf8

& ffmpeg -hide_banner -loglevel error -y -f lavfi -i 'testsrc2=duration=4:size=1280x720:rate=25' `
    -f lavfi -i 'sine=frequency=440:duration=4' -metadata creation_time=2023-04-22T12:15:00Z `
    -c:v libx264 -pix_fmt yuv420p -c:a aac -shortest (Join-Path $root 'Ramazan_Tatili_2023\bayram_sabahi.mp4')
& ffmpeg -hide_banner -loglevel error -y -f lavfi -i 'mandelbrot=size=720x1280:rate=25' -t 3 `
    -metadata creation_time=2022-08-12T17:40:00Z -c:v libx264 -pix_fmt yuv420p `
    (Join-Path $root 'Köy 2022\Nişan\VID_20220812_204000.mp4')

subst "$($Drive):" /d 2>$null | Out-Null
subst "$($Drive):" (Resolve-Path $Path).Path
Write-Host "Örnek disk hazır: $($Drive):\Aile  (18 medya dosyası, 16 benzersiz içerik)"
Write-Host "AlbumBox > Admin Panel > Kütüphaneler > Kütüphane ekle > $($Drive):\Aile"

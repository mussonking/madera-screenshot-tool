# Regenerates the full Madera.SS icon family from the original design:
# opaque accent-red (#E94560) background + white camera glyph filling the canvas.
# Run from anywhere: powershell -File regenerate-icons.ps1
Add-Type -AssemblyName System.Drawing

$dir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RED = [System.Drawing.Color]::FromArgb(255, 233, 69, 96)
$WHITE = [System.Drawing.Color]::FromArgb(255, 255, 255, 255)

function New-RoundedRectPath($x, $y, $w, $h, $r) {
    $p = New-Object System.Drawing.Drawing2D.GraphicsPath
    $p.AddArc($x, $y, $r * 2, $r * 2, 180, 90)
    $p.AddArc($x + $w - $r * 2, $y, $r * 2, $r * 2, 270, 90)
    $p.AddArc($x + $w - $r * 2, $y + $h - $r * 2, $r * 2, $r * 2, 0, 90)
    $p.AddArc($x, $y + $h - $r * 2, $r * 2, $r * 2, 90, 90)
    $p.CloseFigure()
    return $p
}

# Draws the camera glyph from the original 256x256 design, scaled to canvas S.
# glyphScale < 1 shrinks the glyph around the center (for adaptive foregrounds).
function Draw-Camera($g, $S, $glyphScale) {
    $k = ($S / 256.0) * $glyphScale
    $ox = ($S - 256 * $k) / 2.0
    $oy = ($S - 256 * $k) / 2.0

    $whiteBrush = New-Object System.Drawing.SolidBrush($WHITE)
    $redBrush = New-Object System.Drawing.SolidBrush($RED)

    # Camera body
    $g.FillPath($whiteBrush, (New-RoundedRectPath ($ox + 40 * $k) ($oy + 70 * $k) (176 * $k) (130 * $k) (20 * $k)))
    # Flash bump
    $g.FillPath($whiteBrush, (New-RoundedRectPath ($ox + 170 * $k) ($oy + 50 * $k) (30 * $k) (25 * $k) (5 * $k)))
    # Lens: outer red, inner white, center red dot
    $cx = $ox + 128 * $k
    $cy = $oy + 135 * $k
    $g.FillEllipse($redBrush, $cx - 50 * $k, $cy - 50 * $k, 100 * $k, 100 * $k)
    $g.FillEllipse($whiteBrush, $cx - 35 * $k, $cy - 35 * $k, 70 * $k, 70 * $k)
    $g.FillEllipse($redBrush, $cx - 15 * $k, $cy - 15 * $k, 30 * $k, 30 * $k)

    $whiteBrush.Dispose()
    $redBrush.Dispose()
}

# Master bitmap: red full-square background + camera (or transparent if $background=$false)
function New-Master($S, $background, $glyphScale) {
    $bmp = New-Object System.Drawing.Bitmap($S, $S)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    if ($background) { $g.Clear($RED) } else { $g.Clear([System.Drawing.Color]::Transparent) }
    Draw-Camera $g $S $glyphScale
    $g.Dispose()
    return $bmp
}

function Save-Resized($source, $path, $S) {
    $out = New-Object System.Drawing.Bitmap($S, $S)
    $g = [System.Drawing.Graphics]::FromImage($out)
    $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    $g.CompositingQuality = [System.Drawing.Drawing2D.CompositingQuality]::HighQuality
    $g.DrawImage($source, 0, 0, $S, $S)
    $g.Dispose()
    $out.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
    $out.Dispose()
    Write-Output ("  " + (Split-Path $path -Leaf) + " ($S" + "x$S)")
}

# Returns a MemoryStream containing the PNG (objects survive PowerShell return; byte[] does not)
function New-PngStream($source, $S) {
    $tmp = New-Object System.Drawing.Bitmap($S, $S)
    $g = [System.Drawing.Graphics]::FromImage($tmp)
    $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    $g.CompositingQuality = [System.Drawing.Drawing2D.CompositingQuality]::HighQuality
    $g.DrawImage($source, 0, 0, $S, $S)
    $g.Dispose()
    $ms = New-Object System.IO.MemoryStream
    $tmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
    $tmp.Dispose()
    return $ms
}

function Write-BE32($stream, $value) {
    $stream.WriteByte([byte](($value -shr 24) -band 0xFF))
    $stream.WriteByte([byte](($value -shr 16) -band 0xFF))
    $stream.WriteByte([byte](($value -shr 8) -band 0xFF))
    $stream.WriteByte([byte]($value -band 0xFF))
}

function Write-LE16($stream, $value) {
    $stream.WriteByte([byte]($value -band 0xFF))
    $stream.WriteByte([byte](($value -shr 8) -band 0xFF))
}

function Write-LE32($stream, $value) {
    Write-LE16 $stream ($value -band 0xFFFF)
    Write-LE16 $stream (($value -shr 16) -band 0xFFFF)
}

# ---- Master sources (drawn once at high resolution) ----
Write-Output "Drawing masters..."
$master = New-Master 1024 $true 1.0          # full icon: red bg + glyph
$foreground = New-Master 1024 $false 0.62    # Android adaptive foreground: glyph only

# ---- Desktop PNGs ----
Write-Output "Desktop PNGs:"
Save-Resized $master (Join-Path $dir "icon.png") 512
Save-Resized $master (Join-Path $dir "128x128@2x.png") 256
Save-Resized $master (Join-Path $dir "128x128.png") 128
Save-Resized $master (Join-Path $dir "32x32.png") 32

# ---- Windows Store tiles ----
Write-Output "Windows tiles:"
foreach ($s in @(30, 44, 71, 89, 107, 142, 150, 284, 310)) {
    Save-Resized $master (Join-Path $dir "Square$($s)x$($s)Logo.png") $s
}
Save-Resized $master (Join-Path $dir "StoreLogo.png") 50

# ---- Windows ICO (PNG-embedded, Vista+ style) ----
Write-Output "icon.ico:"
$icoSizes = @(16, 24, 32, 48, 64, 256)
$pngStreams = @()
foreach ($s in $icoSizes) { $pngStreams += (New-PngStream $master $s) }
$out = New-Object System.IO.MemoryStream
Write-LE16 $out 0
Write-LE16 $out 1
Write-LE16 $out $icoSizes.Count
$offset = 6 + 16 * $icoSizes.Count
for ($i = 0; $i -lt $icoSizes.Count; $i++) {
    $s = $icoSizes[$i]
    $dim = [byte]$(if ($s -ge 256) { 0 } else { $s })
    $out.WriteByte($dim)
    $out.WriteByte($dim)
    $out.WriteByte(0)
    $out.WriteByte(0)
    Write-LE16 $out 1
    Write-LE16 $out 32
    Write-LE32 $out $pngStreams[$i].Length
    Write-LE32 $out $offset
    $offset += $pngStreams[$i].Length
}
foreach ($m in $pngStreams) { $m.WriteTo($out) }
[System.IO.File]::WriteAllBytes((Join-Path $dir "icon.ico"), $out.ToArray())
foreach ($m in $pngStreams) { $m.Dispose() }
$out.Dispose()
Write-Output ("  icon.ico (" + ($icoSizes -join ", ") + " px)")

# ---- macOS ICNS (PNG-based entries, big-endian container) ----
Write-Output "icon.icns:"
$icnsEntries = @(
    @("ic11", 32),   # 16@2x
    @("ic12", 64),   # 32@2x
    @("ic07", 128),  # 128
    @("ic13", 256),  # 128@2x
    @("ic08", 256),  # 256
    @("ic14", 512),  # 256@2x
    @("ic09", 512)   # 512
)
$chunkStreams = @()
foreach ($e in $icnsEntries) { $chunkStreams += (New-PngStream $master $e[1]) }
$total = 8
foreach ($m in $chunkStreams) { $total += 8 + $m.Length }
$out = New-Object System.IO.MemoryStream
$out.Write([System.Text.Encoding]::ASCII.GetBytes("icns"), 0, 4)
Write-BE32 $out $total
for ($i = 0; $i -lt $icnsEntries.Count; $i++) {
    $type = [System.Text.Encoding]::ASCII.GetBytes($icnsEntries[$i][0])
    $out.Write($type, 0, 4)
    Write-BE32 $out (8 + $chunkStreams[$i].Length)
    $chunkStreams[$i].WriteTo($out)
}
[System.IO.File]::WriteAllBytes((Join-Path $dir "icon.icns"), $out.ToArray())
foreach ($m in $chunkStreams) { $m.Dispose() }
$out.Dispose()
Write-Output "  icon.icns (ic07-ic14 PNG entries)"

# ---- iOS ----
Write-Output "iOS:"
$ios = Join-Path $dir "ios"
$iosMap = @{
    "AppIcon-20x20@1x.png"    = 20
    "AppIcon-20x20@2x.png"    = 40
    "AppIcon-20x20@2x-1.png"  = 40
    "AppIcon-20x20@3x.png"    = 60
    "AppIcon-29x29@1x.png"    = 29
    "AppIcon-29x29@2x.png"    = 58
    "AppIcon-29x29@2x-1.png"  = 58
    "AppIcon-29x29@3x.png"    = 87
    "AppIcon-40x40@1x.png"    = 40
    "AppIcon-40x40@2x.png"    = 80
    "AppIcon-40x40@2x-1.png"  = 80
    "AppIcon-60x60@2x.png"    = 120
    "AppIcon-60x60@3x.png"    = 180
    "AppIcon-76x76@1x.png"    = 76
    "AppIcon-76x76@2x.png"    = 152
    "AppIcon-83.5x83.5@2x.png" = 167
    "AppIcon-512@2x.png"      = 1024
}
foreach ($name in $iosMap.Keys) {
    Save-Resized $master (Join-Path $ios $name) $iosMap[$name]
}

# ---- Android ----
Write-Output "Android:"
$androidSizes = @{ "mdpi" = 48; "hdpi" = 72; "xhdpi" = 96; "xxhdpi" = 144; "xxxhdpi" = 192 }
$foregroundSizes = @{ "mdpi" = 108; "hdpi" = 162; "xhdpi" = 216; "xxhdpi" = 324; "xxxhdpi" = 432 }
foreach ($density in $androidSizes.Keys) {
    $folder = Join-Path $dir "android\mipmap-$density"
    $s = $androidSizes[$density]
    Save-Resized $master (Join-Path $folder "ic_launcher.png") $s
    Save-Resized $master (Join-Path $folder "ic_launcher_round.png") $s
    Save-Resized $foreground (Join-Path $folder "ic_launcher_foreground.png") $foregroundSizes[$density]
}

$master.Dispose()
$foreground.Dispose()
Write-Output "Done."

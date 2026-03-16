Add-Type -AssemblyName System.Drawing

# Create a 32x32 bitmap
$bmp = New-Object System.Drawing.Bitmap(32, 32)
$g = [System.Drawing.Graphics]::FromImage($bmp)

# Fill with a nice red/pink color (screenshot tool accent color)
$accentColor = [System.Drawing.Color]::FromArgb(233, 69, 96)
$brush = New-Object System.Drawing.SolidBrush($accentColor)
$g.FillRectangle($brush, 0, 0, 32, 32)

# Draw a white camera/screenshot icon shape
$whiteBrush = [System.Drawing.Brushes]::White
# Camera body
$g.FillRectangle($whiteBrush, 6, 10, 20, 14)
# Camera lens
$g.FillEllipse($whiteBrush, 11, 12, 10, 10)
# Fill lens center with accent color
$g.FillEllipse($brush, 14, 15, 4, 4)
# Camera flash
$g.FillRectangle($whiteBrush, 20, 7, 4, 3)

$g.Dispose()

# Convert to icon and save
$icon = [System.Drawing.Icon]::FromHandle($bmp.GetHicon())
$fs = [System.IO.File]::Create("$PSScriptRoot\icon.ico")
$icon.Save($fs)
$fs.Close()
$icon.Dispose()
$bmp.Dispose()

Write-Host "Icon created successfully!"

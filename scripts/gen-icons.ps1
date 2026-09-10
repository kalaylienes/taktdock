# Generates the application icons and the two tray states.
# Run from the repository root: powershell -File scripts/gen-icons.ps1
#
# The mark is a metronome reduced to its pendulum: a rod leaning off vertical
# from a pivot, with the sliding weight on it. It reads at 16 pixels, where a
# drawn metronome body turns into a smudge.
Add-Type -AssemblyName System.Drawing

$root = Split-Path -Parent $PSScriptRoot
$icons = Join-Path $root "src-tauri\icons"
if (-not (Test-Path $icons)) { New-Item -ItemType Directory -Path $icons | Out-Null }

function New-RoundedPath([single]$x, [single]$y, [single]$w, [single]$h, [single]$r) {
    $p = New-Object System.Drawing.Drawing2D.GraphicsPath
    $d = $r * 2
    $p.AddArc($x, $y, $d, $d, 180, 90)
    $p.AddArc($x + $w - $d, $y, $d, $d, 270, 90)
    $p.AddArc($x + $w - $d, $y + $h - $d, $d, $d, 0, 90)
    $p.AddArc($x, $y + $h - $d, $d, $d, 90, 90)
    $p.CloseFigure()
    return $p
}

# Draws the pendulum inside a square of side $s at offset ($ox, $oy).
function Draw-Pendulum($g, [single]$ox, [single]$oy, [single]$s, $rodBrush, [System.Drawing.Color]$weight, [single]$thick) {
    $pivotX = $ox + $s * 0.50
    $pivotY = $oy + $s * 0.86
    $topX = $ox + $s * 0.64
    $topY = $oy + $s * 0.14

    $pen = New-Object System.Drawing.Pen($rodBrush, $thick)
    $pen.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
    $pen.EndCap = [System.Drawing.Drawing2D.LineCap]::Round
    $g.DrawLine($pen, $pivotX, $pivotY, $topX, $topY)
    $pen.Dispose()

    # The weight sits a little above the middle of the rod.
    $t = 0.60
    $wx = $pivotX + ($topX - $pivotX) * $t
    $wy = $pivotY + ($topY - $pivotY) * $t
    $r = $s * 0.13
    $b = New-Object System.Drawing.SolidBrush($weight)
    $g.FillEllipse($b, $wx - $r, $wy - $r, 2 * $r, 2 * $r)
    $b.Dispose()
}

function New-AppIcon([int]$size) {
    $bmp = New-Object System.Drawing.Bitmap($size, $size, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $g.Clear([System.Drawing.Color]::Transparent)

    $s = [single]$size
    $bg = New-RoundedPath 0 0 $s $s ($s * 0.22)
    $bgBrush = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(255, 27, 27, 27))
    $g.FillPath($bgBrush, $bg)

    # The base the pivot stands on, in the track colour of the widget.
    $baseH = $s * 0.07
    $base = New-RoundedPath ($s * 0.30) ($s * 0.80) ($s * 0.40) $baseH ($baseH / 2)
    $baseBrush = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(48, 255, 255, 255))
    $g.FillPath($baseBrush, $base)
    $baseBrush.Dispose()
    $base.Dispose()

    $inner = $s * 0.78
    $off = ($s - $inner) / 2
    $rect = New-Object System.Drawing.RectangleF($off, $off, $inner, $inner)
    $from = [System.Drawing.Color]::FromArgb(255, 47, 158, 143)
    $to = [System.Drawing.Color]::FromArgb(255, 63, 191, 174)
    $rod = New-Object System.Drawing.Drawing2D.LinearGradientBrush($rect, $to, $from, 90.0)
    $core = [System.Drawing.Color]::FromArgb(255, 166, 236, 225)
    Draw-Pendulum $g $off ($off - $s * 0.02) $inner $rod $core ([Math]::Max(1.5, $s * 0.075))
    $rod.Dispose()

    $g.Dispose()
    $bgBrush.Dispose()
    $bg.Dispose()
    return $bmp
}

function New-TrayIcon([int]$size, [System.Drawing.Color]$color) {
    $bmp = New-Object System.Drawing.Bitmap($size, $size, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $g.Clear([System.Drawing.Color]::Transparent)

    $s = [single]$size
    $rod = New-Object System.Drawing.SolidBrush($color)
    Draw-Pendulum $g 0 0 $s $rod $color ($s * 0.12)
    $rod.Dispose()

    # A short base line, so the mark still reads as standing on something.
    $pen = New-Object System.Drawing.Pen($color, [single]($s * 0.10))
    $pen.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
    $pen.EndCap = [System.Drawing.Drawing2D.LineCap]::Round
    $g.DrawLine($pen, $s * 0.30, $s * 0.90, $s * 0.70, $s * 0.90)
    $pen.Dispose()

    $g.Dispose()
    return $bmp
}

function Save-Png([System.Drawing.Bitmap]$bmp, [string]$path) {
    $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
}

# Vista and later accept PNG payloads inside an ICO container, so the header is
# written by hand rather than going through Icon.FromHandle.
function Save-Ico([int[]]$sizes, [string]$path) {
    $streams = @()
    foreach ($sz in $sizes) {
        $b = New-AppIcon $sz
        $ms = New-Object System.IO.MemoryStream
        $b.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
        $streams += , @{ size = $sz; bytes = $ms.ToArray() }
        $ms.Dispose()
        $b.Dispose()
    }

    $fs = [System.IO.File]::Create($path)
    $bw = New-Object System.IO.BinaryWriter($fs)
    $bw.Write([UInt16]0)
    $bw.Write([UInt16]1)
    $bw.Write([UInt16]$streams.Count)

    $offset = 6 + 16 * $streams.Count
    foreach ($s in $streams) {
        $dim = if ($s.size -ge 256) { 0 } else { $s.size }
        $bw.Write([Byte]$dim)
        $bw.Write([Byte]$dim)
        $bw.Write([Byte]0)
        $bw.Write([Byte]0)
        $bw.Write([UInt16]1)
        $bw.Write([UInt16]32)
        $bw.Write([UInt32]$s.bytes.Length)
        $bw.Write([UInt32]$offset)
        $offset += $s.bytes.Length
    }
    foreach ($s in $streams) { $bw.Write($s.bytes) }
    $bw.Flush(); $bw.Dispose(); $fs.Dispose()
}

foreach ($pair in @(@(32, "32x32.png"), @(128, "128x128.png"), @(256, "128x128@2x.png"), @(512, "icon.png"))) {
    $b = New-AppIcon $pair[0]
    Save-Png $b (Join-Path $icons $pair[1])
    $b.Dispose()
}
Save-Ico @(16, 24, 32, 48, 64, 128, 256) (Join-Path $icons "icon.ico")

$states = @{
    "tray-idle.png"    = [System.Drawing.Color]::FromArgb(255, 150, 150, 150)
    "tray-playing.png" = [System.Drawing.Color]::FromArgb(255, 63, 191, 174)
}
foreach ($k in $states.Keys) {
    $b = New-TrayIcon 32 $states[$k]
    Save-Png $b (Join-Path $icons $k)
    $b.Dispose()
}

Write-Output "Icons written to $icons"

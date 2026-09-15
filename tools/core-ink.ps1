Add-Type -AssemblyName System.Drawing
$ref = [System.Drawing.Bitmap]::FromFile("D:\git\gpui-sdf-component\REFERENCE-land-map.png")
function Core-Ink($x0, $x1, $y0, $y1, $name) {
    $px = @()
    for ($y = $y0; $y -lt $y1; $y += 2) {
        for ($x = $x0; $x -lt $x1; \$x += 2) {
            $c = $ref.GetPixel($x, $y)
            $px += ,@( ($c.R + $c.G + $c.B), $c.R, $c.G, $c.B )
        }
    }
    $sorted = $px | Sort-Object { $_[0] }
    $n = [Math]::Max(1, [int]($sorted.Count * 0.08))
    $sel = $sorted[0..($n - 1)]
    $r = ($sel | ForEach-Object { $_[1] } | Measure-Object -Average).Average
    $g = ($sel | ForEach-Object { $_[2] } | Measure-Object -Average).Average
    $b = ($sel | ForEach-Object { $_[3] } | Measure-Object -Average).Average
    $rf = [Math]::Round($r / 255, 3); $gf = [Math]::Round($g / 255, 3); $bf = [Math]::Round($b / 255, 3)
    Write-Host "$name darkest-8%: R=$([Math]::Round($r)) G=$([Math]::Round($g)) B=$([Math]::Round($b))  -> ($rf, $gf, $bf)"
}
Core-Ink 1850 2160 890 1090 "big-title (DESERT)"
Core-Ink 1680 1960 1075 1120 "font-label (TASHKALP)"
Core-Ink 300 1000 40 120 "title URDAVAH area"
$ref.Dispose()

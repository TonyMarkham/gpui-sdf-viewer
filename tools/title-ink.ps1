Add-Type -AssemblyName System.Drawing
$ref = [System.Drawing.Bitmap]::FromFile("D:\git\gpui-sdf-component\REFERENCE-land-map.png")
# letters C-R-I-M-S-O-N of the big title: land-only region, warm filter
$px = @()
for ($y = 850; $y -lt 1000; $y += 2) {
    for ($x = 1700; $x -lt 2050; $x += 2) {
        $c = $ref.GetPixel($x, $y)
        if ($c.R -gt $c.B -and ($c.R + $c.G + $c.B) -lt 660) {
            $px += ,@( ($c.R + $c.G + $c.B), $c.R, $c.G, $c.B )
        }
    }
}
$sorted = $px | Sort-Object { $_[0] }
$n = [Math]::Max(1, [int]($sorted.Count * 0.10))
$sel = $sorted[0..($n - 1)]
$r = ($sel | ForEach-Object { $_[1] } | Measure-Object -Average).Average
$g = ($sel | ForEach-Object { $_[2] } | Measure-Object -Average).Average
$b = ($sel | ForEach-Object { $_[3] } | Measure-Object -Average).Average
Write-Host "title-ink darkest-10% (warm, land-only, $($sorted.Count) candidates): R=$([Math]::Round($r)) G=$([Math]::Round($g)) B=$([Math]::Round($b)) -> ($([Math]::Round($r/255,3)), $([Math]::Round($g/255,3)), $([Math]::Round($b/255,3)))"
$ref.Dispose()

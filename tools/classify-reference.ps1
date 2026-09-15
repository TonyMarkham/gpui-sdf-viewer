Add-Type -AssemblyName System.Drawing
$bmp = [System.Drawing.Bitmap]::FromFile("D:\git\gpui-sdf-component\REFERENCE-land-map.png")
$n = 256
$cls = New-Object byte[] ($n * $n)   # 0 = sea, 255 = land
$cf  = New-Object byte[] ($n * $n)   # confidence: 0 skip, 1 use
for ($y = 0; $y -lt $n; $y++) {
    for ($x = 0; $x -lt $n; $x++) {
        $sx = [int](($x + 0.5) * $bmp.Width / $n)
        $sy = [int](($y + 0.5) * $bmp.Height / $n)
        if ($sx -ge $bmp.Width) { $sx = $bmp.Width - 1 }
        if ($sy -ge $bmp.Height) { $sy = $bmp.Height - 1 }
        $c = $bmp.GetPixel($sx, $sy)
        $br = $c.B - $c.R
        if ($br -gt 12) { $cf[$y*$n+$x] = 1; $cls[$y*$n+$x] = 0 }   # sea (confident)
        elseif ($br -lt -12) { $cf[$y*$n+$x] = 1; $cls[$y*$n+$x] = 255 } # land
        else { $cf[$y*$n+$x] = 0 }
        $cf[$y*$n+$x] = if ($br -gt 12 -or $br -lt -12) { 1 } else { 0 }
        $cls[$y*$n+$x] = if ($br -lt 0) { 255 } else { 0 }
    }
}
$bmp.Dispose()
$use = 0; foreach ($b in $cf) { $use += $b }
"confident cells: $use / $($n*$n)"
[System.IO.File]::WriteAllBytes("$PWD\ref256.cls", $cls)
[System.IO.File]::WriteAllBytes("$PWD\ref256.cf", $cf)

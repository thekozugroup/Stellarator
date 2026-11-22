# One launch of the packaged Windows app for CI: start it with the probe on, wait, screenshot the
# primary screen, read its memory, then close its window the way the user does (WM_CLOSE, which
# goes through the app's close handler: the server stops, then the app quits).
#
#   app/ci/windows-session.ps1 -Name light -Exe <Stellarator.exe> -Data <folder> -Out <folder>
param([string]$Name, [string]$Exe, [string]$Data, [string]$Out)

New-Item -ItemType Directory -Force -Path $Out, $Data | Out-Null
$env:STELLARATOR_PROBE = "1"
$env:STELLARATOR_DATA_DIR = $Data
$env:RUST_LOG = "info"
$p = Start-Process -FilePath $Exe -PassThru -RedirectStandardOutput "$Out\$Name.log" -RedirectStandardError "$Out\$Name.err.log"
Start-Sleep -Seconds 15

Add-Type -AssemblyName System.Windows.Forms, System.Drawing
$bounds = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
$bitmap = New-Object System.Drawing.Bitmap $bounds.Width, $bounds.Height
$graphics = [System.Drawing.Graphics]::FromImage($bitmap)
$graphics.CopyFromScreen($bounds.Location, [System.Drawing.Point]::Empty, $bounds.Size)
$bitmap.Save("$Out\$Name.png", [System.Drawing.Imaging.ImageFormat]::Png)
$graphics.Dispose(); $bitmap.Dispose()

$p.Refresh()
$memory = "working set {0:N1} MiB, private {1:N1} MiB, peak working set {2:N1} MiB" -f ($p.WorkingSet64 / 1MB), ($p.PrivateMemorySize64 / 1MB), ($p.PeakWorkingSet64 / 1MB)
$memory | Tee-Object -FilePath "$Out\$Name-memory.txt"

if ($p.HasExited) {
  Write-Output "${Name}: the app exited early with $($p.ExitCode)"
} else {
  $closed = $p.CloseMainWindow()
  if (-not $p.WaitForExit(20000)) {
    Write-Output "${Name}: still running 20 s after closing its window (close sent: $closed); stopping it"
    Stop-Process -Id $p.Id -Force
  }
  Write-Output "${Name}: exit code $($p.ExitCode)"
}
Write-Output "--- $Name output"
Get-Content "$Out\$Name.log", "$Out\$Name.err.log" -ErrorAction SilentlyContinue | Select-Object -Last 40

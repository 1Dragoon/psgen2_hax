foreach ($file in $((get-childitem "d:\"))) {
    if ($file.Name.EndsWith("DAT")) {
        $a = $(Get-FileHash -Algorithm sha256 $("d:\"+$file.Name)).Hash
        $b = $(Get-FileHash -Algorithm sha256 $("C:\Users\jjd\code\psgen2_repack\"+$file.Name)).Hash
        $blength = $(get-item $("C:\Users\jjd\code\psgen2_repack\"+$file.Name)).Length
        Write-Host "$($file.Name) $($a -eq $b) $a $($file.Length), $b $blength"
    }
}

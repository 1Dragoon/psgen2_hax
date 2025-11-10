# Aeroprism alpha

Use `aeroprism --help` for general usage information. A few usage examples:

### Example:
To extract the English ISO and DAT file structure to a directory. Assumes the ISO is already mounted to e:\

`aeroprism e:\ -e -o c:\psgen2_en_workspace`

To get right to editing the text output, have a look at the EVENT.DAT/xxxx.eventdialog.lz77.toml files. Please do not rename anything as it can break the assumed build order.

### Example:
To rebuild the DAT files into a directory ready for creating an ISO files:

`aeroprism c:\psgen2_en_workspace -r -o c:\psgen2_en_iso`

### You can then build the ISO from these files and boot right to it in an emulator. Example with mkisofs:

mkisofs -o c:\users\myname\Documents\PCSX2\games\test.iso c:\psgen2_en_iso

Note: I can't comment on whether this will run on a real PS2, as I don't have one to test on.
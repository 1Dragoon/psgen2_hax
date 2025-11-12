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

### Performance tips:

If you have no intention of modifying the image files, you can use the `-c` parameter save yourself some time on the repacking by having Aeroprism simply copy them over to the destination folder without decompressing or converting them. While this process is pretty fast, LZ77 compressing (in a way that remains compatible with the game) the SGGG image format is relatively slow compared to everything else, and there are a lot of files so it adds up. On my system, this reduces the repackaging time from 20 seconds to just under 3.

Although I could implement a caching system like algoring has, I'm not certain it's worth the effort as SGGG compression is where a good 85% or so of the time is spent.

If you're working from a spinning disk, Aeroprism is probably going to cause some heavy disk thrashing as it maximizes the use of every last one of your CPU cores. HDDs don't tolerate rapid random access particularly well where SSDs generally do. If this is causing a problem on your setup, you might consider lowering the thread count to 1 or 2, using the `-t X` parameter, where `X` is the number of cores you want to use. It defaults to the total number of cores available to your OS.

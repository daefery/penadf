**PenaDF** is a small, offline PDF app for Windows and macOS: open, sign, comment on, fill, organize, compare and protect PDFs on your own PC. Free and open source; nothing is uploaded.

### Download

- **PenaDF-Setup.exe** (recommended): installs for your account, no admin rights. Indonesian or English, following Windows. Tick "Make PenaDF my default PDF app" on the last page to open PDFs with it.
- **PenaDF-windows-x64.zip**: portable. Unzip and run `PenaDF.exe`, keeping `pdfium.dll` next to it.
- **PenaDF-macOS.dmg**: for Apple Silicon and Intel Macs (macOS 11 or later). Drag PenaDF onto Applications. The first time, macOS asks you to allow it: System Settings → Privacy & Security → **Open Anyway** (PenaDF is not notarized by Apple yet).

Checksums: `SHA256SUMS.txt`.

The installer is not code-signed yet, so Windows may show "Windows protected your PC". Choose **More info → Run anyway**.

### New in 0.3.1

- Select and copy text: drag over text to select it (double-click for one word, Ctrl+A for the whole page), then copy with Ctrl+C (⌘C on Mac), Edit → Copy or the right-click menu. Password-protected PDFs that forbid copying say so instead.

### In this version

- View PDFs in tabs; double-clicking more PDFs adds tabs to the open window.
- Sign (draw once, place with a drag), highlight, underline, strike out, sticky notes, freehand drawing.
- Fill forms; organize, merge, split, crop and rotate pages; compare two versions; password protection (AES-256).
- Create PDFs from images and text (and from Office files when Microsoft Office or LibreOffice is installed); export to PNG/JPEG, TXT and HTML.

Requires Windows 10 or 11 (64-bit), or macOS 11 or later.

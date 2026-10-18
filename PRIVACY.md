# Privacy

This program will not transfer any information to other networked systems unless
specifically requested by the user or the person installing or operating it.

In detail:

* PenaDF has no accounts, analytics, telemetry, ads or automatic update checks.
* Documents are opened, changed and saved only on your PC.
* PenaDF stores two things in `%APPDATA%\PenaDF`: your saved signature, if you choose to save
  it, and the choice to hide the "make default" reminder. Delete that folder to remove them.
* The installer writes Windows registry entries so PenaDF can open PDFs and appear in
  Settings → Default apps. The uninstaller removes them.
* Two actions start another program on your PC, and only when you choose them:
  * **Print** hands the file to Windows' print command.
  * **Create PDF from Office files** asks Microsoft Office or LibreOffice, if installed, to convert the file.

  Neither sends anything over the network.

If a future version adds an online feature (for example official e-Meterai), it will only run
when you ask for it, will say which service receives the document, and this page will be
updated first.

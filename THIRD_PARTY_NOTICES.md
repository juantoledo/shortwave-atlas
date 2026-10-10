# Third-party software in Shortwave Atlas installers

The Windows and macOS installers include one program that Shortwave Atlas runs as a separate
process. It is built by `assets/sidecars/build.py`, from the sources below, and its license
texts are installed in the `licenses/` folder next to this file.

Linux packages do not ship it: they use the system's Hamlib package.

## Hamlib 4.7.2: `rigctld`

Talks to the radio over its CAT serial port.

- License: GNU GPL v2 or later (the `rigctld` program), GNU LGPL v2.1 or later (the library).
- Home: https://hamlib.github.io/
- Source: https://github.com/Hamlib/Hamlib/releases/download/4.7.2/hamlib-4.7.2.tar.gz
  (SHA-256 `ae1fcf2dbc80ea0786ea8f047b09399c3f7737d1930442f61a031708ed33e88f`)
- Windows: the Hamlib project's own build,
  https://github.com/Hamlib/Hamlib/releases/download/4.7.2/hamlib-w64-4.7.2.zip
  (`rigctld.exe` and its DLLs: `libhamlib-4.dll`, `libusb-1.0.dll`, `libgcc_s_seh-1.dll`,
  `libwinpthread-1.dll`).
- macOS: built from the source above, statically, with
  `--disable-shared --enable-static --without-readline --without-libusb --without-cxx-binding
  --without-xml-support --without-indi --disable-winradio`.

To rebuild it, or to build it from modified sources, run
`python3 assets/sidecars/build.py <target>` from the Shortwave Atlas source tree.

# Data and artwork in every build

## EiBi shortwave schedules

The station schedules and transmitter sites (`data/eibi/source`, bundled into every build) are
EiBi's files, unchanged: http://www.eibispace.de/dx/ by Eike Bierwirth. From its README.TXT:
"All my frequency lists are free of cost, and any person is absolutely free to download, use,
copy, or distribute these files or to use them within third-party software."

## flag-icons 7.5

Country flags in the user interface. https://github.com/lipis/flag-icons

The MIT License (MIT)

Copyright (c) 2013 Panayiotis Lipiridis

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and
associated documentation files (the "Software"), to deal in the Software without restriction,
including without limitation the rights to use, copy, modify, merge, publish, distribute,
sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or
substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT
NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM,
DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT
OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

# Third-party software in SW Atlas installers

The Windows and macOS installers include two programs that SW Atlas runs as separate
processes. They are built by `assets/sidecars/build.py`, from the sources below, and their
license texts are installed in the `licenses/` folder next to this file.

Linux packages ship neither: they use the system's Hamlib and ffmpeg packages.

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

## FFmpeg 9.0.2: `ffmpeg`

Captures the radio's audio from its USB sound card.

- License: GNU LGPL v2.1 or later. The build enables no GPL or nonfree parts.
- Home: https://ffmpeg.org/
- Source: https://ffmpeg.org/releases/ffmpeg-9.0.2.tar.xz
  (SHA-256 `8c3850283eb25fa026482078a04051e0be17347b09ef81a0849bec15a96e002e`)
- Configured with
  `--disable-everything --enable-small --disable-doc --disable-debug --disable-network
  --disable-ffplay --disable-ffprobe --disable-x86asm --enable-ffmpeg --enable-swresample
  --enable-protocol=pipe,file --enable-muxer=pcm_s16le --enable-encoder=pcm_s16le
  --enable-decoder=pcm_s16le,pcm_s24le,pcm_s32le,pcm_f32le
  --enable-filter=abuffer,abuffersink,aformat,anull,aresample`, plus:
  - Windows (cross-compiled with MinGW-w64): `--disable-autodetect --enable-cross-compile
    --target-os=mingw32 --arch=x86_64 --cross-prefix=x86_64-w64-mingw32- --pkg-config=false
    --extra-ldflags=-static --enable-indev=dshow`
  - macOS: `-arch <arch> -mmacosx-version-min=11.0` and `--disable-sdl2 --disable-iconv
    --disable-xlib --disable-libxcb --enable-indev=avfoundation`

To rebuild either program, or to build it from modified sources, run
`python3 assets/sidecars/build.py <target>` from the SW Atlas source tree.

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

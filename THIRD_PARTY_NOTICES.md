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

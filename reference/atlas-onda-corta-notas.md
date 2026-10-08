# Atlas de Onda Corta: notas de diseño y arquitectura

Resumen de la conversación de planificación (8 oct 2026). Pensado para pasarlo a Claude Code como contexto inicial.

## 1. La idea

App de escritorio (Windows, Linux, macOS) que se conecta a un rig por Hamlib y, mientras sintonizas, muestra los datos de la estación de forma visual.

- Ejemplo: sintonizas 13570 kHz AM, el globo vuela al transmisor de Radio Martí en **Greenville, Carolina del Norte** (no California), lanza ondas desde el sitio y traza el gran círculo hasta tu ubicación (QTH).
- **Sin TX.** Solo cambiar frecuencia y modo, leer la señal y encender o apagar. El fuerte es la visualización, la experiencia y la base de datos de estaciones.
- Modelo mental: el rig es un sensor de frecuencia; el producto son los datos y la interfaz.

## 2. Decisiones tomadas

| Tema | Decisión |
|---|---|
| Control del rig | `rigctld` (Hamlib) como proceso aparte, hablado por TCP. No enlazar Hamlib. |
| Interfaz | Web (HTML/JS) dentro de un contenedor de escritorio. |
| Globo 3D | Globe.gl (Three.js). Alternativa: MapLibre + deck.gl. |
| Datos | SQLite local, offline. Programación de EiBi aparte de las notas del usuario. |
| Rig durante el desarrollo | `rigctld -m 1` (rig de prueba de Hamlib) y un `RigBackend` simulado. |

## 3. Contenedor de escritorio: Tauri v2 o Electron

| | Tauri v2 | Electron |
|---|---|---|
| Núcleo | Rust | Node.js |
| Peso del instalador | ~10-20 MB | ~100-150 MB |
| Render | WebView del sistema (puede variar entre sistemas) | Chromium propio, igual en todos lados |
| Control del rig | Rust (tokio, TCP) | Node (`net`), mismo lenguaje que la interfaz |

- **Referencia: Nexus** (https://github.com/kd9taw/Nexus, GPLv3). Usa Rust + Tauri v2 + React/TypeScript, con Hamlib dentro del instalador y un globo 3D en la interfaz. Según su README: Windows con instalador NSIS sin firmar; Linux con AppImage y `.deb`; sin build de macOS listado. Es una app mucho más grande (módems, logbook, satélites): tomar la **forma** del proyecto, no el código (la licencia GPLv3 obligaría a publicar el nuestro igual).
- **Recomendación:** Tauri v2 si te sientes cómodo con Rust; Electron si quieres avanzar rápido solo con JavaScript. La interfaz es casi idéntica en ambos, así que la decisión es barata de cambiar.
- Pendiente de verificar: el stack de GridTracker. Lo que se dijo en la conversación (GridTracker 1 sobre NW.js, GridTracker 2 sobre Electron, ambos con OpenLayers) era de memoria y **no se pudo confirmar**. Las docs oficiales solo confirman que escucha tráfico de WSJT-X, lee logs ADIF y tiene capas como la línea gris.

## 4. Arquitectura

```
Rig ──CAT──> rigctld (Hamlib) ──TCP :4532──> Núcleo (Rust o Node) ──eventos──> Interfaz web
                                                   │                               │
                                                   └── SQLite (estaciones, logs) ──┘
```

1. **Capa de rig:** el núcleo lanza `rigctld` como proceso hijo y le habla por TCP. Consulta cada 150-200 ms y emite un evento solo cuando cambia algo.
2. **Interfaz `RigBackend`:** `get_state()`, `set_freq()`, `set_mode()`, `set_power()`. Implementaciones: Hamlib, simulador y, a futuro, SDR.
3. **Núcleo:** lógica "frecuencia + hora UTC + tolerancia ±1 kHz → estaciones candidatas", ordenadas por: al aire ahora, y luego más cerca de tu QTH.
4. **Interfaz:** mapa, ficha de la estación y sintonizador. Solo escucha eventos del rig.
5. **Opcional:** ofrecer un puerto compatible con `rigctld` propio, para que WSJT-X, GridTracker y otros compartan la radio (Nexus lo hace).

### Hamlib: protocolo mínimo (`rigctld`, puerto 4532)

| Comando | Qué hace |
|---|---|
| `f` / `F 13570000` | Leer / fijar frecuencia (**en Hz**) |
| `m` / `M AM 6000` | Leer / fijar modo y ancho de banda |
| `l STRENGTH` | Nivel de señal (dB relativo a S9) |
| `\get_powerstat` / `\set_powerstat` | Estado de energía |

Las respuestas de escritura terminan en `RPRT 0` si salió bien.

Lanzar con el rig real: `rigctld -m <modelo> -r <puerto> -s <baudios>`; el número de modelo y qué soporta cada uno se ve con `rigctl -l` y `rigctl -m <modelo> -u`.

### Cosas a tener en cuenta con Hamlib

- Cada rig soporta un subconjunto distinto de funciones. **Falta saber qué rig tienes** (marca, modelo, USB/serial/red).
- **Encender por CAT** suele ser poco fiable con el equipo apagado; apagar funciona mejor.
- El medidor de señal puede ser aproximado: sirve para una barra visual.
- Dos programas no pueden abrir el mismo puerto serial. La salida limpia: tu app se conecta a un `rigctld` que ya corra, y se ofrece en ajustes tanto "usar el mío" como "usar uno existente".
- WSJT-X trae Hamlib incluido (de memoria: enlazado a la librería, con `rigctld` propio en su carpeta de instalación) y puede conectarse a uno externo con el modelo "Hamlib NET rigctl".
- Licencia: Hamlib es LGPL; como proceso aparte simplifica el cumplimiento.
- Linux: el usuario debe estar en el grupo `dialout` para usar el puerto serial.

## 5. Datos de estaciones

- **Fuente de programación:** EiBi (eibispace.de), CSV por temporada (dos al año). Formato de memoria: separado por `;` con frecuencia, horario UTC, días, país ITU, estación, idioma, destino y observaciones. **Verificar el formato exacto al implementarlo.** Las coordenadas de los transmisores hay que cruzarlas con una tabla propia de sitios.
- **Esquema SQLite sugerido:** `stations`, `transmitter_sites`, `schedules`, `my_logs`, `media`.
- **Capas:** programación importada (se reemplaza en cada temporada) y datos curados por el usuario (logos, fotos, notas de escucha, reportes con fecha y SINPO, grabaciones). Van en tablas separadas para que una actualización no pise lo curado.
- Prever un actualizador de la base cada temporada.
- **Los datos del prototipo son de muestra**, sin verificar: 10 estaciones con horarios y potencias aproximadas (varias potencias figuran como "n/d").

## 6. Animación del globo

- **Librería:** Globe.gl (el prototipo usa la versión 2.46.2, cargada por CDN). Soporta cámara animada, anillos pulsantes, arcos animados, trayectorias sobre la superficie y elementos HTML anclados a coordenadas.
- **Secuencia de "película" implementada** en `lockOn()`:
  1. Alejar la cámara y girar hacia el transmisor, con interpolación esférica y una curva suave. Duración y alejamiento proporcionales a la distancia.
  2. Aterrizar a altitud 1.5 y mostrar la etiqueta con anillos que se expanden.
  3. Tras 450 ms, trazar el arco hasta el QTH (más la trayectoria de gran círculo sobre la superficie).
  4. Tras 1.5 s, anillo azul de "señal recibida" en el QTH.
  5. Opcional: reencuadrar la ruta completa a la altura adecuada.
- Si la estación está **fuera del aire** a esa hora, la cámara igual vuela, pero sin ondas ni arco.
- **Terminador día/noche:** textura del mapa dibujada en un canvas y re-renderizada cada 2 minutos con la posición real del sol. Sin imágenes externas.
- Respeta `prefers-reduced-motion` (sin vuelo ni rotación automática).

## 7. Prototipo adjunto

`atlas-onda-corta.html` es una sola página autocontenida (los contornos de continentes y fronteras van embebidos; el globo viene de CDN) con:

- globo con textura dibujada, terminador solar, atmósfera y fondo estrellado;
- rig simulado: lectura de frecuencia, modos AM/USB/LSB/CW, medidor de señal falso, botón de encendido, dial con marcas en las frecuencias de la base;
- ficha de la estación: estado al aire (con minutos restantes), transmisor, distancia, rumbo, hora local allá, longitud de onda, barra de horario UTC y candidatas en la misma frecuencia;
- clic en el globo para mover el QTH (por defecto Santiago de Chile).

Qué reemplazar al migrar a la app real: el estado simulado de frecuencia y modo por eventos del núcleo (`rig://freq`, `rig://mode`, `rig://signal`), y el arreglo `STATIONS` por consultas a SQLite.

Las funciones de lógica pura (distancia, rumbo, horarios UTC, posición del sol) están entre marcadores `//<pure>` en el script y se probaron aparte con resultados correctos.

## 8. Empaquetado y publicación

- **Instaladores:** Windows `.exe`/`.msi` (NSIS o MSI), macOS `.dmg` (Apple Silicon e Intel), Linux AppImage y `.deb`.
- **`rigctld` como sidecar:** en Tauri, `externalBin` con sufijo por arquitectura (por ejemplo `x86_64-pc-windows-msvc`, `aarch64-apple-darwin`); en Electron, `extraResources`.
- **Hamlib por sistema:** Windows, binarios oficiales; Linux, paquete o dependencia; macOS, compilar o tomar de Homebrew, incluir las librerías en el `.app` y firmar todo (el más trabajoso).
- **CI:** GitHub Actions con matriz para los tres sistemas, subiendo a GitHub Releases. Para actualizaciones automáticas: el actualizador integrado de Tauri o `electron-updater`.
- **Canales de terceros:** winget, Chocolatey, un cask de Homebrew y AUR (estos se vieron en las búsquedas de GridTracker 2; muchas veces los mantienen otros usuarios).
- **Firma:** sin firmar, Windows muestra SmartScreen y macOS bloquea la app. Para macOS hace falta Apple Developer ID (US$99 al año) y notarización. Para una app de radioaficionados también se puede publicar sin firmar, con instrucciones de "abrir de todos modos".

## 9. Plan por fases

1. **Esqueleto:** proyecto de escritorio que muestra el prototipo y se conecta a `rigctld -m 1`; leer y fijar frecuencia y modo desde la interfaz.
2. **Base de datos:** importar EiBi a SQLite, con tabla de sitios y búsqueda por frecuencia y hora.
3. **Mapa:** reemplazar los datos de muestra, pulir animaciones, mejorar la textura (más detalle de costas si hace falta).
4. **Rig real:** probar con tu equipo; documentar qué soporta.
5. **Extras:** notas y registros propios, modo pantalla completa, puerto `rigctld` compartido.
6. **Publicación:** CI para los tres sistemas, firma, canales de terceros.

## 10. Preguntas abiertas

- ¿Qué rig tienes y cómo se conecta? Define el número de modelo de Hamlib y qué funciones existen.
- ¿Tauri (Rust) o Electron (JS)?
- ¿Se publica una versión para macOS desde el comienzo?
- ¿Importar EiBi completo o empezar con un subconjunto?
- ¿Idioma de la interfaz: solo español o también inglés?

## 11. Prompt de arranque sugerido para Claude Code

> Construye una app de escritorio multiplataforma llamada "Atlas de Onda Corta" con [Tauri v2 | Electron]. Incluye `atlas-onda-corta.html` como punto de partida de la interfaz (globo con Globe.gl). Primer objetivo: lanzar `rigctld -m 1` como proceso hijo, conectarse por TCP al puerto 4532, consultar frecuencia, modo y señal cada 200 ms y emitir eventos a la interfaz para reemplazar el simulador. Mantén el rig detrás de una interfaz `RigBackend` con una implementación Hamlib y otra simulada. Deja la lógica de "frecuencia → estaciones" en un módulo aparte, sin dependencias de interfaz, con pruebas. Aún no toques base de datos ni empaquetado.

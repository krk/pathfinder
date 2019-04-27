# Pathfinder 3 - Turtle Demo

A demo that can display a subset of KTurtle "turtle graphics" is added to the project.

Turtle files are parsed with a [lalrpop](https://github.com/lalrpop/lalrpop) grammar and rendered using Pathfinder 3.

It even has a tiger as its default graphics:

![TIGER](https://github.com/krk/pathfinder/blob/pf3-turtle/tiger.png)


* Turtle commands (a subset is supported): https://docs.kde.org/trunk5/en/kdeedu/kturtle/commands.html
* Grammar: https://github.com/krk/pathfinder/blob/pf3-turtle/uturtle/src/turtle.lalrpop
* Example turtle file: https://github.com/krk/pathfinder/blob/pf3-turtle/resources/turtle/tiger.turtle

---

Pathfinder 3 is a fast, practical, GPU-based rasterizer for fonts and vector graphics using OpenGL
and OpenGL ES 3.0+.

Please note that Pathfinder is under heavy development and is incomplete in various areas.

The project features:

* High quality antialiasing. Pathfinder can compute exact fractional trapezoidal area coverage on a
  per-pixel basis for the highest-quality antialiasing possible (effectively 256xAA).

* Fast CPU setup, making full use of parallelism. Pathfinder 3 uses the Rayon library to quickly
  perform a CPU tiling prepass to prepare vector scenes for the GPU. This prepass can be pipelined
  with the GPU to hide its latency.

* Fast GPU rendering, even at small pixel sizes. Even on lower-end GPUs, Pathfinder typically
  matches or exceeds the performance of the best CPU rasterizers. The difference is particularly
  pronouced at large sizes, where Pathfinder regularly achieves multi-factor speedups. All shaders
  have no loops and minimal branching.

* Advanced font rendering. Pathfinder can render fonts with slight hinting and can perform subpixel
  antialiasing on LCD screens. It can do stem darkening/font dilation like macOS and FreeType in
  order to make text easier to read at small sizes. The library also has support for gamma
  correction.

* Support for SVG. Pathfinder 3 is designed to efficiently handle workloads that consist of many
  overlapping vector paths, such as those commonly found in SVG and PDF files. It can perform
  occlusion culling, which often results in dramatic performance wins over typical software
  renderers that use the painter's algorithm. A simple loader that leverages the `resvg` library
  to render a subset of SVG is included, so it's easy to get started.

* 3D capability. Pathfinder can render fonts and vector paths in 3D environments without any loss
  in quality. This is intended to be useful for vector-graphics-based user interfaces in VR, for
  example.

* Lightweight. Unlike large vector graphics packages that mix and match many different algorithms,
  Pathfinder 3 uses a single, simple technique. It consists of a set of modular crates, so
  applications can pick and choose only the components that are necessary to minimize dependencies.

* Portability to most GPUs manufactured in the last decade, including integrated and mobile GPUs.
  Geometry, tessellation, and compute shader functionality is not required.

## Building

Pathfinder 3 is a set of modular packages, allowing you to choose which parts of the library you
need. An SVG rendering demo, written in Rust, is included, so you can try Pathfinder out right
away. It also provides an example of how to use the library. (Note that, like the rest of
Pathfinder, the demo is under heavy development and has known bugs.)

Running the demo is as simple as:

    $ cd demo/native
    $ RUSTFLAGS="-C target-cpu=native" cargo run --release

On macOS, it is recommended that you force the use of the integrated GPU, as issues with Apple's
OpenGL drivers may limit performance on discrete GPUs. You can use
[gfxCardStatus.app](https://gfx.io/) for this.

## Authors

The primary author is Patrick Walton (@pcwalton), with contributions from the Servo development
community.

The logo was designed by Jay Vining.

Contributors to Pathfinder are expected to abide by the same Code of Conduct as Rust itself.

## License

Pathfinder is licensed under the same terms as Rust itself. See `LICENSE-APACHE` and `LICENSE-MIT`.

Material Design icons are copyright Google Inc. and licensed under the Apache 2.0 license.

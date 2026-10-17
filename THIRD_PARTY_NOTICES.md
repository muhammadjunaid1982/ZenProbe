# Third-party notices

## Bruno

ZenBruno is an independent reimplementation and is not affiliated with or endorsed
by the Bruno project or Bruno Software Inc. "Bruno" is used only to describe file-format
compatibility.

ZenBruno's `.bru` grammar handling, OpenCollection YAML mapping, variable interpolation,
script ordering, auth inheritance and cURL/URL behaviour were ported from Bruno's
MIT-licensed source (`@usebruno/lang`, `@usebruno/filestore`, `@usebruno/common` and the
Bruno Electron app). Bruno's license:

```
MIT License

Copyright (c) 2022 Anoop M D, Anusree P S and Contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## Rust crates

ZenBruno links many open-source crates (egui/eframe, boa, reqwest, tonic, prost,
protox, tungstenite, serde, tokio and their dependencies), each under its own license
(mostly MIT and/or Apache-2.0). Run `cargo tree` to list them, or
[`cargo about`](https://github.com/EmbarkStudios/cargo-about) to generate a full
license report.

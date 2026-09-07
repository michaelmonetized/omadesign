# Native decompression libraries

LibRaw's DNG decoder is linked with MozJPEG (mozjpeg-sys 2.2.3,
https://github.com/kornelski/mozjpeg-sys) and zlib (libz-sys 1.1.29,
https://github.com/rust-lang/libz-sys). Cargo.lock pins their exact sources and
checksums. Their original notices accompany this file. Both compile statically;
no system JPEG/zlib or external command is needed to open RAW photographs.

Portable Linux releases also statically link Zig's LLVM libc++, libc++abi and
libunwind runtimes. Their original Apache 2.0 / LLVM exception notices accompany
this file (`LICENSE-libcxx`, `LICENSE-libcxxabi`, `LICENSE-libunwind`).

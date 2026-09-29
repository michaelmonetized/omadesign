# Candidate build verification

Product source `8ee01b4ea08e0dac9891694b82dd3f4f377a5cf2` passed the all-target check and **906 library tests, 0 failed, 7 ignored**. Both architecture builds completed. [Build receipt](build-receipt.json) contains exact binary/runtime hashes, compiler identity, commands and build profiles. [Publication receipt](publication.json) records original and sanitized log hashes.

QA and portable binaries use the original fleet benchmark release profile with LTO disabled. Portable binaries target glibc 2.35 through the repository Zig wrappers. This is the candidate intended for 0.6.1; its package version is still 0.6.0. No installed release or earlier accepted human QA evidence was replaced.

The two cross builds emitted the same deprecated linker optimization warning and succeeded. Absolute repository and target paths are replaced in published logs; the immutable original build receipt remains available locally and is checked against every run.

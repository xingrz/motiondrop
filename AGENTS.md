# MotionDrop maintenance

## Product

All UI copy, errors, documentation, and commit messages must be in English.

MotionDrop is a macOS GPUI application for splitting, composing, and previewing
Google Motion Photos. Keep the interface minimal, immersive, and black. Use a
transparent native titlebar without a separate visible header. Avoid technical settings in the user interface.

The application identifier is `me.xingrz.motiondrop`. Use `xingrz.me` for personal
namespace and domain metadata. Original project code is MIT licensed; dependencies
retain their own licenses.

## Behavior to preserve

- Detect inputs by content. Accept photo/video in either order, in one drop or two.
- Name generated files `<photo stem>.MP.jpg` without leading whitespace or a
  duplicated `.MP` suffix. Never overwrite an input file.
- Export real files using GPUI native external drags, not internal-only drags.
- Preview MotionPhoto video once automatically, then show its still image.
  A floating corner button and Space replay or return to the still image.
- Preserve video/audio bytes and JPEG pixels. Keep unrelated EXIF/XMP metadata.
- Validate offsets and container boundaries before extracting media.
- Keep file processing off the UI thread and retain temporary exports until exit.
- Keep the workflow strip visible at a fixed height in all states. Show
  Photo + Video -> MotionPhoto for composition and reverse it for extraction.
- Fit photos and videos fully inside the same measured viewport, with black
  letterboxing. Never layer the photo underneath a playing video.
- Left-side source tiles accept replacements. Plain media chooses its slot by
  content. In compose mode, a MotionPhoto dropped on a source tile replaces only
  that component. Replacing split input with plain media retains the complement.
- Reverse a complete flow using the current result as its source. Preserve media
  and metadata when switching direction without edits.
- Show source thumbnails, dimensions, and video duration in the workflow strip.
- Use an icon-only playback control with a hover label. The native traffic lights
  and transparent drag region remain visible without a separate branded header.
- Put Clear next to the files it clears. Keep non-interactive text non-interactive.
- Errors must preserve the last successful workspace and explain what to do.

## Structure

- `src/motion.rs`: JPEG, XMP, and ISO BMFF parsing and composition.
- `src/workspace.rs`: background imports, image conversion, and temporary exports.
- `src/main.rs`: GPUI window, preview state, and drag/drop interaction.
- `src/player.rs`, `native/player.m`: main-thread AVFoundation playback, returning
  CoreVideo frames to GPUI's Metal surface. Preserve orientation and audio.
- `tests/`: deterministic generated media and format/workflow regression tests.
- `scripts/`, `packaging/`: reproducible app bundling and metadata.
- `.github/workflows/`: CI, packaging, and tag-triggered GitHub Releases.

The GPUI snapshot and Rust toolchain are pinned. Update Cargo.lock with dependency
changes. macOS is the supported platform; do not claim other OS support without
implementing and validating their playback and file-drag backends.

## Checks

Keep blank lines between functions and implementation blocks.
Run `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings`, and
`cargo test --locked` for relevant changes. Build the app when native code changes.
Manually check Finder drop-in/drop-out and playback-to-still transitions after
changing GPUI or playback integration. Use synthetic fixtures, never private media.
Avoid tests that only repeat implementation details.

## Git and documentation

Maintain this project in its repository, not in a separate generated project.
Commit changes autonomously in coherent, reviewable groups. Use English
Conventional Commits, e.g. `feat: preview motion photos inline`.

Write a short imperative subject explaining what changed. The body briefly
explains why. Hard-wrap body paragraphs at 75 characters. End every authored commit
with a `Co-authored-by: <actual AI model name and version> <email>` trailer. Use the
actual model identity supplied by the runtime, never invent a more specific version.
For a GPT-6 runtime, use `Co-authored-by: GPT-6 <noreply@openai.com>`.

Document only the resulting design, behavior, and maintenance requirements.
Do not record conversations, requests, implementation diaries, or abandoned plans
in project files or commit messages. README is for humans: explain what the app does,
how to use it, supported formats, and essential build/release steps. Keep detailed
implementation guidance in this file or concise code comments.

## Releases

GitHub Actions owns CI, building, packaging, and releases. The macOS matrix targets
Apple Silicon and Intel. A `vX.Y.Z` tag must match the Cargo package version.
Use the domain-derived bundle ID and derive bundle versions from Cargo.toml.
Do not publish a tag or Release unless requested. Do not claim a remote workflow
passed based only on local checks. Signing is ad-hoc until developer signing and
notarization credentials are explicitly configured.

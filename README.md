# MotionDrop

A macOS utility for extracting photos and videos from Google Motion Photos,
and combining a photo and video into a MotionPhoto.

## Usage

Download the build for your Mac from GitHub Releases and open `MotionDrop.app`.

- Drop a MotionPhoto into the window, then drag out its photo or video to save it.
- Drop a photo and video in either order, then drag out the combined `.MP.jpg` file.

Click the middle arrow to reverse the flow. Replace inputs by dropping files
onto the left-side previews. A MotionPhoto dropped onto a photo or video input
replaces only that component.

You can also select files with `Cmd+O`. Original files are left unchanged.
Save the results before closing the app.

Supported inputs are JPEG, PNG, WebP, MP4, and MOV. PNG and WebP images are
converted to JPEG. HEIC, AVIF, and composition of HDR gain-map JPEGs are not
currently supported. Each input file can be up to 512 MB.

## Development

Requires macOS, Xcode Command Line Tools, and Rust.

```sh
cargo run --locked
cargo test --locked
./scripts/bundle-macos.sh
```

The application is built at `dist/MotionDrop.app`.
GitHub Actions builds Apple Silicon and Intel packages. Pushing a `vX.Y.Z` tag
matching the package version publishes a GitHub Release with ZIPs and checksums.
Current builds are ad-hoc signed and are not Apple-notarized.

## License

[MIT](LICENSE) · [XiNGRZ](https://xingrz.me)

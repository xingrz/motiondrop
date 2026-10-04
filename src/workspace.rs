use crate::motion;
use anyhow::{Context, Result, bail, ensure};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
};

const MAX_BYTES: u64 = 512 * 1024 * 1024;
#[derive(Clone)]
pub struct Asset {
    pub path: PathBuf,
    pub name: String,
    pub bytes: Arc<Vec<u8>>,
    pub preview: Option<PathBuf>,
}

#[derive(Clone, Default)]
pub struct Session {
    pub photo: Option<Asset>,
    pub video: Option<Asset>,
    pub output: Option<Asset>,
    pub split: bool,
    pub converted: bool,
    // Keep exports alive even after reset while the application is running.
    pub storage: Vec<Arc<tempfile::TempDir>>,
}

fn read(path: &Path) -> Result<Vec<u8>> {
    ensure!(path.is_file(), "Drop files, not folders");
    let mut bytes = vec![];
    fs::File::open(path)?
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= MAX_BYTES,
        "Each file must be smaller than 512 MB"
    );
    Ok(bytes)
}

fn asset(dir: &Path, name: String, bytes: Vec<u8>, preview: Option<PathBuf>) -> Result<Asset> {
    let path = dir.join(&name);
    fs::write(&path, &bytes)?;
    Ok(Asset {
        path,
        name,
        bytes: Arc::new(bytes),
        preview,
    })
}

fn thumbnail(bytes: &[u8], dir: &Path) -> Option<PathBuf> {
    let mut decoder = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?
        .into_decoder()
        .ok()?;
    use image::ImageDecoder;
    let orientation = decoder.orientation().ok();
    let mut image = image::DynamicImage::from_decoder(decoder).ok()?;
    if let Some(o) = orientation {
        image.apply_orientation(o);
    }
    let path = dir.join("preview.png");
    image.thumbnail(1000, 700).save(&path).ok()?;
    Some(path)
}

pub fn import(mut session: Session, paths: Vec<PathBuf>) -> Result<Session> {
    ensure!(
        !paths.is_empty() && paths.len() <= 2,
        "Drop one MotionPhoto, or one photo and one video"
    );
    let dir = Arc::new(tempfile::Builder::new().prefix("motiondrop-").tempdir()?);
    let mut photos = vec![];
    let mut videos = vec![];
    let mut converted = false;
    for path in &paths {
        let bytes = read(path).with_context(|| format!("Could not read {}", path.display()))?;
        let stem = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        if bytes.starts_with(&[0xff, 0xd8]) {
            if let Some(parts) = motion::split(&bytes)? {
                ensure!(
                    paths.len() == 1,
                    "Drop a MotionPhoto on its own to split it"
                );
                let preview = thumbnail(&parts.photo, dir.path());
                let ext = if parts.mime == "video/quicktime" {
                    "mov"
                } else {
                    "mp4"
                };
                session.photo = Some(asset(
                    dir.path(),
                    format!("{stem}-photo.jpg"),
                    parts.photo,
                    preview,
                )?);
                session.video = Some(asset(
                    dir.path(),
                    format!("{stem}-video.{ext}"),
                    parts.video,
                    None,
                )?);
                session.output = None;
                session.split = true;
                session.converted = false;
                session.storage.push(dir);
                return Ok(session);
            }
            photos.push((stem, bytes));
        } else if image::guess_format(&bytes).is_ok() {
            let image = image::load_from_memory(&bytes).context("Could not read this photo")?;
            let mut jpeg = vec![];
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 95)
                .encode_image(&image.to_rgb8())?;
            photos.push((stem, jpeg));
            converted = true;
        } else {
            let mime = motion::video_mime(&bytes)
                .context("Use JPEG, PNG or WebP photos and MP4 or MOV videos. HEIC and AVIF are not supported yet.")?;
            videos.push((stem, bytes, mime));
        }
    }
    ensure!(
        photos.len() <= 1 && videos.len() <= 1,
        "Choose one photo and one video, not two files of the same type"
    );
    if session.split {
        session.photo = None;
        session.video = None;
        session.converted = false;
    }
    session.split = false;
    session.output = None;
    if let Some((name, bytes)) = photos.pop() {
        let preview = thumbnail(&bytes, dir.path());
        session.photo = Some(asset(
            dir.path(),
            format!("{name}-photo.jpg"),
            bytes,
            preview,
        )?);
        session.converted = converted;
    }
    if let Some((name, bytes, mime)) = videos.pop() {
        let ext = if mime == "video/quicktime" {
            "mov"
        } else {
            "mp4"
        };
        session.video = Some(asset(
            dir.path(),
            format!("{name}-video.{ext}"),
            bytes,
            None,
        )?);
    }
    if let (Some(photo), Some(video)) = (&session.photo, &session.video) {
        let bytes = motion::compose(&photo.bytes, &video.bytes)?;
        let stem = photo.name.strip_suffix("-photo.jpg").unwrap_or("Motion");
        session.output = Some(asset(
            dir.path(),
            format!(
                "{}.MP.jpg",
                stem.trim_start()
                    .strip_suffix(".MP")
                    .unwrap_or(stem.trim_start())
            ),
            bytes,
            photo.preview.clone(),
        )?);
    }
    if session.photo.is_none() && session.video.is_none() {
        bail!("No supported files were found");
    }
    session.storage.push(dir);
    Ok(session)
}

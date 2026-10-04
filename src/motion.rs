use anyhow::{Context, Result, bail, ensure};
use std::ops::Range;

const XMP: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";
const CAMERA: &str = "http://ns.google.com/photos/1.0/camera/";
const CONTAINER: &str = "http://ns.google.com/photos/1.0/container/";
const ITEM: &str = "http://ns.google.com/photos/1.0/container/item/";
const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";

struct Jpeg {
    end: usize,
    xmp: Vec<Range<usize>>,
    mpf: bool,
}

// Walk JPEG markers and entropy data: embedded thumbnails and FF00 are not EOI.
fn jpeg(data: &[u8]) -> Result<Jpeg> {
    ensure!(data.starts_with(&[0xff, 0xd8]), "A JPEG photo is required");
    let (mut pos, mut entropy) = (2, false);
    let mut result = Jpeg {
        end: 0,
        xmp: vec![],
        mpf: false,
    };
    while pos < data.len() {
        if entropy && data[pos] != 0xff {
            pos += 1;
            continue;
        }
        ensure!(data[pos] == 0xff, "Invalid JPEG marker");
        let start = pos;
        while data.get(pos) == Some(&0xff) {
            pos += 1;
        }
        let tag = *data.get(pos).context("Incomplete JPEG file")?;
        pos += 1;
        if entropy && (tag == 0 || (0xd0..=0xd7).contains(&tag)) {
            continue;
        }
        if tag == 0xd9 {
            result.end = pos;
            return Ok(result);
        }
        ensure!(tag != 0 && tag != 0xd8, "Invalid JPEG marker");
        if tag == 1 || (0xd0..=0xd7).contains(&tag) {
            continue;
        }
        let len = data.get(pos..pos + 2).context("Incomplete JPEG segment")?;
        let len = u16::from_be_bytes([len[0], len[1]]) as usize;
        ensure!(len >= 2, "Invalid JPEG segment length");
        let payload = data
            .get(pos + 2..pos + len)
            .context("JPEG segment extends beyond the file")?;
        if tag == 0xe1 && payload.starts_with(XMP) {
            result.xmp.push(start..pos + len);
        }
        if tag == 0xe2 && payload.starts_with(b"MPF\0") {
            result.mpf = true;
        }
        pos += len;
        entropy = tag == 0xda;
    }
    bail!("Missing JPEG end marker")
}

fn xml<'a>(data: &'a [u8], range: &Range<usize>) -> Result<&'a str> {
    // Marker fill bytes are legal; find APP1's two-byte length before its payload.
    let marker = range.start
        + data[range.clone()]
            .iter()
            .take_while(|b| **b == 0xff)
            .count();
    std::str::from_utf8(&data[marker + 3 + XMP.len()..range.end]).context("XMP is not valid UTF-8")
}

fn prop(doc: &roxmltree::Document<'_>, ns: &str, name: &str) -> Option<String> {
    for n in doc.descendants().filter(|n| n.is_element()) {
        if let Some(v) = n.attribute((ns, name)) {
            return Some(v.to_string());
        }
        if n.has_tag_name((ns, name)) {
            return n.text().map(str::to_string);
        }
    }
    None
}

/// Remove only motion metadata, maintaining byte offsets for MPF/gainmap images.
fn clean_xmp(text: &str) -> Result<String> {
    let doc = roxmltree::Document::parse(text).context("Could not parse XMP")?;
    let mut ranges = vec![];
    for node in doc.descendants().filter(|n| n.is_element()) {
        let tag = node.tag_name();
        if (tag.namespace() == Some(CAMERA)
            && (tag.name().starts_with("MotionPhoto") || tag.name().starts_with("MicroVideo")))
            || tag.namespace() == Some(CONTAINER)
        {
            ranges.push(node.range());
        }
        for attr in node.attributes() {
            if attr.namespace() == Some(CAMERA)
                && (attr.name().starts_with("MotionPhoto") || attr.name().starts_with("MicroVideo"))
            {
                ranges.push(attr.range());
            }
        }
    }
    let mut bytes = text.as_bytes().to_vec();
    for range in ranges {
        bytes[range].fill(b' ');
    }
    String::from_utf8(bytes).context("Could not clean XMP")
}

pub fn video_mime(data: &[u8]) -> Result<&'static str> {
    // Validate top-level ISO BMFF boxes, and require a video handler in a track.
    let mut pos = 0;
    let mut mime = None;
    let mut has_video = false;
    let mut has_media = false;
    while pos < data.len() {
        let (kind, body, end) = read_box(data, pos)?;
        match kind {
            b"ftyp" => {
                ensure!(body.len() >= 8, "Invalid video ftyp box");
                mime = Some(if &body[..4] == b"qt  " {
                    "video/quicktime"
                } else {
                    "video/mp4"
                });
            }
            b"moov" => {
                has_video = contains_video_track(body)?;
            }
            b"mdat" => {
                has_media |= !body.is_empty();
            }
            _ => {}
        }
        pos = end;
    }
    ensure!(
        has_video && has_media,
        "No valid video track or media data was found"
    );
    mime.context("Use an MP4 or MOV video")
}

fn read_box(data: &[u8], pos: usize) -> Result<(&[u8], &[u8], usize)> {
    let header = data
        .get(pos..pos + 8)
        .context("Incomplete video container")?;
    let size = u32::from_be_bytes(header[..4].try_into()?) as usize;
    let (size, head) = if size == 1 {
        (
            usize::try_from(u64::from_be_bytes(
                data.get(pos + 8..pos + 16)
                    .context("Incomplete extended video header")?
                    .try_into()?,
            ))?,
            16,
        )
    } else if size == 0 {
        (data.len() - pos, 8)
    } else {
        (size, 8)
    };
    ensure!(size >= head, "Invalid video box length");
    let end = pos.checked_add(size).context("Video length overflow")?;
    let body = data
        .get(pos + head..end)
        .context("Video box extends beyond the file")?;
    Ok((&header[4..8], body, end))
}

fn contains_video_track(data: &[u8]) -> Result<bool> {
    let mut pos = 0;
    while pos < data.len() {
        let (kind, body, end) = read_box(data, pos)?;
        if kind == b"trak" && track_is_video(body)? {
            return Ok(true);
        }
        pos = end;
    }
    Ok(false)
}

fn track_is_video(data: &[u8]) -> Result<bool> {
    let mut pos = 0;
    while pos < data.len() {
        let (kind, body, end) = read_box(data, pos)?;
        if kind == b"mdia" {
            let mut p = 0;
            while p < body.len() {
                let (k, b, e) = read_box(body, p)?;
                if k == b"hdlr" && b.get(8..12) == Some(b"vide") {
                    return Ok(true);
                }
                p = e;
            }
        }
        pos = end;
    }
    Ok(false)
}

pub struct Split {
    pub photo: Vec<u8>,
    pub video: Vec<u8>,
    pub mime: &'static str,
}

pub fn split(data: &[u8]) -> Result<Option<Split>> {
    let jpeg = jpeg(data)?;
    let mut length = None;
    let mut disabled = false;
    for range in &jpeg.xmp {
        let text = xml(data, range)?;
        let doc = roxmltree::Document::parse(text).context("Could not parse XMP")?;
        if let Some(flag) = prop(&doc, CAMERA, "MotionPhoto") {
            disabled |= flag != "1";
        }
        for item in doc.descendants().filter(|n| n.is_element()) {
            if item.attribute((ITEM, "Semantic")) == Some("MotionPhoto") {
                length = item
                    .attribute((ITEM, "Length"))
                    .map(str::parse::<usize>)
                    .transpose()
                    .context("Invalid motion video length")?;
            }
        }
        if length.is_none() && prop(&doc, CAMERA, "MicroVideo").as_deref() == Some("1") {
            length = prop(&doc, CAMERA, "MicroVideoOffset")
                .map(|s| s.parse::<usize>())
                .transpose()
                .context("Invalid motion video offset")?;
        }
    }
    if disabled {
        return Ok(None);
    }
    let Some(length) = length else {
        return Ok(None);
    };
    ensure!(
        length > 0 && length <= data.len().saturating_sub(jpeg.end),
        "The motion video is missing or its length is invalid"
    );
    let start = data.len() - length;
    let mime = video_mime(&data[start..])?;
    let mut photo = data[..start].to_vec();
    for range in &jpeg.xmp {
        let text = xml(data, range)?;
        let base = range.end - text.len();
        photo[base..range.end].copy_from_slice(clean_xmp(text)?.as_bytes());
    }
    Ok(Some(Split {
        photo,
        video: data[start..].to_vec(),
        mime,
    }))
}

pub fn compose(photo: &[u8], video: &[u8]) -> Result<Vec<u8>> {
    let jpeg = jpeg(photo)?;
    ensure!(
        !jpeg.mpf,
        "This photo has an HDR gain map or multi-image index. Use a standard JPEG to compose a MotionPhoto."
    );
    ensure!(
        photo.len() == jpeg.end,
        "This photo contains appended data. Split it before composing a new MotionPhoto."
    );
    let mime = video_mime(video)?;
    ensure!(
        jpeg.xmp.len() <= 1,
        "Multiple XMP packets are not supported for composition"
    );
    let description = format!(
        r#"<rdf:Description rdf:about="" xmlns:rdf="{RDF}" xmlns:Camera="{CAMERA}" xmlns:Container="{CONTAINER}" xmlns:Item="{ITEM}" Camera:MotionPhoto="1" Camera:MotionPhotoVersion="1" Camera:MicroVideo="1" Camera:MicroVideoVersion="1" Camera:MicroVideoOffset="{}"><Container:Directory><rdf:Seq><rdf:li rdf:parseType="Resource"><Container:Item Item:Mime="image/jpeg" Item:Semantic="Primary" Item:Length="0" Item:Padding="0"/></rdf:li><rdf:li rdf:parseType="Resource"><Container:Item Item:Mime="{mime}" Item:Semantic="MotionPhoto" Item:Length="{}"/></rdf:li></rdf:Seq></Container:Directory></rdf:Description>"#,
        video.len(),
        video.len()
    );
    let packet = if let Some(range) = jpeg.xmp.first() {
        let cleaned = clean_xmp(xml(photo, range)?)?;
        let doc = roxmltree::Document::parse(&cleaned)?;
        let rdf = doc
            .descendants()
            .find(|n| n.has_tag_name((RDF, "RDF")))
            .context("Missing RDF in XMP")?;
        let part = &cleaned[rdf.range()];
        ensure!(
            !part.trim_end().ends_with("/>"),
            "Unsupported XMP RDF structure"
        );
        let end = rdf.range().start + part.rfind("</").context("Unclosed XMP RDF element")?;
        format!("{}{}{}", &cleaned[..end], description, &cleaned[end..])
    } else {
        format!(
            r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="{RDF}">{description}</rdf:RDF></x:xmpmeta>"#
        )
    };
    let size = XMP.len() + packet.len() + 2;
    ensure!(
        size <= u16::MAX as usize,
        "XMP is too large to write safely into JPEG"
    );
    let mut out = Vec::with_capacity(photo.len() + size + video.len());
    out.extend_from_slice(&photo[..2]);
    out.extend_from_slice(&[0xff, 0xe1]);
    out.extend_from_slice(&(size as u16).to_be_bytes());
    out.extend_from_slice(XMP);
    out.extend_from_slice(packet.as_bytes());
    let mut start = 2;
    for range in &jpeg.xmp {
        out.extend_from_slice(&photo[start..range.start]);
        start = range.end;
    }
    out.extend_from_slice(&photo[start..]);
    out.extend_from_slice(video);
    Ok(out)
}

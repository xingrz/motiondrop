use motiondrop::{motion, workspace};
use std::fs;
const PHOTO: &[u8] = include_bytes!("fixtures/still.jpg");
const VIDEO: &[u8] = include_bytes!("fixtures/clip.mp4");
const CAMERA: &str = "http://ns.google.com/photos/1.0/camera/";
fn xmp(photo: &[u8], text: &str) -> Vec<u8> {
    let mut out = vec![0xff, 0xd8, 0xff, 0xe1];
    let header = b"http://ns.adobe.com/xap/1.0/\0";
    out.extend_from_slice(&((text.len() + header.len() + 2) as u16).to_be_bytes());
    out.extend_from_slice(header);
    out.extend_from_slice(text.as_bytes());
    out.extend_from_slice(&photo[2..]);
    out
}

#[test]
fn round_trip_preserves_video_and_photo_pixels() {
    let merged = motion::compose(PHOTO, VIDEO).unwrap();
    let split = motion::split(&merged).unwrap().unwrap();
    assert_eq!(split.video, VIDEO);
    assert!(motion::split(&split.photo).unwrap().is_none());
    assert_eq!(
        image::load_from_memory(&split.photo).unwrap().to_rgb8(),
        image::load_from_memory(PHOTO).unwrap().to_rgb8()
    );
    let second = motion::compose(&split.photo, &split.video).unwrap();
    assert_eq!(motion::split(&second).unwrap().unwrap().video, VIDEO);
}

#[test]
fn preserves_unrelated_xmp_and_exif() {
    let text = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><r:RDF xmlns:r="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><r:Description xmlns:dc="http://purl.org/dc/elements/1.1/" dc:description="Travel &amp; Sea"/></r:RDF></x:xmpmeta>"#;
    let mut original = xmp(PHOTO, text);
    let exif = b"Exif\0\0test\xff\xd9thumbnail";
    let mut segment = vec![0xff, 0xe1];
    segment.extend_from_slice(&((exif.len() + 2) as u16).to_be_bytes());
    segment.extend_from_slice(exif);
    original.splice(2..2, segment);
    let combined = motion::compose(&original, VIDEO).unwrap();
    let split = motion::split(&combined).unwrap().unwrap();
    assert!(split.photo.windows(exif.len()).any(|w| w == exif));
    assert!(String::from_utf8_lossy(&split.photo).contains("Travel &amp; Sea"));
}

#[test]
fn legacy_namespace_alias_and_element_properties() {
    for property in [
        format!(
            r#"<g:MicroVideo>1</g:MicroVideo><g:MicroVideoOffset>{}</g:MicroVideoOffset>"#,
            VIDEO.len()
        ),
        String::new(),
    ] {
        let attr = if property.is_empty() {
            format!(r#"g:MicroVideo="1" g:MicroVideoOffset="{}""#, VIDEO.len())
        } else {
            String::new()
        };
        let mut bytes = xmp(
            PHOTO,
            &format!(
                r#"<r:RDF xmlns:r="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><r:Description xmlns:g="{CAMERA}" {attr}>{property}</r:Description></r:RDF>"#
            ),
        );
        bytes.extend_from_slice(VIDEO);
        let split = motion::split(&bytes).unwrap().unwrap();
        assert_eq!(split.video, VIDEO);
        assert!(motion::split(&split.photo).unwrap().is_none());
    }
}

#[test]
fn modern_directory_works_without_legacy_markers() {
    let bytes = motion::compose(PHOTO, VIDEO).unwrap();
    let mut bytes = bytes;
    let flag = b"Camera:MicroVideo=\"1\"";
    let pos = bytes.windows(flag.len()).position(|s| s == flag).unwrap();
    bytes[pos..pos + flag.len()].fill(b' ');
    assert_eq!(motion::split(&bytes).unwrap().unwrap().video, VIDEO);
}

#[test]
fn rejects_truncation_overflow_and_fake_video() {
    let mut combined = motion::compose(PHOTO, VIDEO).unwrap();
    combined.truncate(combined.len() - 50);
    assert!(motion::split(&combined).is_err());
    assert!(motion::compose(PHOTO, b"randomftypdata").is_err());
    assert!(motion::compose(&PHOTO[..PHOTO.len() - 2], VIDEO).is_err());
    let bytes = xmp(
        PHOTO,
        &format!(
            r#"<x xmlns:g="{CAMERA}" g:MicroVideo="1" g:MicroVideoOffset="18446744073709551616000"/>"#
        ),
    );
    assert!(motion::split(&bytes).is_err());
}

#[test]
fn disabled_motion_is_not_extracted() {
    let mut bytes = xmp(
        PHOTO,
        &format!(
            r#"<x xmlns:g="{CAMERA}" g:MotionPhoto="0" g:MicroVideo="1" g:MicroVideoOffset="{}"/>"#,
            VIDEO.len()
        ),
    );
    bytes.extend_from_slice(VIDEO);
    assert!(motion::split(&bytes).unwrap().is_none());
}

#[test]
fn import_order_export_names_and_source_integrity() {
    let dir = tempfile::tempdir().unwrap();
    let photo = dir.path().join("Trip.jpg");
    let video = dir.path().join("Trip.mp4");
    fs::write(&photo, PHOTO).unwrap();
    fs::write(&video, VIDEO).unwrap();
    let first = workspace::import(workspace::Session::default(), vec![video.clone()]).unwrap();
    assert!(first.output.is_none());
    let second = workspace::import(first, vec![photo.clone()]).unwrap();
    let output = second.output.as_ref().unwrap();
    assert_eq!(output.name, "Trip.MP.jpg");
    let split =
        workspace::import(workspace::Session::default(), vec![output.path.clone()]).unwrap();
    assert!(split.split);
    assert_eq!(&**split.video.unwrap().bytes, VIDEO);
    assert_eq!(fs::read(photo).unwrap(), PHOTO);
    assert_eq!(fs::read(video).unwrap(), VIDEO);
}

#[test]
fn rejects_ambiguous_batches() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("still.jpg");
    fs::write(&path, PHOTO).unwrap();
    assert!(workspace::import(workspace::Session::default(), vec![path.clone(), path]).is_err());
    assert!(workspace::import(workspace::Session::default(), vec![dir.path().to_owned()]).is_err());
}

#[test]
fn arbitrary_truncations_never_panic() {
    let merged = motion::compose(PHOTO, VIDEO).unwrap();
    for end in (0..merged.len()).step_by(31) {
        let _ = motion::split(&merged[..end]);
    }
}

#[test]
fn all_four_input_orders_produce_the_same_output() {
    let dir = tempfile::tempdir().unwrap();
    let photo = dir.path().join("Unordered.jpg");
    let video = dir.path().join("Video.mp4");
    fs::write(&photo, PHOTO).unwrap();
    fs::write(&video, VIDEO).unwrap();
    let mut results = vec![];
    for paths in [
        vec![photo.clone(), video.clone()],
        vec![video.clone(), photo.clone()],
    ] {
        let both = workspace::import(workspace::Session::default(), paths.clone()).unwrap();
        let first =
            workspace::import(workspace::Session::default(), vec![paths[0].clone()]).unwrap();
        let separate = workspace::import(first, vec![paths[1].clone()]).unwrap();
        for session in [both, separate] {
            let result = session.output.as_ref().unwrap();
            assert_eq!(result.name, "Unordered.MP.jpg");
            results.push(result.bytes.clone());
        }
    }
    assert!(results.windows(2).all(|pair| pair[0] == pair[1]));
}

fn fixture_pair(dir: &std::path::Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let photo = dir.join("photo.jpg");
    let video = dir.join("video.mp4");
    fs::write(&photo, PHOTO).unwrap();
    fs::write(&video, VIDEO).unwrap();
    (photo, video)
}

#[test]
fn reversing_reuses_the_result_and_retains_both_components() {
    let dir = tempfile::tempdir().unwrap();
    let (photo, video) = fixture_pair(dir.path());
    let composed = workspace::import(Default::default(), vec![photo, video]).unwrap();
    let original = composed.output.as_ref().unwrap().bytes.clone();
    let split = workspace::reverse(composed).unwrap();
    assert!(split.split);
    assert_eq!(split.motion.as_ref().unwrap().bytes, original);
    let reversed = workspace::reverse(split).unwrap();
    assert!(!reversed.split);
    assert_eq!(reversed.output.as_ref().unwrap().bytes, original);
    assert_eq!(reversed.video.as_ref().unwrap().bytes.as_slice(), VIDEO);
}

#[test]
fn replacing_a_split_component_keeps_the_other_component() {
    let dir = tempfile::tempdir().unwrap();
    let (photo, video) = fixture_pair(dir.path());
    let composed =
        workspace::import(Default::default(), vec![photo.clone(), video.clone()]).unwrap();
    let split = workspace::reverse(composed).unwrap();
    let original_photo = split.photo.as_ref().unwrap().bytes.clone();
    let original_video = split.video.as_ref().unwrap().bytes.clone();
    let updated_photo =
        workspace::import_into(split.clone(), vec![photo], workspace::DropTarget::Motion).unwrap();
    assert!(!updated_photo.split);
    assert_eq!(updated_photo.video.as_ref().unwrap().bytes, original_video);
    assert_eq!(
        updated_photo.photo.as_ref().unwrap().bytes.as_slice(),
        PHOTO
    );
    let updated_video =
        workspace::import_into(split, vec![video], workspace::DropTarget::Motion).unwrap();
    assert!(!updated_video.split);
    assert_eq!(updated_video.photo.as_ref().unwrap().bytes, original_photo);
    assert_eq!(
        updated_video.video.as_ref().unwrap().bytes.as_slice(),
        VIDEO
    );
}

#[test]
fn motionphoto_drop_replaces_only_the_targeted_input() {
    let dir = tempfile::tempdir().unwrap();
    let (photo, video) = fixture_pair(dir.path());
    let baseline = workspace::import(Default::default(), vec![photo, video]).unwrap();
    let mut changed_video = VIDEO.to_vec();
    // A legal additional free box changes the container without changing playback.
    changed_video.extend_from_slice(&[0, 0, 0, 8, b'f', b'r', b'e', b'e']);
    let changed_photo = xmp(
        PHOTO,
        r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><r:RDF xmlns:r="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><r:Description xmlns:dc="http://purl.org/dc/elements/1.1/" dc:description="Alternate"/></r:RDF></x:xmpmeta>"#,
    );
    let incoming = dir.path().join("alternate.MP.jpg");
    fs::write(
        &incoming,
        motion::compose(&changed_photo, &changed_video).unwrap(),
    )
    .unwrap();
    let photo_only = workspace::import_into(
        baseline.clone(),
        vec![incoming.clone()],
        workspace::DropTarget::Photo,
    )
    .unwrap();
    assert!(!photo_only.split);
    assert_eq!(
        photo_only.video.as_ref().unwrap().bytes,
        baseline.video.as_ref().unwrap().bytes
    );
    assert!(
        String::from_utf8_lossy(&photo_only.photo.as_ref().unwrap().bytes).contains("Alternate")
    );
    let video_only = workspace::import_into(
        baseline.clone(),
        vec![incoming],
        workspace::DropTarget::Video,
    )
    .unwrap();
    assert_eq!(
        video_only.photo.as_ref().unwrap().bytes,
        baseline.photo.as_ref().unwrap().bytes
    );
    assert_eq!(
        video_only.video.as_ref().unwrap().bytes.as_slice(),
        changed_video
    );
}

#[test]
fn plain_media_type_takes_priority_over_the_drop_slot() {
    let dir = tempfile::tempdir().unwrap();
    let (photo, video) = fixture_pair(dir.path());
    let first = workspace::import_into(
        Default::default(),
        vec![video],
        workspace::DropTarget::Photo,
    )
    .unwrap();
    assert!(first.video.is_some() && first.photo.is_none());
    let second = workspace::import_into(first, vec![photo], workspace::DropTarget::Video).unwrap();
    assert!(second.output.is_some());
}

#[test]
fn source_dimensions_duration_and_thumbnails_are_available() {
    let dir = tempfile::tempdir().unwrap();
    let (photo, video) = fixture_pair(dir.path());
    let session = workspace::import(Default::default(), vec![photo, video]).unwrap();
    let photo = session.photo.as_ref().unwrap();
    let info = photo.info.as_ref().unwrap();
    assert_eq!((info.width, info.height), (320, 240));
    assert!(info.duration.is_none());
    let video = session.video.as_ref().unwrap();
    let info = video.info.as_ref().unwrap();
    assert_eq!((info.width, info.height), (320, 240));
    assert!((info.duration.unwrap() - 1.0).abs() < 0.02);
    assert!(photo.preview.as_ref().unwrap().is_file());
    assert!(video.preview.as_ref().unwrap().is_file());
}

#[test]
fn empty_flow_can_reverse_and_incomplete_flow_is_preserved() {
    let empty = workspace::reverse(Default::default()).unwrap();
    assert!(empty.split);
    assert!(!workspace::reverse(empty).unwrap().split);
    let dir = tempfile::tempdir().unwrap();
    let (photo, _) = fixture_pair(dir.path());
    let partial = workspace::import(Default::default(), vec![photo]).unwrap();
    assert!(!partial.can_reverse());
    assert!(workspace::reverse(partial.clone()).is_err());
    assert!(partial.photo.as_ref().unwrap().path.exists());
}

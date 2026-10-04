use core_video::pixel_buffer::CVPixelBuffer;
use gpui::{
    Bounds, Corners, DevicePixels, IntoElement, ObjectFit, Pixels, RenderImage, Size, canvas,
    prelude::*, size,
};
use std::sync::Arc;

pub enum Media {
    Photo(Arc<RenderImage>),
    Video(CVPixelBuffer),
}

/// Both media types use the same measured viewport and fitting calculation.
/// Image intrinsic dimensions must never participate in viewport layout.
pub fn media_canvas(media: Media) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |viewport, _, window, _| match media {
            Media::Photo(image) => {
                let bounds = fit_inside(viewport, image.size(0));
                let _ = window.paint_image(viewport, bounds, Corners::default(), image, 0, false);
            }
            Media::Video(frame) => {
                let dimensions = size(frame.get_width().into(), frame.get_height().into());
                window.paint_surface(fit_inside(viewport, dimensions), frame);
            }
        },
    )
    .absolute()
    .inset_0()
    .size_full()
}

fn fit_inside(viewport: Bounds<Pixels>, dimensions: Size<DevicePixels>) -> Bounds<Pixels> {
    ObjectFit::Contain.get_bounds(viewport, dimensions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{point, px};

    #[test]
    fn portrait_is_centered_without_cropping_in_a_wide_viewport() {
        let viewport = Bounds::new(point(px(10.), px(20.)), size(px(800.), px(450.)));
        let fitted = fit_inside(viewport, size(600.into(), 800.into()));
        assert_eq!(fitted.size, size(px(337.5), px(450.)));
        assert_eq!(fitted.origin, point(px(241.25), px(20.)));
    }

    #[test]
    fn landscape_is_centered_without_cropping_in_a_tall_viewport() {
        let viewport = Bounds::new(point(px(0.), px(0.)), size(px(400.), px(600.)));
        let fitted = fit_inside(viewport, size(1600.into(), 900.into()));
        assert_eq!(fitted.size, size(px(400.), px(225.)));
        assert_eq!(fitted.origin, point(px(0.), px(187.5)));
    }
}

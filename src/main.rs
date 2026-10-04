mod player;
mod preview;

use core_video::pixel_buffer::CVPixelBuffer;
use gpui::{div, img, prelude::*, px, rgb, size, *};
use motiondrop::workspace::{self, Asset, DropTarget, Session};
use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

const BLACK: u32 = 0x090a0b;
const PANEL: u32 = 0x131517;
const LINE: u32 = 0x272a2d;
const MUTED: u32 = 0x858b90;
const WHITE: u32 = 0xf3f4f5;
const ACCENT: u32 = 0xc8e2cc;
actions!(motiondrop, [Quit, Choose, Reset, Replay, Reverse]);

enum Work {
    Import(Vec<PathBuf>, DropTarget),
    Reverse,
}

struct Hint(String);

impl Render for Hint {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .py_2()
            .rounded_md()
            .bg(rgb(PANEL))
            .border_1()
            .border_color(rgb(LINE))
            .text_color(rgb(WHITE))
            .text_xs()
            .child(self.0.clone())
    }
}

struct MotionDrop {
    focus: FocusHandle,
    session: Session,
    busy: bool,
    error: Option<String>,
    generation: u64,
    player: Option<player::Player>,
    photo_preview: Option<Arc<RenderImage>>,
    frame: Option<CVPixelBuffer>,
    playing: bool,
    started: Instant,
}

#[derive(Clone)]
struct Export {
    path: PathBuf,
    name: String,
}

impl Render for Export {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .bg(rgb(PANEL))
            .text_color(rgb(WHITE))
            .border_1()
            .border_color(rgb(LINE))
            .px_4()
            .py_3()
            .rounded_xl()
            .shadow_lg()
            .child(self.name.clone())
    }
}

impl MotionDrop {
    fn load(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        self.process(Work::Import(paths, DropTarget::Workspace), cx);
    }

    fn reverse(&mut self, _: &Reverse, _: &mut Window, cx: &mut Context<Self>) {
        if self.session.can_reverse() {
            self.process(Work::Reverse, cx);
        }
    }

    fn process(&mut self, work: Work, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.error = None;
        self.generation += 1;
        let generation = self.generation;
        let session = self.session.clone();
        let job = cx.background_executor().spawn(async move {
            let session = match work {
                Work::Import(paths, target) => workspace::import_into(session, paths, target)?,
                Work::Reverse => workspace::reverse(session)?,
            };
            let photo_preview = session
                .photo
                .as_ref()
                .and_then(|asset| asset.preview.as_ref())
                .and_then(|path| image::open(path).ok())
                .map(|image| {
                    let mut pixels = image.to_rgba8();
                    for pixel in pixels.pixels_mut() {
                        pixel.0.swap(0, 2);
                    }
                    Arc::new(RenderImage::new(vec![image::Frame::new(pixels)]))
                });
            Ok::<_, anyhow::Error>((session, photo_preview))
        });
        cx.spawn(async move |this, cx| {
            let result = job.await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.busy = false;
                match result {
                    Ok((session, photo_preview)) => {
                        this.photo_preview = photo_preview;
                        this.player = None;
                        this.frame = None;
                        this.playing = false;
                        this.session = session;
                        if let Some(video) = &this.session.video {
                            this.player = player::Player::new(&video.path);
                            this.playing = this.player.is_some();
                            this.started = Instant::now();
                            if !this.playing {
                                this.error = Some("Preview is unavailable. You can still drag the files out to save them.".into());
                            }
                        }
                    }
                    Err(error) => this.error = Some(format!("{error:#}")),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn choose(&mut self, _: &Choose, _: &mut Window, cx: &mut Context<Self>) {
        self.choose_for(DropTarget::Workspace, cx);
    }

    fn choose_for(&mut self, target: DropTarget, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let prompt = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Choose photos or videos".into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = prompt.await {
                let _ = this.update(cx, |this, cx| this.process(Work::Import(paths, target), cx));
            }
        })
        .detach();
    }

    fn reset(&mut self, _: &Reset, _: &mut Window, cx: &mut Context<Self>) {
        self.generation += 1;
        self.busy = false;
        self.player = None;
        self.photo_preview = None;
        self.frame = None;
        self.playing = false;
        let storage = std::mem::take(&mut self.session.storage);
        self.session = Session {
            storage,
            ..Default::default()
        };
        self.error = None;
        cx.notify();
    }

    fn replay(&mut self, _: &Replay, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(player) = &self.player {
            if self.playing {
                player.stop();
                self.playing = false;
                self.frame = None;
            } else {
                player.replay();
                self.playing = true;
                self.started = Instant::now();
            }
            cx.notify();
        }
    }

    fn poll_player(&mut self, window: &mut Window) {
        if self.playing
            && let Some(player) = &self.player
        {
            let status = player.status();
            if status == -1 || (status == 0 && self.started.elapsed() > Duration::from_secs(15)) {
                player.stop();
                self.playing = false;
                self.frame = None;
                self.error = Some(
                    "This video could not be played. You can still drag it out to save it.".into(),
                );
            } else if status == 2 {
                self.playing = false;
                // A MotionPhoto always returns to its full-resolution still image.
                if self.session.photo.is_some() {
                    self.frame = None;
                }
            } else {
                if let Some(frame) = player.frame() {
                    self.frame = Some(frame);
                }
                window.request_animation_frame();
            }
        }
    }

    fn export(&self, asset: &Asset, kind: &'static str, symbol: &'static str) -> AnyElement {
        let file = Export {
            path: asset.path.clone(),
            name: asset.name.clone(),
        };
        let path = asset.path.clone();
        let name = asset.name.clone();
        tile_body(Some(asset), kind, symbol, true)
            .id(SharedString::from(
                asset.path.to_string_lossy().into_owned(),
            ))
            .cursor_grab()
            .hover(|s| s.border_color(rgb(0x66796b)))
            .tooltip(move |_, cx| cx.new(|_| Hint(name.clone())).into())
            .on_drag(file, |value, _, _, cx| cx.new(|_| value.clone()))
            .external_drag_payload(|file: &Export, _, _| {
                Some(ExternalDragPayload::Files(FileDragPaths::new([(
                    file.path.clone(),
                    false,
                )])))
            })
            .on_click(move |event, _, cx| {
                if event.click_count() == 2 {
                    cx.open_with_system(&path);
                }
            })
            .into_any_element()
    }

    fn input(
        &self,
        asset: Option<&Asset>,
        label: &'static str,
        symbol: &'static str,
        target: DropTarget,
        cx: &Context<Self>,
    ) -> AnyElement {
        let hint = asset
            .map(|asset| asset.name.clone())
            .unwrap_or_else(|| "Drop or choose a file".into());
        tile_body(asset, label, symbol, false)
            .id(label)
            .cursor_pointer()
            .hover(|s| s.border_color(rgb(0x596366)))
            .drag_over::<ExternalPaths>(|s, _, _, _| s.border_color(rgb(ACCENT)).bg(rgb(0x202923)))
            .tooltip(move |_, cx| cx.new(|_| Hint(hint.clone())).into())
            .on_click(cx.listener(move |this, _, _, cx| this.choose_for(target, cx)))
            .on_drop(cx.listener(move |this, paths: &ExternalPaths, _, cx| {
                cx.stop_propagation();
                this.process(Work::Import(paths.paths().to_vec(), target), cx);
            }))
            .into_any_element()
    }
}

impl Render for MotionDrop {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.poll_player(window);
        let has_content = self.session.photo.is_some() || self.session.video.is_some();
        let media = self
            .frame
            .clone()
            .map(preview::Media::Video)
            .or_else(|| self.photo_preview.clone().map(preview::Media::Photo));
        let title = "Drop your files";
        let output = |asset: Option<&Asset>, label, symbol| {
            asset
                .map(|asset| self.export(asset, label, symbol))
                .unwrap_or_else(|| tile_body(None, label, symbol, true).into_any_element())
        };
        let can_reverse = self.session.can_reverse() && !self.busy;
        let reverse = div()
            .id("reverse")
            .size(px(32.))
            .flex_shrink_0()
            .rounded_full()
            .flex()
            .items_center()
            .justify_center()
            .text_color(rgb(if can_reverse { ACCENT } else { MUTED }))
            .tooltip(move |_, cx| {
                cx.new(|_| {
                    Hint(
                        if can_reverse {
                            "Reverse flow"
                        } else {
                            "Add both files to reverse"
                        }
                        .into(),
                    )
                })
                .into()
            })
            .when(can_reverse, |d| {
                d.cursor_pointer()
                    .hover(|s| s.bg(rgb(PANEL)))
                    .on_click(cx.listener(|this, _, window, cx| this.reverse(&Reverse, window, cx)))
            })
            .child("⇄");
        let flow = div().flex_1().min_w_0().flex().items_center().gap_3();
        let flow = if self.session.split {
            flow.child(self.input(
                self.session.motion.as_ref(),
                "MotionPhoto",
                "◉",
                DropTarget::Motion,
                cx,
            ))
            .child(reverse)
            .child(output(self.session.photo.as_ref(), "Photo", "▧"))
            .child(flow_symbol("+"))
            .child(output(self.session.video.as_ref(), "Video", "▷"))
        } else {
            flow.child(self.input(
                self.session.photo.as_ref(),
                "Photo",
                "▧",
                DropTarget::Photo,
                cx,
            ))
            .child(flow_symbol("+"))
            .child(self.input(
                self.session.video.as_ref(),
                "Video",
                "▷",
                DropTarget::Video,
                cx,
            ))
            .child(reverse)
            .child(output(self.session.output.as_ref(), "MotionPhoto", "◉"))
        };
        let preview = div()
            .id("preview")
            .relative()
            .flex_1()
            .min_h_0()
            .w_full()
            .overflow_hidden()
            .bg(rgb(0x000000))
            .when_some(media, |d, media| d.child(preview::media_canvas(media)))
            .child(
                div()
                    .id("window-drag")
                    .absolute()
                    .top_0()
                    .left(px(86.))
                    .right_0()
                    .h(px(44.))
                    .window_control_area(WindowControlArea::Drag)
                    .on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move()),
            )
            .when_some(self.error.clone(), |d, error| {
                d.child(
                    div()
                        .absolute()
                        .top_4()
                        .left_4()
                        .right_4()
                        .px_4()
                        .py_3()
                        .rounded_lg()
                        .bg(rgba(0x30201bee))
                        .text_color(rgb(0xefbca4))
                        .text_sm()
                        .child(error),
                )
            })
            .when(self.busy, |d| {
                d.child(
                    div()
                        .absolute()
                        .top_4()
                        .left_4()
                        .px_3()
                        .py_2()
                        .rounded_lg()
                        .bg(rgb(PANEL))
                        .text_color(rgb(MUTED))
                        .text_sm()
                        .child("Processing…"),
                )
            })
            .when(self.player.is_some(), |d| {
                d.child(
                    div().absolute().bottom_5().right_5().child(
                        div()
                            .id("replay")
                            .flex()
                            .items_center()
                            .gap_2()
                            .size(px(36.))
                            .justify_center()
                            .rounded_full()
                            .bg(rgba(0x181b1ee8))
                            .border_1()
                            .border_color(rgba(0xffffff25))
                            .text_color(rgb(WHITE))
                            .text_sm()
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(0x343a3e)))
                            .on_click(
                                cx.listener(|this, _, window, cx| this.replay(&Replay, window, cx)),
                            )
                            .tooltip({
                                let playing = self.playing;
                                move |_, cx| {
                                    cx.new(|_| {
                                        Hint(if playing { "Show photo" } else { "Replay" }.into())
                                    })
                                    .into()
                                }
                            })
                            .child(if self.playing { "Ⅱ" } else { "▶" }),
                    ),
                )
            })
            .when(!has_content, |d| {
                d.child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .flex_col()
                        .justify_center()
                        .items_center()
                        .gap_5()
                        .child(
                            div()
                                .relative()
                                .w(px(176.))
                                .h(px(106.))
                                .child(
                                    div()
                                        .absolute()
                                        .left_0()
                                        .top_0()
                                        .w(px(104.))
                                        .h(px(80.))
                                        .rounded_xl()
                                        .border_1()
                                        .border_color(rgb(0x41494a))
                                        .bg(rgb(0x151a1b))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .text_size(px(36.))
                                        .text_color(rgb(0x9aa8a5))
                                        .child("▧"),
                                )
                                .child(
                                    div()
                                        .absolute()
                                        .right_0()
                                        .bottom_0()
                                        .w(px(104.))
                                        .h(px(80.))
                                        .rounded_xl()
                                        .border_1()
                                        .border_color(rgb(0x667969))
                                        .bg(rgb(0x242f28))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .text_size(px(32.))
                                        .text_color(rgb(ACCENT))
                                        .child("▷"),
                                ),
                        )
                        .child(
                            div()
                                .text_size(px(25.))
                                .font_weight(FontWeight::MEDIUM)
                                .child(title),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(rgb(MUTED))
                                .child("A MotionPhoto, or a photo and a video"),
                        )
                        .child(
                            div()
                                .id("choose")
                                .mt_3()
                                .px_5()
                                .py_2()
                                .rounded_full()
                                .bg(rgb(0x202426))
                                .border_1()
                                .border_color(rgb(0x363d40))
                                .text_sm()
                                .cursor_pointer()
                                .hover(|s| s.bg(rgb(0x303638)))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.choose(&Choose, window, cx)
                                }))
                                .child("Choose files  ↗"),
                        ),
                )
            });
        div()
            .id("motiondrop")
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(BLACK))
            .text_color(rgb(WHITE))
            .font_family(".AppleSystemUIFont")
            .on_action(cx.listener(Self::choose))
            .on_action(cx.listener(Self::reset))
            .on_action(cx.listener(Self::replay))
            .on_action(cx.listener(Self::reverse))
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                this.load(paths.paths().to_vec(), cx)
            }))
            .drag_over::<ExternalPaths>(|s, _, _, _| s.border_2().border_color(rgb(ACCENT)))
            .child(preview)
            .child(
                div()
                    .h(px(124.))
                    .flex_shrink_0()
                    .px_5()
                    .flex()
                    .items_center()
                    .gap_3()
                    .border_t_1()
                    .border_color(rgb(LINE))
                    .child(flow)
                    .child(
                        div()
                            .w(px(44.))
                            .flex_shrink_0()
                            .flex()
                            .justify_center()
                            .when(has_content, |d| {
                                d.child(
                                    div()
                                        .id("clear-files")
                                        .text_xs()
                                        .py_3()
                                        .text_color(rgb(MUTED))
                                        .cursor_pointer()
                                        .hover(|s| s.text_color(rgb(WHITE)))
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.reset(&Reset, window, cx)
                                        }))
                                        .child("Clear"),
                                )
                            }),
                    ),
            )
    }
}

fn tile_body(
    asset: Option<&Asset>,
    label: &'static str,
    symbol: &'static str,
    output: bool,
) -> Div {
    let preview = asset.and_then(|asset| asset.preview.clone());
    let resolution = asset
        .and_then(|asset| asset.info.as_ref())
        .map(|info| format!("{} × {}", info.width, info.height));
    let duration = asset
        .and_then(|asset| asset.info.as_ref())
        .and_then(|info| info.duration)
        .map(|seconds| format!("{seconds:.1} s"));
    div()
        .flex_1()
        .min_w_0()
        .h(px(88.))
        .flex()
        .items_center()
        .gap_3()
        .px_3()
        .rounded_xl()
        .border_1()
        .border_color(rgb(if output && asset.is_some() {
            0x3d5144
        } else {
            LINE
        }))
        .bg(rgb(if output && asset.is_some() {
            PANEL
        } else {
            BLACK
        }))
        .child(
            div()
                .w(px(50.))
                .h(px(66.))
                .flex_shrink_0()
                .rounded_md()
                .overflow_hidden()
                .bg(rgb(0x000000))
                .flex()
                .items_center()
                .justify_center()
                .when_some(preview.clone(), |d, path| {
                    d.child(
                        img(Arc::<std::path::Path>::from(path))
                            .w(px(50.))
                            .h(px(66.))
                            .aspect_ratio(50. / 66.)
                            .object_fit(ObjectFit::Contain),
                    )
                })
                .when(preview.is_none(), |d| {
                    d.text_xl().text_color(rgb(MUTED)).child(symbol)
                }),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(if asset.is_some() { WHITE } else { MUTED }))
                        .child(label),
                )
                .when_some(resolution, |d, value| {
                    d.child(div().text_xs().text_color(rgb(MUTED)).child(value))
                })
                .when_some(duration, |d, value| {
                    d.child(div().text_xs().text_color(rgb(MUTED)).child(value))
                }),
        )
        .when(output && asset.is_some(), |d| {
            d.child(div().text_xs().text_color(rgb(ACCENT)).child("↗"))
        })
}

fn flow_symbol(symbol: &'static str) -> impl IntoElement {
    div()
        .flex_shrink_0()
        .text_sm()
        .text_color(rgb(MUTED))
        .child(symbol)
}

fn main() {
    let paths: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    gpui_platform::application().run(move |cx: &mut App| {
        player::dark_appearance();
        cx.bind_keys([
            KeyBinding::new("cmd-q", Quit, None),
            KeyBinding::new("cmd-o", Choose, None),
            KeyBinding::new("cmd-r", Reset, None),
            KeyBinding::new("space", Replay, None),
        ]);
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.set_menus(vec![Menu {
            name: "MotionDrop".into(),
            disabled: false,
            items: vec![
                MenuItem::action("Choose files…", Choose),
                MenuItem::action("Play / Show still", Replay),
                MenuItem::action("Clear files", Reset),
                MenuItem::separator(),
                MenuItem::action("Quit MotionDrop", Quit),
            ],
        }]);
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        let bounds = Bounds::centered(None, size(px(860.), px(690.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(640.), px(540.))),
                titlebar: Some(TitlebarOptions {
                    title: Some("MotionDrop".into()),
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(18.), px(21.))),
                }),
                app_owns_titlebar_drag: true,
                ..Default::default()
            },
            |window, cx| {
                cx.new(|cx| {
                    let focus = cx.focus_handle();
                    focus.focus(window, cx);
                    let mut view = MotionDrop {
                        focus,
                        session: Session::default(),
                        busy: false,
                        error: None,
                        generation: 0,
                        player: None,
                        photo_preview: None,
                        frame: None,
                        playing: false,
                        started: Instant::now(),
                    };
                    if !paths.is_empty() {
                        view.load(paths, cx);
                    }
                    view
                })
            },
        )
        .expect("Could not create the window");
        cx.activate(true);
    });
}

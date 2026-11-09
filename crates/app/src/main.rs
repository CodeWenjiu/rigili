gpuili_macro::mod_prv!(backend, format, home, playback, platform, shell);

use gpui_kit::App;

use crate::platform::assets::AppAssets;
use crate::playback::PlaybackRequest;

enum Launch {
    Browse,
    Play(PlaybackRequest),
    PlayWithBrowser(PlaybackRequest),
}

fn main() {
    let launch = parse_args();

    gpui_kit::application()
        .with_assets(AppAssets)
        .run(move |cx| {
            gpui_kit::init(cx);
            platform::init(cx);

            if let Err(error) = start(launch, cx) {
                eprintln!("failed to open window: {error}");
            }
        });
}

fn parse_args() -> Launch {
    let mut args = std::env::args().skip(1);
    let first = args.next();
    let second = args.next();

    match first {
        Some(bvid) if bvid.starts_with("BV") => {
            Launch::PlayWithBrowser(PlaybackRequest::video(bvid.clone(), bvid, None))
        }
        Some(video) => Launch::Play(PlaybackRequest::stream(video_title(&video), video, second)),
        None => Launch::Browse,
    }
}

fn start(launch: Launch, cx: &mut App) -> gpui_kit::Result<()> {
    match launch {
        Launch::Browse => shell::open_main(None, cx),
        Launch::Play(request) => playback::open(request, None, cx).map(|_| ()),
        Launch::PlayWithBrowser(request) => shell::open_main(Some(request), cx),
    }
}

fn video_title(video: &str) -> String {
    if video.contains("://") {
        return "播放器".into();
    }
    std::path::Path::new(video)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "播放器".into())
}

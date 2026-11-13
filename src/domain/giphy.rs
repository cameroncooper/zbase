use crate::domain::message::LinkPreview;
use reqwest::Url;

/// The media preview for a direct GIPHY media link, such as the
/// `https://media3.giphy.com/media/.../giphy.mp4#height=360&width=480&isvideo=true`
/// links the Keybase `/giphy` command sends. The page behind such a link is
/// GIPHY's generic HTML, so its Open Graph card shows site boilerplate; this
/// shows the GIF itself. The GIF rather than the MP4, because images animate
/// and video previews show a single frame.
pub fn giphy_media_preview(url: &str) -> Option<LinkPreview> {
    let parsed = Url::parse(url.trim()).ok()?;
    let host = parsed.host_str()?.to_ascii_lowercase();
    let is_media_host = host == "i.giphy.com"
        || host
            .strip_suffix(".giphy.com")
            .is_some_and(|sub| sub.starts_with("media"));
    if !is_media_host {
        return None;
    }

    let file_name = parsed.path_segments()?.next_back()?;
    let (stem, extension) = file_name.rsplit_once('.')?;
    if stem.is_empty()
        || !["gif", "mp4", "webp"]
            .iter()
            .any(|known| extension.eq_ignore_ascii_case(known))
    {
        return None;
    }

    let mut gif = parsed.clone();
    gif.set_fragment(None);
    gif.path_segments_mut()
        .ok()?
        .pop()
        .push(&format!("{stem}.gif"));

    let (width, height) = fragment_dimensions(&parsed);
    Some(LinkPreview {
        url: url.to_string(),
        site: Some("giphy.com".to_string()),
        thumbnail_asset: Some(gif.to_string()),
        is_media: true,
        media_width: width,
        media_height: height,
        ..LinkPreview::default()
    })
}

/// Keybase appends `#height=…&width=…` to GIPHY links.
fn fragment_dimensions(url: &Url) -> (Option<u32>, Option<u32>) {
    let (mut width, mut height) = (None, None);
    for pair in url.fragment().unwrap_or_default().split('&') {
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        let value = value.parse::<u32>().ok().filter(|value| *value > 0);
        match key {
            "width" => width = value,
            "height" => height = value,
            _ => {}
        }
    }
    (width, height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keybase_giphy_mp4_link_becomes_its_gif() {
        let url = "https://media3.giphy.com/media/v1.Y2lk/iKBAAfYNDu1dowhnEj/giphy.mp4#height=360&width=480&isvideo=true";
        let preview = giphy_media_preview(url).expect("giphy media link");
        assert_eq!(preview.url, url);
        assert_eq!(
            preview.thumbnail_asset.as_deref(),
            Some("https://media3.giphy.com/media/v1.Y2lk/iKBAAfYNDu1dowhnEj/giphy.gif")
        );
        assert!(preview.is_media);
        assert!(!preview.is_video);
        assert_eq!(preview.media_width, Some(480));
        assert_eq!(preview.media_height, Some(360));
    }

    #[test]
    fn short_media_host_and_webp_map_to_gif() {
        let preview = giphy_media_preview("https://i.giphy.com/abc123.webp").expect("media");
        assert_eq!(
            preview.thumbnail_asset.as_deref(),
            Some("https://i.giphy.com/abc123.gif")
        );
        assert_eq!(preview.media_width, None);
    }

    #[test]
    fn giphy_pages_and_other_hosts_are_not_media() {
        assert!(giphy_media_preview("https://giphy.com/gifs/lets-go-abc123").is_none());
        assert!(giphy_media_preview("https://media.giphy.com/media/abc/").is_none());
        assert!(giphy_media_preview("https://example.com/giphy.mp4").is_none());
        assert!(giphy_media_preview("https://notgiphy.com/media/a/giphy.gif").is_none());
    }
}

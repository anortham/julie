use super::{extract_symbols, SymbolKind};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_multimedia_elements_svg_canvas_and_embedded_content() {
        let html_code = r###"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <title>Media and Embedded Content</title>
</head>
<body>
  <section class="gallery" aria-label="Photo gallery">
    <h2>Image Gallery</h2>

    <figure class="featured-image">
      <img
        src="/images/hero-image.jpg"
        alt="Beautiful sunset over mountains with vibrant orange and pink colors"
        width="800"
        height="600"
        loading="lazy"
        decoding="async"
        sizes="(max-width: 768px) 100vw, (max-width: 1200px) 50vw, 33vw"
        srcset="
          /images/hero-image-400.jpg 400w,
          /images/hero-image-800.jpg 800w,
          /images/hero-image-1200.jpg 1200w,
          /images/hero-image-1600.jpg 1600w
        "
      >
      <figcaption class="image-caption">
        Sunset over the Rocky Mountains -
        <cite>Photo by Jane Photographer</cite>
        <time datetime="2024-01-15">January 15, 2024</time>
      </figcaption>
    </figure>

    <div class="image-grid">
      <picture class="responsive-image">
        <source
          media="(min-width: 1200px)"
          srcset="/images/gallery-1-large.webp"
          type="image/webp"
        >
        <source
          media="(min-width: 768px)"
          srcset="/images/gallery-1-medium.webp"
          type="image/webp"
        >
        <source
          srcset="/images/gallery-1-small.webp"
          type="image/webp"
        >
        <img
          src="/images/gallery-1-medium.jpg"
          alt="Abstract art piece with geometric patterns"
          loading="lazy"
          decoding="async"
        >
      </picture>

      <img
        src="/images/gallery-2.jpg"
        alt="Modern architecture with glass and steel elements"
        width="400"
        height="300"
        loading="lazy"
        decoding="async"
      >

      <img
        src="data:image/svg+xml;base64,PHN2ZyB3aWR0aD0iMjAwIiBoZWlnaHQ9IjIwMCIgeG1sbnM9Imh0dHA6Ly93d3cudzMub3JnLzIwMDAvc3ZnIj4KICA8cmVjdCB3aWR0aD0iMTAwJSIgaGVpZ2h0PSIxMDAlIiBmaWxsPSIjZGRkIi8+CiAgPHRleHQgeD0iNTAlIiB5PSI1MCUiIGZvbnQtZmFtaWx5PSJBcmlhbCwgc2Fucy1zZXJpZiIgZm9udC1zaXplPSIxNnB4IiBmaWxsPSIjOTk5IiB0ZXh0LWFuY2hvcj0ibWlkZGxlIiBkeT0iLjNlbSI+UGxhY2Vob2xkZXI8L3RleHQ+Cjwvc3ZnPgo="
        alt="Placeholder image"
        width="200"
        height="200"
        loading="lazy"
      >
    </div>
  </section>

  <section class="video-section" aria-label="Video content">
    <h2>Video Content</h2>

    <div class="video-wrapper">
      <video
        id="main-video"
        class="main-video"
        width="800"
        height="450"
        controls
        preload="metadata"
        poster="/images/video-poster.jpg"
        aria-describedby="video-description"
      >
        <source src="/videos/demo.mp4" type="video/mp4">
        <source src="/videos/demo.webm" type="video/webm">
        <source src="/videos/demo.ogv" type="video/ogg">

        <track
          kind="subtitles"
          src="/videos/demo-en.vtt"
          srclang="en"
          label="English"
          default
        >
        <track
          kind="subtitles"
          src="/videos/demo-es.vtt"
          srclang="es"
          label="Español"
        >
        <track
          kind="captions"
          src="/videos/demo-captions.vtt"
          srclang="en"
          label="English Captions"
        >
        <track
          kind="descriptions"
          src="/videos/demo-descriptions.vtt"
          srclang="en"
          label="Audio Descriptions"
        >

        <p class="video-fallback">
          Your browser doesn't support HTML5 video.
          <a href="/videos/demo.mp4">Download the video</a> instead.
        </p>
      </video>

      <div id="video-description" class="video-description">
        Product demonstration showing key features and user interface walkthrough.
      </div>
    </div>
  </section>

  <section class="audio-section" aria-label="Audio content">
    <h2>Audio Content</h2>

    <div class="audio-player">
      <audio
        id="podcast-player"
        class="audio-element"
        preload="none"
        aria-describedby="audio-description"
      >
        <source src="/audio/podcast-episode-1.mp3" type="audio/mpeg">
        <source src="/audio/podcast-episode-1.ogg" type="audio/ogg">
        <source src="/audio/podcast-episode-1.wav" type="audio/wav">

        <p class="audio-fallback">
          Your browser doesn't support HTML5 audio.
          <a href="/audio/podcast-episode-1.mp3">Download the audio file</a> instead.
        </p>
      </audio>

      <div class="audio-info">
        <h3 class="audio-title">Tech Talk Episode 1: Web Accessibility</h3>
        <p id="audio-description" class="audio-description">
          In this episode, we discuss the importance of web accessibility and practical tips for developers.
        </p>
        <div class="audio-meta">
          <span class="duration">Duration: 45 minutes</span>
          <span class="file-size">Size: 32.5 MB</span>
        </div>
      </div>
    </div>
  </section>

  <section class="graphics-section" aria-label="Vector graphics">
    <h2>SVG Graphics</h2>

    <div class="svg-container">
      <svg
        width="300"
        height="200"
        viewBox="0 0 300 200"
        xmlns="http://www.w3.org/2000/svg"
        role="img"
        aria-labelledby="chart-title chart-desc"
      >
        <title id="chart-title">Sales Data Chart</title>
        <desc id="chart-desc">
          Bar chart showing quarterly sales data with values for Q1: 100, Q2: 150, Q3: 200, Q4: 175
        </desc>

        <defs>
          <linearGradient id="barGradient" x1="0%" y1="0%" x2="0%" y2="100%">
            <stop offset="0%" style="stop-color:#4285f4;stop-opacity:1" />
            <stop offset="100%" style="stop-color:#1a73e8;stop-opacity:1" />
          </linearGradient>
        </defs>

        <rect x="0" y="0" width="300" height="200" fill="#fafafa" stroke="#ddd" stroke-width="1"/>

        <rect x="40" y="120" width="40" height="60" fill="url(#barGradient)" aria-label="Q1: 100">
          <title>Q1: $100k</title>
        </rect>
        <rect x="100" y="95" width="40" height="85" fill="url(#barGradient)" aria-label="Q2: 150">
          <title>Q2: $150k</title>
        </rect>
        <rect x="160" y="70" width="40" height="110" fill="url(#barGradient)" aria-label="Q3: 200">
          <title>Q3: $200k</title>
        </rect>
        <rect x="220" y="82" width="40" height="98" fill="url(#barGradient)" aria-label="Q4: 175">
          <title>Q4: $175k</title>
        </rect>

        <text x="60" y="195" text-anchor="middle" font-family="Arial, sans-serif" font-size="12" fill="#666">Q1</text>
        <text x="120" y="195" text-anchor="middle" font-family="Arial, sans-serif" font-size="12" fill="#666">Q2</text>
        <text x="180" y="195" text-anchor="middle" font-family="Arial, sans-serif" font-size="12" fill="#666">Q3</text>
        <text x="240" y="195" text-anchor="middle" font-family="Arial, sans-serif" font-size="12" fill="#666">Q4</text>

        <circle cx="150" cy="50" r="20" fill="#ff6b6b" opacity="0.8">
          <animate attributeName="r" values="15;25;15" dur="2s" repeatCount="indefinite"/>
        </circle>
      </svg>

      <img src="/images/logo.svg" alt="Company logo" width="150" height="75" class="svg-logo">

      <object data="/images/infographic.svg" type="image/svg+xml" width="400" height="300" aria-label="Data infographic">
        <img src="/images/infographic-fallback.png" alt="Data infographic showing key statistics">
      </object>
    </div>
  </section>

  <section class="canvas-section" aria-label="Interactive graphics">
    <h2>Canvas Graphics</h2>

    <div class="canvas-container">
      <canvas
        id="interactive-chart"
        class="chart-canvas"
        width="600"
        height="400"
        role="img"
        aria-label="Interactive data visualization"
        aria-describedby="canvas-description"
      >
        <p id="canvas-description">
          Interactive chart showing real-time data. Canvas is not supported in your browser.
          <a href="/data.csv">Download the raw data</a> instead.
        </p>
      </canvas>
    </div>
  </section>

  <section class="embed-section" aria-label="Embedded content">
    <h2>Embedded Content</h2>

    <div class="video-embed">
      <iframe
        width="560"
        height="315"
        src="https://www.youtube-nocookie.com/embed/dQw4w9WgXcQ"
        title="Sample Video"
        frameborder="0"
        allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture"
        allowfullscreen
        loading="lazy"
        aria-describedby="embed-description"
      ></iframe>
      <div id="embed-description" class="embed-description">
        Educational video about web development best practices.
      </div>
    </div>

    <div class="map-embed">
      <iframe
        src="https://www.openstreetmap.org/export/embed.html?bbox=-0.004017949104309083%2C51.47612752641776%2C0.00030577182769775396%2C51.478569861898606&layer=mapnik"
        width="400"
        height="300"
        frameborder="0"
        title="Office location map"
        aria-label="Interactive map showing office location"
        loading="lazy"
      ></iframe>
    </div>

    <embed
      src="/documents/presentation.pdf"
      type="application/pdf"
      width="600"
      height="400"
      aria-label="Product presentation PDF"
    >
  </section>

  <section class="components-section" aria-label="Custom web components">
    <h2>Custom Elements</h2>

    <custom-video-player
      src="/videos/demo.mp4"
      poster="/images/video-poster.jpg"
      controls="true"
      autoplay="false"
      aria-label="Custom video player component"
    >
      <p slot="fallback">Video player not supported in your browser.</p>
    </custom-video-player>

    <data-visualization
      type="chart"
      data-source="/api/analytics"
      refresh-interval="30000"
      aria-label="Real-time analytics dashboard"
    ></data-visualization>

    <image-gallery
      images='[
        {"src": "/images/1.jpg", "alt": "Image 1", "caption": "First image"},
        {"src": "/images/2.jpg", "alt": "Image 2", "caption": "Second image"}
      ]'
      layout="grid"
      lazy-loading="true"
    ></image-gallery>
  </section>
</body>
</html>"###;
        let symbols = extract_symbols(html_code);

        // Image elements
        let hero_image = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .map_or(false, |sig| sig.contains(r#"src="/images/hero-image.jpg""#))
        });
        assert!(hero_image.is_some());
        assert_eq!(hero_image.unwrap().kind, SymbolKind::Variable); // Media elements as variables
        assert!(hero_image
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains(r#"loading="lazy""#));
        assert!(hero_image
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains(r#"decoding="async""#));
        assert!(hero_image
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("srcset="));
        assert!(hero_image
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("sizes="));

        let data_uri_image = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .map_or(false, |sig| sig.contains("data:image/svg+xml;base64"))
        });
        assert!(data_uri_image.is_some());

        // Figure and figcaption
        let figure_element = symbols.iter().find(|s| s.name == "figure");
        assert!(figure_element.is_some());

        let figcaption_element = symbols.iter().find(|s| s.name == "figcaption");
        assert!(figcaption_element.is_some());

        let cite_element = symbols.iter().find(|s| s.name == "cite");
        assert!(cite_element.is_some());

        // Picture and source elements
        let picture_element = symbols.iter().find(|s| s.name == "picture");
        assert!(picture_element.is_some());

        let source_elements: Vec<_> = symbols.iter().filter(|s| s.name == "source").collect();
        assert!(source_elements.len() >= 5); // Picture sources + video/audio sources

        let webp_source = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .map_or(false, |sig| sig.contains(r#"type="image/webp""#))
        });
        assert!(webp_source.is_some());

        let media_source = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .map_or(false, |sig| sig.contains(r#"media="(min-width: 1200px)""#))
        });
        assert!(media_source.is_some());

        // Video element
        let video_element = symbols.iter().find(|s| s.name == "video");
        assert!(video_element.is_some());
        assert!(video_element
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("controls"));
        assert!(video_element
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains(r#"preload="metadata""#));
        assert!(video_element
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains(r#"poster="/images/video-poster.jpg""#));

        // Track elements
        let track_elements: Vec<_> = symbols.iter().filter(|s| s.name == "track").collect();
        assert_eq!(track_elements.len(), 4);

        let subtitles_track = symbols.iter().find(|s| {
            s.signature.as_ref().map_or(false, |sig| {
                sig.contains(r#"kind="subtitles""#) && sig.contains(r#"srclang="en""#)
            })
        });
        assert!(subtitles_track.is_some());

        let captions_track = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .map_or(false, |sig| sig.contains(r#"kind="captions""#))
        });
        assert!(captions_track.is_some());

        let descriptions_track = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .map_or(false, |sig| sig.contains(r#"kind="descriptions""#))
        });
        assert!(descriptions_track.is_some());

        let default_track = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .map_or(false, |sig| sig.contains("default"))
                && s.name == "track"
        });
        assert!(default_track.is_some());

        // Audio element
        let audio_element = symbols.iter().find(|s| s.name == "audio");
        assert!(audio_element.is_some());
        assert!(audio_element
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains(r#"preload="none""#));

        let mp3_source = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .map_or(false, |sig| sig.contains(r#"type="audio/mpeg""#))
        });
        assert!(mp3_source.is_some());

        let ogg_source = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .map_or(false, |sig| sig.contains(r#"type="audio/ogg""#))
        });
        assert!(ogg_source.is_some());

        // SVG element
        let svg_element = symbols.iter().find(|s| s.name == "svg");
        assert!(svg_element.is_some());
        assert!(svg_element
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains(r#"role="img""#));
        assert!(svg_element
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains(r#"aria-labelledby="chart-title chart-desc""#));

        let title_element = symbols.iter().find(|s| {
            s.name == "title"
                && s.signature
                    .as_ref()
                    .map_or(false, |sig| sig.contains("Sales Data Chart"))
        });
        assert!(title_element.is_some());

        let desc_element = symbols.iter().find(|s| s.name == "desc");
        assert!(desc_element.is_some());

        // SVG shapes
        let rect_elements: Vec<_> = symbols.iter().filter(|s| s.name == "rect").collect();
        assert!(rect_elements.len() >= 5);

        let circle_element = symbols.iter().find(|s| s.name == "circle");
        assert!(circle_element.is_some());

        let text_elements: Vec<_> = symbols.iter().filter(|s| s.name == "text").collect();
        assert!(text_elements.len() >= 4);

        // SVG animation
        let animate_element = symbols.iter().find(|s| s.name == "animate");
        assert!(animate_element.is_some());
        assert!(animate_element
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains(r#"attributeName="r""#));
        assert!(animate_element
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains(r#"repeatCount="indefinite""#));

        // Object element
        let object_element = symbols.iter().find(|s| s.name == "object");
        assert!(object_element.is_some());
        assert!(object_element
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains(r#"type="image/svg+xml""#));

        // Canvas element
        let canvas_element = symbols.iter().find(|s| s.name == "canvas");
        assert!(canvas_element.is_some());
        assert!(canvas_element
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains(r#"role="img""#));
        assert!(canvas_element
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains(r#"aria-describedby="canvas-description""#));

        // Iframe elements
        let iframe_elements: Vec<_> = symbols.iter().filter(|s| s.name == "iframe").collect();
        assert_eq!(iframe_elements.len(), 2);

        let youtube_iframe = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .map_or(false, |sig| sig.contains("youtube-nocookie.com"))
        });
        assert!(youtube_iframe.is_some());
        assert!(youtube_iframe
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("allowfullscreen"));
        assert!(youtube_iframe
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains(r#"loading="lazy""#));

        let map_iframe = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .map_or(false, |sig| sig.contains("openstreetmap.org"))
        });
        assert!(map_iframe.is_some());

        // Embed element
        let embed_element = symbols.iter().find(|s| s.name == "embed");
        assert!(embed_element.is_some());
        assert!(embed_element
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains(r#"type="application/pdf""#));

        // Custom elements
        let custom_video_player = symbols.iter().find(|s| s.name == "custom-video-player");
        assert!(custom_video_player.is_some());
        assert_eq!(custom_video_player.unwrap().kind, SymbolKind::Class);
        assert!(custom_video_player
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains(r#"controls="true""#));

        let data_visualization = symbols.iter().find(|s| s.name == "data-visualization");
        assert!(data_visualization.is_some());
        assert!(data_visualization
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains(r#"data-source="/api/analytics""#));

        let image_gallery = symbols.iter().find(|s| s.name == "image-gallery");
        assert!(image_gallery.is_some());
        assert!(image_gallery
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains(r#"lazy-loading="true""#));

        // Slot element
        let slot_element = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .map_or(false, |sig| sig.contains(r#"slot="fallback""#))
        });
        assert!(slot_element.is_some());

        // Media-specific attributes
        let loading_lazy: Vec<_> = symbols
            .iter()
            .filter(|s| {
                s.signature
                    .as_ref()
                    .map_or(false, |sig| sig.contains(r#"loading="lazy""#))
            })
            .collect();
        assert!(loading_lazy.len() > 5);

        let decoding_async: Vec<_> = symbols
            .iter()
            .filter(|s| {
                s.signature
                    .as_ref()
                    .map_or(false, |sig| sig.contains(r#"decoding="async""#))
            })
            .collect();
        assert!(decoding_async.len() >= 3);

        let allow_attribute = symbols.iter().find(|s| {
            s.signature
                .as_ref()
                .map_or(false, |sig| sig.contains("allow=\"accelerometer; autoplay"))
        });
        assert!(allow_attribute.is_some());

        // Fallback content
        let video_fallback = symbols.iter().find(|s| {
            s.signature.as_ref().map_or(false, |sig| {
                sig.contains("Your browser doesn't support HTML5 video")
            })
        });
        assert!(video_fallback.is_some());

        let audio_fallback = symbols.iter().find(|s| {
            s.signature.as_ref().map_or(false, |sig| {
                sig.contains("Your browser doesn't support HTML5 audio")
            })
        });
        assert!(audio_fallback.is_some());

        let canvas_fallback = symbols.iter().find(|s| {
            s.signature.as_ref().map_or(false, |sig| {
                sig.contains("Canvas is not supported in your browser")
            })
        });
        assert!(canvas_fallback.is_some());
    }
}

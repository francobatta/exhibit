# Format marks

The logos of the formats a placard names, as vectors from Wikimedia Commons. Each was trimmed to
its ink (the viewBox set to the drawing's bounds) and optimised with `svgo` (multipass, 2-digit
precision, dimensions, classes and inline styles removed). Each is drawn as a mask in the text colour
(`components/FormatMark.tsx`), so only its shape matters. The marks are trademarks of their owners.

| File | Shown for | Commons file | Licence on Commons | Commons' source |
|---|---|---|---|---|
| `ultra-hd.svg` | 4K | [Ultra HD - Logo (transparent).svg](https://commons.wikimedia.org/wiki/File:Ultra_HD_-_Logo_(transparent).svg) | Public domain | heise.de: the Ultra HD logo for devices |
| `hd-1080p.svg` | 1080p | [HD ready 1080p logo.svg](https://commons.wikimedia.org/wiki/File:HD_ready_1080p_logo.svg) | Public domain | EICTA's HD ready 1080p logo; its black square removed |
| `hd-720p.svg` | 720p | [HD ready logo.svg](https://commons.wikimedia.org/wiki/File:HD_ready_logo.svg) | Public domain | EICTA's HD ready logo |
| `sd.svg` | SD | [YouTube standard definition video quality icon.svg](https://commons.wikimedia.org/wiki/File:YouTube_standard_definition_video_quality_icon.svg) | Public domain | YouTube Creator Studio; its empty 24-unit frame removed |
| `hdr10.svg` | HDR10 | [HDR 10 logo (black).svg](https://commons.wikimedia.org/wiki/File:HDR_10_logo_(black).svg) | Public domain | A Commons uploader's drawing: HDR10 is an open standard with no logo of its own |
| `dolby-vision.svg` | Dolby Vision | [Dolby Vision 2021 logo.svg](https://commons.wikimedia.org/wiki/File:Dolby_Vision_2021_logo.svg) | Public domain | Dolby Vision brand guidelines (games.dolby.com) |
| `dolby-truehd.svg` | TrueHD | [Dolby TrueHD.svg](https://commons.wikimedia.org/wiki/File:Dolby_TrueHD.svg) | Public domain | Dolby TrueHD site and white paper |
| `dolby-digital-plus.svg` | E-AC-3 | [Dolby-Digital-Plus.svg](https://commons.wikimedia.org/wiki/File:Dolby-Digital-Plus.svg) | Public domain | Dolby Digital Plus site and white paper |
| `dolby-digital.svg` | AC-3 | [Dolby-Digital.svg](https://commons.wikimedia.org/wiki/File:Dolby-Digital.svg) | Public domain | Dolby Digital site |
| `dts.svg` | DTS | [DTS (2020).svg](https://commons.wikimedia.org/wiki/File:DTS_(2020).svg) | Public domain | dts.com, the 2020 logo |
| `dts-hd-ma.svg` | DTS-HD MA | [DTS-HD-MA.svg](https://commons.wikimedia.org/wiki/File:DTS-HD-MA.svg) | Public domain | English Wikipedia, uploaded by Locke Cole |
| `dts-x.png` | DTS:X | [DTS X B&W.png](https://commons.wikimedia.org/wiki/File:DTS_X_B%26W.png) | CC BY-SA 4.0, by CinemaLover24680 | Their drawing of the DTS:X print logo. Changed here: its darkness made its alpha, cropped to the ink, scaled to 160 px high. No vector exists on Commons |
| `dolby-atmos.svg` | Atmos | [Logo Dolby Atmos.svg](https://commons.wikimedia.org/wiki/File:Logo_Dolby_Atmos.svg) | Public domain | Dolby's consumer movie technologies page; the stacked mark, like the other Dolby marks here |

The HD ready marks were made for displays, not for video. They are the marks people know for
those sizes, and EICTA's successor, DIGITALEUROPE, also made the Ultra HD mark, so the three sizes
read as one family. HLG has no mark and stays in words. The sound mark names the best format among the file's tracks;
mpv tells DTS-HD MA, DTS:X and Atmos apart from their core formats by the profile it reads once a
track's decoder is open (see `probe.rs`).

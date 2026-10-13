# Image attribution

Every image under `docs/images/` is a screenshot of PhotoCraft (MIT OR Apache-2.0, by the
ArtCraft Team) converting an openly licensed photograph of a film negative. The photographs:

| Path pattern | Title | Author | Source | Licence | Changes |
|---|---|---|---|---|---|
| `docs/images/0*.jpg`, `docs/images/flow.gif` | *Color print film strip 01 - negative* (a strip of Kodak Ektar 100, photographed against daylight) | [LoMit](https://commons.wikimedia.org/wiki/User:LoMit) | [Wikimedia Commons](https://commons.wikimedia.org/wiki/File:Color_print_film_strip_01_-_negative.jpg) | [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/) | one frame cropped out (frame 22, with the unexposed gaps beside it), converted to a positive by this plug-in, shown inside PhotoCraft |

The author's own positive of the same strip, used as the reference the conversion is compared
with, is [*Color print film strip 03 - positive with tonal correction*](https://commons.wikimedia.org/wiki/File:Color_print_film_strip_03_-_positive_with_tonal_correction.jpg)
(LoMit, CC BY 4.0). A second openly licensed negative, a CC0 scan of a 110 frame by
[Laerol](https://commons.wikimedia.org/wiki/User:Laerol)
([*110 format negative closeup of frame Ericht at Blairgowrie*](https://commons.wikimedia.org/wiki/File:110_format_negative_closeup_of_frame_Ericht_at_Blairgowrie.jpg)),
is fetched by the same script for trying the auto analysis.

The originals are not committed; `scripts/fetch-samples.sh` downloads them into `corpus/`
(ignored by git) and verifies their SHA-256. `scripts/screenshots.sh` regenerates every image
in this folder from them.

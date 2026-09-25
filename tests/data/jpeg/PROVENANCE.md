# tests/data/jpeg -- the test pictures of lib/jpeg (OpenPlan LIB-010)

Made here on 25 September 2026 with Pillow 12 (libjpeg-turbo) from one
synthetic 123 x 77 picture (random ellipses and a pixel pattern, drawn by
tools/libmvp/check_jpeg.py's generator, seed 1). No third-party content.

| file | what it exercises |
|---|---|
| base444.jpg | baseline, 4:4:4 |
| base420.jpg | baseline, 4:2:0 (fancy h2v2 upsampling) |
| prog420.jpg | progressive, 4:2:0 -- the same pixels as base420.jpg |
| gray.jpg | one component |
| rst.jpg | restart markers every 3 MCUs |
| exif6.jpg | EXIF orientation 6 (the result is 77 x 123) |
| cmyk.jpg | four components, Adobe APP14 |
| q10.jpg | quality 10 (large coefficients, clamping) |

tests/1916_jpeg.fi checks the CRC-32 of the decoded RGBA against the CRC-32
of Pillow's RGBA for the same file.

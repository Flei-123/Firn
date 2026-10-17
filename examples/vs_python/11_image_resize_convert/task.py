from PIL import Image

im = Image.open("in.jpg")
small = im.resize((im.width // 2, im.height // 2), Image.BILINEAR)
small.save("out.png")
small.save("out.webp", lossless=True)
back = Image.open("out.png").convert("RGBA")
print(f"in {im.width}x{im.height}")
print(f"out {back.width}x{back.height}")

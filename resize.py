from PIL import Image
img = Image.open('assets/app_icon.jpg')
img = img.resize((64, 64), Image.Resampling.LANCZOS)
img.save('assets/icon.png', 'PNG')
img.save('assets/icon.ico', format='ICO', sizes=[(16,16), (32,32), (48,48), (64,64), (128,128), (256,256)])
print('Saved icon.png and icon.ico successfully')
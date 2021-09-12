import time
start = time.perf_counter()
import io
import os
import sys
from datetime import datetime

import torch, torchvision
from torchvision import transforms
from PIL import Image, ImageDraw
sys.stderr.write('imports\t\t%.3fs\n' % (time.perf_counter() - start))

torch.hub.set_dir('torch')
model = torchvision.models.detection.maskrcnn_resnet50_fpn(pretrained=True)
model.eval()
sys.stderr.write('init model \t%.3fs\n' % (time.perf_counter() - start))

img_path = 'PennFudanPed/PNGImages/FudanPed00001.png'
sys.stderr.write('read img \t%.3fs (%s)\n' % (time.perf_counter() - start, img_path))
img = Image.open(img_path).convert("RGB")
img_tensor = transforms.ToTensor()(img)
sys.stderr.write('init img \t%.3fs (%s)\n' % (time.perf_counter() - start, img_path))

with torch.no_grad():
    output = model([img_tensor])[0]
sys.stderr.write('inference \t%.3fs\n' % (time.perf_counter() - start))

# sys.stderr.write('{}\n'.format(output['boxes']))
# sys.stderr.write('{}\n'.format(output['labels']))
# sys.stderr.write('{}\n'.format(output['scores']))

# sys.stderr.write('Filtering by confidence > 0.6 and label == 1 (person)\n')
boxes = output['boxes'].tolist()
boxes_filtered = []
for i in range(len(output['labels'])):
    if output['scores'][i] > 0.6 and output['labels'][i] == 1:
        boxes_filtered.append(boxes[i])

draw = ImageDraw.Draw(img)
for box in boxes_filtered:
    print(box)
    draw.rectangle(box, outline="yellow", width=4)
img_byte_arr = io.BytesIO()
img.save(img_byte_arr, format='PNG')
img_byte_arr = img_byte_arr.getvalue()
sys.stderr.write('prepare results \t%.3fs\n' % (time.perf_counter() - start))

sys.stderr.write(bytes([len(boxes_filtered)]))
sys.stderr.write("training_data", img_byte_arr)
sys.stderr.write('send training_data \t%.3fs\n' % (time.perf_counter() - start))
sys.stderr.write('log output \t%.3fs\n' % (time.perf_counter() - start))

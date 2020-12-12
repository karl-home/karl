import sys
import time

start = time.perf_counter()
import torch, torchvision
from torchvision import transforms
from PIL import Image
sys.stderr.write('imports\t\t%.3fs\n' % (time.perf_counter() - start))

torch.hub.set_dir('torch')
model = torchvision.models.detection.maskrcnn_resnet50_fpn(pretrained=True)
model.eval()
sys.stderr.write('init model \t%.3fs\n' % (time.perf_counter() - start))

if len(sys.argv) > 1:
    img_path = sys.argv[1]
else:
    img_path = 'PennFudanPed/PNGImages/FudanPed00001.png'
img = Image.open(img_path).convert("RGB")
img_tensor = transforms.ToTensor()(img)
sys.stderr.write('init img \t%.3fs (%s)\n' % (time.perf_counter() - start, img_path))

with torch.no_grad():
    output = model([img_tensor])[0]
sys.stderr.write('inference \t%.3fs\n' % (time.perf_counter() - start))

sys.stderr.write('{}\n'.format(output['boxes']))
sys.stderr.write('{}\n'.format(output['labels']))
sys.stderr.write('{}\n'.format(output['scores']))

sys.stderr.write('Filtering by confidence > 0.6 and label == 1 (person)\n')
boxes = output['boxes'].tolist()
for i in range(len(output['labels'])):
    if output['scores'][i] > 0.6 and output['labels'][i] == 1:
        print(boxes[i])
sys.stderr.write('log output \t%.3fs\n' % (time.perf_counter() - start))

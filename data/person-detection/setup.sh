# Setup Python virtual environment
virtualenv -p python3 env
source env/bin/activate
pip install grpcio grpcio-tools
pip install torch torchvision

# Generate the gRPC wrappers
python -m grpc_tools.protoc -I../../protos \
    --python_out=. --grpc_python_out=. ../../protos/request.proto

# Download the PennFudan dataset
wget https://www.cis.upenn.edu/~jshi/ped_html/PennFudanPed.zip
unzip PennFudanPed.zip
rm PennFudanPed.zip

# Download the maskrcnn model
mkdir -p torch/checkpoints
wget https://download.pytorch.org/models/maskrcnn_resnet50_fpn_coco-bf2d0c1e.pth
mv maskrcnn_resnet50_fpn_coco-bf2d0c1e.pth torch/checkpoints/

# Deactivate
deactivate

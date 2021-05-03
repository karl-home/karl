g++ -std=c++11 -Wno-expansion-to-defined Register.cpp Karl.cpp protos/request.pb.cc -o register-linux $(pkg-config --libs protobuf)
g++ -std=c++11 -Wno-expansion-to-defined PersonDetection.cpp Karl.cpp protos/request.pb.cc -o person_detection-linux $(pkg-config --libs protobuf)

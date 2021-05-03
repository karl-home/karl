echo "WARNING: need GNU tar with --transform feature, do not use AddFileAs or AddDirAs"
g++ -std=c++11 -Wno-expansion-to-defined Register.cpp Karl.cpp protos/request.pb.cc -o register-macos -lprotobuf
g++ -std=c++11 -Wno-expansion-to-defined PersonDetection.cpp Karl.cpp protos/request.pb.cc -o person_detection-macos -lprotobuf